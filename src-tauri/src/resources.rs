//! Bounded HTTPS requests, no redirects, fixed provider hosts, shared cooldown.
use std::{
    io::Read,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};
static REQUESTS: Mutex<Option<Instant>> = Mutex::new(None);
pub fn read_text(path: &Path, max: usize) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Selecciona un archivo".into());
    }
    let mut raw = String::new();
    file.take(max as u64 + 1)
        .read_to_string(&mut raw)
        .map_err(|_| "El archivo debe ser UTF-8")?;
    if raw.len() > max {
        return Err("Archivo demasiado grande".into());
    }
    Ok(raw)
}
pub fn get(url: reqwest::Url, max: usize) -> Result<Vec<u8>, String> {
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
        || !(host == "lrclib.net" || host == "archive.org" || host.ends_with(".archive.org")
            || host == "itunes.apple.com" || host == "music.apple.com"
            || host.ends_with(".mzstatic.com") || host == "i.ytimg.com")
    {
        return Err("Proveedor HTTPS no autorizado".into());
    }
    let apple_page = host == "music.apple.com";
    let expected_album = if apple_page { album_identity(&url) } else { None };
    let mut until = REQUESTS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(t) = *until
        && t > Instant::now()
        && t.duration_since(Instant::now()) <= Duration::from_millis(500)
    {
        std::thread::sleep(t.saturating_duration_since(Instant::now()));
    }
    if until.is_some_and(|t| t > Instant::now()) {
        return Err("El proveedor pide esperar antes de volver a consultar".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(if apple_page { 7 } else { 15 }))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 3 {
                return attempt.error("Demasiadas redirecciones");
            }
            let url = attempt.url();
            if let Some(expected) = &expected_album
                && (url.host_str() != Some("music.apple.com") || album_identity(url).as_ref() != Some(expected)) {
                return attempt.error("La redirección cambió el álbum de Apple Music");
            }
            if url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none_or(|p| p == 443)
                && url
                    .host_str()
                    .is_some_and(|h| h == "archive.org" || h.ends_with(".archive.org")
                        || h == "music.apple.com" || h == "itunes.apple.com"
                        || h.ends_with(".mzstatic.com"))
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("ZenithPlayer/0.1.0 (https://github.com/getzenithdev/player)")
        .build()
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let mut request = client.get(url);
        if apple_page {
            request = request.header("Accept", "text/html").header("Accept-Language", "en-US,en;q=0.8");
        }
        let response = request
            .send()
            .map_err(|_| "Proveedor sin conexión o no disponible")?;
        if response.status().as_u16() == 429 || response.status().as_u16() == 503 {
            let wait = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60)
                .max(1);
            *until = Some(Instant::now() + Duration::from_secs(wait.min(86400) + 1));
            return Err("Proveedor ocupado. Intenta más tarde".into());
        }
        let response = response
            .error_for_status()
            .map_err(|e| format!("Proveedor: {}", e.status().map(|s| s.as_u16()).unwrap_or(0)))?;
        if response.content_length().is_some_and(|n| n > max as u64) {
            return Err("Recurso demasiado grande".into());
        }
        let mut bytes = Vec::new();
        response
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Descarga interrumpida")?;
        if bytes.len() > max {
            return Err("Recurso demasiado grande".into());
        }
        Ok(bytes)
    })();
    if until.is_none_or(|t| t <= Instant::now()) {
        *until = Some(Instant::now() + Duration::from_millis(500));
    }
    result
}

fn album_identity(url: &reqwest::Url) -> Option<(String, String)> {
    let parts: Vec<_> = url.path_segments()?.filter(|part| !part.is_empty()).collect();
    if !(parts.len() == 3 || parts.len() == 4) || parts[1] != "album" { return None; }
    let id = *parts.last()?;
    if !id.bytes().all(|c| c.is_ascii_digit()) { return None; }
    Some((parts[0].to_string(), id.to_string()))
}
