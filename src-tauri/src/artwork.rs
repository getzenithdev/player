//! Artwork proposals, validated content-addressed assets and offline resolution.
use crate::{
    library, metadata, resources,
    storage::{StorageManager, indexes::TrackEntry},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{AnimationDecoder, ImageDecoder, ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
const DOCUMENT: &str = "library/artwork.json";
const MAX_IMAGE: usize = 15 * 1024 * 1024;
mod apple;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source_url: Option<String>,
    pub license: String,
    pub attribution: String,
    pub saved_at: u64,
    pub width: u32,
    pub height: u32,
    pub animated: bool,
    #[serde(default)]
    pub motion_url: Option<String>,
    pub score: u32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Preferences {
    pub external: bool,
    pub autoplay: bool,
    pub looping: bool,
    pub battery_saver: bool,
    pub quality: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            external: false,
            autoplay: true,
            looping: true,
            battery_saver: false,
            quality: "preview".into(),
        }
    }
}
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct Document {
    tracks: BTreeMap<String, Vec<Asset>>,
    selected: BTreeMap<String, String>,
    preferences: Preferences,
    apple_checked: BTreeMap<String, (String, u64)>,
    apple_warnings: BTreeMap<String, Vec<String>>,
    artist_images: BTreeMap<String, String>,
    artist_apple_images: BTreeMap<String, String>,
    artist_apple_checked: BTreeMap<String, u64>,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub identity: String,
    pub resolved: Option<Asset>,
    pub candidates: Vec<Asset>,
    pub preferences: Preferences,
    pub warnings: Vec<String>,
}
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub provider: String,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source_url: Option<String>,
    pub license: String,
    pub attribution: String,
    pub permitted_storage: bool,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Snapshot,
    Discover { #[serde(default)] force: bool },
    Local,
    Import { path: String },
    ImportManifest { path: String },
    Search { query: Option<String> },
    Archive { identifier: String },
    Select { id: String },
    Remove,
    Preferences { preferences: Preferences },
    Export { path: String, id: String },
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn load(s: &StorageManager) -> Result<Document, String> {
    s.load_document::<Document>(DOCUMENT)
        .map(|d| d.map(|d| d.data).unwrap_or_default())
        .map_err(|e| e.to_string())
}
/// Global presentation preferences, shared with the settings center (phase 13).
pub fn load_preferences(s: &StorageManager) -> Result<Preferences, String> {
    Ok(load(s)?.preferences)
}
/// Stores the global preferences without touching saved artwork.
pub fn save_preferences(s: &StorageManager, preferences: Preferences) -> Result<(), String> {
    if !["thumbnail", "preview"].contains(&preferences.quality.as_str()) {
        return Err("Calidad inválida".into());
    }
    let _lock = s.library_lock();
    let mut doc = load(s)?;
    doc.preferences = preferences;
    s.save_document(DOCUMENT, &doc).map_err(|e| e.to_string())
}
fn bounded_file(path: &Path) -> Result<Vec<u8>, String> {
    let f = fs::File::open(path).map_err(|e| e.to_string())?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Selecciona una imagen".into());
    }
    let mut bytes = Vec::new();
    f.take(MAX_IMAGE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_IMAGE {
        return Err("Imagen mayor a 15 MiB".into());
    }
    Ok(bytes)
}

/// Artist portraits are separate from album covers and survive application restarts.
#[tauri::command]
pub async fn artist_artwork(
    app: tauri::AppHandle,
    artist_key: String,
    track_id: Option<String>,
    path: Option<String>,
    artist_name: Option<String>,
) -> Result<Option<String>, String> {
    use tauri::{Emitter, Manager};
    tauri::async_runtime::spawn_blocking(move || {
        if artist_key.trim().is_empty() || artist_key.len() > 4096
            || artist_name.as_ref().is_some_and(|name| name.len() > 4096) {
            return Err("Artista inválido".into());
        }
        let s = app.state::<StorageManager>();
        let (saved, apple_saved, checked) = {
            let _lock = s.library_lock();
            let doc = load(&s)?;
            (doc.artist_images.get(&artist_key).cloned(),
             doc.artist_apple_images.get(&artist_key).cloned(),
             doc.artist_apple_checked.get(&artist_key).copied().unwrap_or(0))
        };
        if path.is_none()
            && let Some(id) = saved.as_ref().or(apple_saved.as_ref())
            && let Ok(bytes) = bounded_file(&image_path(&s, id, "original")?) {
            return Ok(Some(format!("data:image/png;base64,{}", STANDARD.encode(bytes))));
        }
        let track = if let Some(track_id) = track_id.as_ref() {
            let _lock = s.library_lock();
            library::load(&s)?.tracks.into_iter().find(|track| &track.id == track_id)
        } else { None };
        let source = if let Some(path) = path {
            Some(std::path::PathBuf::from(path))
        } else {
            track.as_ref().and_then(|track| track.local_path.as_deref())
                .and_then(|path| Path::new(path).parent())
                .and_then(|directory| ["artist.jpg", "artist.png", "artist.webp"]
                    .iter().map(|name| directory.join(name)).find(|path| path.is_file()))
        };
        let remote = source.is_none();
        let bytes = if let Some(source) = source {
            bounded_file(&source)?
        } else {
            let allowed = load_preferences(&s)?.external || crate::sources::external_enabled(&s);
            if !allowed || checked > now() { return Ok(None); }
            let name = artist_name.as_deref().filter(|name| !name.trim().is_empty())
                .or_else(|| track.as_ref().and_then(|track| track.artist.as_deref()));
            let Some(name) = name else { return Ok(None); };
            let apple_id = track.as_ref()
                .filter(|track| metadata::key(track.artist.as_deref().unwrap_or_default()) == metadata::key(name))
                .and_then(|track| track.metadata.identifiers.provider_artist_ids.get("apple-music"))
                .map(String::as_str);
            let result = apple::artist_portrait(name, apple_id);
            if !matches!(&result, Ok(Some(_))) {
                let _lock = s.library_lock();
                let mut doc = load(&s)?;
                doc.artist_apple_checked.insert(artist_key.clone(), now() + if result.is_err() { 60 } else { 1800 });
                s.save_document(DOCUMENT, &doc).map_err(|error| error.to_string())?;
            }
            match result { Ok(Some(bytes)) => bytes, Ok(None) => return Ok(None), Err(error) => return Err(error) }
        };
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format().map_err(|error| error.to_string())?;
        reader.limits(limits());
        let photo = reader.decode().map_err(|_| "No se pudo leer la foto del artista")?
            .thumbnail(960, 960);
        let bytes = encode_png(&photo)?;
        let id = format!("{:x}.png", Sha256::digest(&bytes));
        {
            let _lock = s.library_lock();
            let mut doc = load(&s)?;
            // A manual selection made during the network request always wins.
            if remote && let Some(manual) = doc.artist_images.get(&artist_key)
                && let Ok(bytes) = bounded_file(&image_path(&s, manual, "original")?) {
                return Ok(Some(format!("data:image/png;base64,{}", STANDARD.encode(bytes))));
            }
            s.write_artwork_resource(&image_path(&s, &id, "original")?, &bytes)
                .map_err(|error| error.to_string())?;
            if remote { doc.artist_apple_images.insert(artist_key.clone(), id); }
            else { doc.artist_images.insert(artist_key.clone(), id); }
            s.save_document(DOCUMENT, &doc).map_err(|error| error.to_string())?;
        }
        let _ = app.emit("artist:changed", &artist_key);
        Ok(Some(format!("data:image/png;base64,{}", STANDARD.encode(bytes))))
    }).await.map_err(|error| error.to_string())?
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits
}
fn encode_png(image: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}
fn square_cover(image: &image::DynamicImage, maximum: u32) -> image::DynamicImage {
    let side = image.width().min(image.height());
    image.crop_imm((image.width() - side) / 2, (image.height() - side) / 2, side, side)
        .thumbnail(maximum, maximum)
}
fn valid_id(id: &str) -> bool {
    id.split_once('.').is_some_and(|(hash, ext)| {
        hash.len() == 64
            && hash.bytes().all(|b| b.is_ascii_hexdigit())
            && ["png", "jpg", "webp", "gif"].contains(&ext)
    })
}
fn image_path(s: &StorageManager, id: &str, variant: &str) -> Result<std::path::PathBuf, String> {
    if !valid_id(id) || !["original", "thumbnail", "preview"].contains(&variant) {
        return Err("Recurso inválido".into());
    }
    Ok(s.document_path(&format!(
        "artwork/resources/{id}{}",
        match variant {
            "thumbnail" => ".thumb.png",
            "preview" => ".preview.png",
            _ => "",
        }
    )))
}
fn store(
    s: &StorageManager,
    bytes: &[u8],
    m: Manifest,
    track: &TrackEntry,
) -> Result<Asset, String> {
    if !m.permitted_storage
        || m.version != 1
        || !["local", "apple-music", "archive-org", "youtube"].contains(&m.provider.as_str())
        || m.license.trim().is_empty()
        || m.attribution.len() > 4096
        || m.license.len() > 4096
        || m.title.len() > 4096
        || m.album.len() > 4096
        || m.artist.len() > 4096
    {
        return Err("Falta autorización o procedencia válida para guardar la portada".into());
    }
    if m.provider != "local"
        && (m.attribution.trim().is_empty()
            || !m.source_url.as_deref().is_some_and(|u| {
                reqwest::Url::parse(u).is_ok_and(|u| {
                    u.scheme() == "https" && u.username().is_empty() && u.password().is_none()
                })
            }))
    {
        return Err("El proveedor requiere fuente HTTPS y atribución".into());
    }
    if bytes.len() > MAX_IMAGE {
        return Err("Imagen demasiado grande".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "No es una imagen compatible")?;
    let ext = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::WebP => "webp",
        ImageFormat::Gif => "gif",
        _ => return Err("Usa PNG, JPEG, WebP o GIF".into()),
    };
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    let decoded = reader
        .decode()
        .map_err(|_| "Imagen dañada o con dimensiones excesivas")?;
    let width = decoded.width();
    let height = decoded.height();
    if width < 32 || height < 32 {
        return Err("La portada debe medir al menos 32 × 32 píxeles".into());
    }
    let animated = if format == ImageFormat::Gif {
        decode_frames(bytes)?.len() > 1
    } else {
        false
    };
    let id = format!("{:x}.{ext}", Sha256::digest(bytes));
    let thumb = encode_png(&if m.provider == "youtube" { square_cover(&decoded, 256) }
        else { decoded.thumbnail(256, 256) })?;
    let preview = encode_png(&if m.provider == "youtube" { square_cover(&decoded, 1024) }
        else { decoded.thumbnail(1024, 1024) })?;
    s.write_artwork_resource(&image_path(s, &id, "original")?, bytes)
        .map_err(|e| e.to_string())?;
    s.write_artwork_resource(&image_path(s, &id, "thumbnail")?, &thumb)
        .map_err(|e| e.to_string())?;
    s.write_artwork_resource(&image_path(s, &id, "preview")?, &preview)
        .map_err(|e| e.to_string())?;
    let mut score = 0;
    if metadata::key(&m.artist) == metadata::key(track.artist.as_deref().unwrap_or_default())
        && !m.artist.is_empty()
    {
        score += 40;
    }
    if metadata::key(&m.album) == metadata::key(track.album.as_deref().unwrap_or_default())
        && !m.album.is_empty()
    {
        score += 40;
    }
    if metadata::key(&m.title) == metadata::key(&track.title) {
        score += 20;
    }
    Ok(Asset {
        id,
        provider: m.provider,
        title: m.title,
        artist: m.artist,
        album: m.album,
        source_url: m.source_url,
        license: m.license,
        attribution: m.attribution,
        saved_at: now(),
        width,
        height,
        animated,
        motion_url: None,
        score,
    })
}
fn local_manifest(path: String, t: &TrackEntry) -> Manifest {
    Manifest {
        version: 1,
        provider: "local".into(),
        path,
        title: t.title.clone(),
        artist: t.artist.clone().unwrap_or_default(),
        album: t.album.clone().unwrap_or_default(),
        source_url: None,
        license: "User supplied local resource".into(),
        attribution: "Archivo local".into(),
        permitted_storage: true,
    }
}
#[cfg(test)]
pub fn priority(a: &Asset) -> u8 {
    match (a.provider.as_str(), a.animated) {
        ("apple-music", true) => 0,
        ("apple-music", false) => 1,
        ("archive-org", _) => 2,
        ("youtube", _) => 3,
        _ => 4,
    }
}
fn configured_priority(
    asset: &Asset,
    providers: &[String],
) -> (bool, usize, std::cmp::Reverse<u32>) {
    (
        !asset.animated,
        providers
            .iter()
            .position(|provider| provider == &asset.provider)
            .unwrap_or(providers.len()),
        std::cmp::Reverse(asset.score),
    )
}
fn resolve(s: &StorageManager, doc: &Document, track: &TrackEntry) -> Option<Asset> {
    let assets = doc.tracks.get(&track.id)?;
    let available = |a: &&Asset| image_path(s, &a.id, "preview").is_ok_and(|p| p.is_file());
    if let Some(id) = doc.selected.get(&track.id)
        && let Some(a) = assets.iter().filter(available).find(|a| &a.id == id)
    {
        if a.animated { return Some(a.clone()); }
    }
    let providers = crate::settings::load(s)
        .ok()?
        .settings
        .artwork
        .provider_priority;
    let animated = assets.iter().filter(available).filter(|asset| asset.animated)
        .min_by_key(|asset| configured_priority(asset, &providers));
    if let Some(asset) = animated { return Some(asset.clone()); }
    if let Some(selected) = doc.selected.get(&track.id)
        && let Some(asset) = assets.iter().filter(available).find(|asset| &asset.id == selected) {
        return Some(asset.clone());
    }
    assets
        .iter()
        .filter(available)
        .min_by_key(|a| configured_priority(a, &providers))
        .cloned()
}
fn local(s: &StorageManager, t: &TrackEntry) -> Vec<Asset> {
    let mut result = Vec::new();
    if let Some(id) = &t.artwork_id
        && let Ok(bytes) = bounded_file(&s.document_path(&format!(
            "artwork/local/{}",
            if valid_id(id) { id.as_str() } else { "invalid" }
        )))
        && let Ok(asset) = store(s, &bytes, local_manifest(String::new(), t), t)
    {
        result.push(asset);
    }
    if let Some(path) = t.local_path.as_deref() {
        let path = Path::new(path);
        let folder = path.parent().unwrap_or(Path::new("."));
        let mut paths = vec![path.with_extension("jpg"), path.with_extension("png")];
        for stem in ["cover", "folder", "front"] {
            for ext in ["jpg", "png", "webp", "gif"] {
                paths.push(folder.join(format!("{stem}.{ext}")));
            }
        }
        for path in paths {
            if let Ok(bytes) = bounded_file(&path)
                && let Ok(a) = store(
                    s,
                    &bytes,
                    local_manifest(path.to_string_lossy().into(), t),
                    t,
                )
                && !result.iter().any(|x| x.id == a.id)
            {
                result.push(a);
            }
        }
    }
    result
}
#[derive(Serialize)]
pub struct AnimationFrame {
    pub url: String,
    pub delay: u32,
}
fn decode_frames(bytes: &[u8]) -> Result<Vec<AnimationFrame>, String> {
    let mut decoder =
        image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).map_err(|_| "GIF inválido")?;
    decoder
        .set_limits(limits())
        .map_err(|_| "GIF demasiado grande")?;
    let mut frames = Vec::new();
    let mut total = 0u64;
    for frame in decoder.into_frames().take(121) {
        if frames.len() >= 120 {
            return Err("La animación supera 120 cuadros".into());
        }
        let frame = frame.map_err(|_| "GIF dañado")?;
        total += u64::from(frame.buffer().width()) * u64::from(frame.buffer().height()) * 4;
        if total > 64 * 1024 * 1024 {
            return Err("La animación supera el límite de memoria".into());
        }
        let (n, d) = frame.delay().numer_denom_ms();
        let delay = n.checked_div(d).unwrap_or(100).clamp(40, 10000);
        let data =
            encode_png(&image::DynamicImage::ImageRgba8(frame.into_buffer()).thumbnail(512, 512))?;
        frames.push(AnimationFrame {
            url: format!("data:image/png;base64,{}", STANDARD.encode(data)),
            delay,
        });
    }
    Ok(frames)
}
#[tauri::command]
pub async fn artwork_frames(
    app: tauri::AppHandle,
    id: String,
) -> Result<Vec<AnimationFrame>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let s = app.state::<StorageManager>();
        let doc = load(&s)?;
        if !doc
            .tracks
            .values()
            .flatten()
            .any(|a| a.id == id && a.animated)
        {
            return Err("Animación no guardada".into());
        }
        decode_frames(&bounded_file(&image_path(&s, &id, "original")?)?)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn artwork_data(
    app: tauri::AppHandle,
    id: String,
    variant: String,
) -> Result<String, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let s = app.state::<StorageManager>();
        let target = image_path(&s, &id, &variant)?;
        let mut bytes = bounded_file(&target)?;
        // Upgrade older rectangular YouTube previews only when first used.
        // Originals stay untouched for export and future recropping.
        if variant != "original" {
            let rectangular = ImageReader::new(Cursor::new(bytes.as_slice()))
                .with_guessed_format().ok()
                .and_then(|reader| reader.into_dimensions().ok())
                .is_some_and(|(width, height)| width != height);
            if rectangular {
                let youtube = {
                    let _lock = s.library_lock();
                    load(&s)?.tracks.values().flatten()
                        .any(|asset| asset.id == id && asset.provider == "youtube")
                };
                if youtube {
                    let original = bounded_file(&image_path(&s, &id, "original")?)?;
                    let mut reader = ImageReader::new(Cursor::new(original))
                        .with_guessed_format().map_err(|error| error.to_string())?;
                    reader.limits(limits());
                    let image = reader.decode().map_err(|error| error.to_string())?;
                    bytes = encode_png(&square_cover(&image, if variant == "thumbnail" { 256 } else { 1024 }))?;
                    // A read-only library can still display the cropped image.
                    let _ = s.write_artwork_resource(&target, &bytes);
                }
            }
        }
        let mime = if variant != "original" {
            "png"
        } else {
            match id.rsplit('.').next().unwrap_or_default() {
                "jpg" => "jpeg",
                ext => ext,
            }
        };
        Ok(format!(
            "data:image/{mime};base64,{}",
            STANDARD.encode(bytes)
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}
fn archive_search(t: &TrackEntry, query: Option<String>) -> Result<Vec<Asset>, String> {
    let terms = query.filter(|q| !q.trim().is_empty()).unwrap_or_else(|| {
        format!(
            "{} {}",
            t.artist.as_deref().unwrap_or_default(),
            t.album.as_deref().unwrap_or(&t.title)
        )
    });
    if terms.len() > 500 {
        return Err("Búsqueda demasiado larga".into());
    }
    let terms = terms
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>();
    let mut url = reqwest::Url::parse("https://archive.org/advancedsearch.php").unwrap();
    url.query_pairs_mut()
        .append_pair("q", &format!("mediatype:audio AND ({terms})"))
        .append_pair("output", "json")
        .append_pair("rows", "8")
        .append_pair("fl[]", "identifier,title,creator,licenseurl");
    let value: serde_json::Value = serde_json::from_slice(&resources::get(url, 1024 * 1024)?)
        .map_err(|_| "Respuesta Archive.org inválida")?;
    Ok(value["response"]["docs"]
        .as_array()
        .ok_or("Respuesta Archive.org inválida")?
        .iter()
        .filter_map(|r| {
            let identifier = r["identifier"].as_str()?;
            if !safe_identifier(identifier) {
                return None;
            }
            let title = r["title"].as_str().unwrap_or_default().to_string();
            let artist = r["creator"]
                .as_str()
                .or_else(|| r["creator"][0].as_str())
                .unwrap_or_default()
                .to_string();
            let license = r["licenseurl"].as_str().unwrap_or_default().to_string();
            let mut score = 0;
            if metadata::key(&artist) == metadata::key(t.artist.as_deref().unwrap_or_default())
                && !artist.is_empty()
            {
                score += 40;
            }
            if metadata::key(&title) == metadata::key(t.album.as_deref().unwrap_or(&t.title)) {
                score += 40;
            }
            Some(Asset {
                id: format!("archive:{identifier}"),
                provider: "archive-org".into(),
                title,
                artist,
                album: String::new(),
                source_url: Some(format!("https://archive.org/details/{identifier}")),
                license,
                attribution: "Internet Archive — revisar identidad y licencia".into(),
                saved_at: 0,
                width: 0,
                height: 0,
                animated: false,
                motion_url: None,
                score,
            })
        })
        .collect())
}
fn safe_identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && id != "."
        && id != ".."
}
fn archive(s: &StorageManager, t: &TrackEntry, id: &str) -> Result<Asset, String> {
    if !safe_identifier(id) {
        return Err("Identificador Archive.org inválido".into());
    }
    let url = reqwest::Url::parse(&format!("https://archive.org/metadata/{id}")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&resources::get(url, 2 * 1024 * 1024)?)
        .map_err(|_| "Metadatos de Archive.org inválidos")?;
    let m = &value["metadata"];
    let license = m["licenseurl"]
        .as_str()
        .or_else(|| m["licenseurl"][0].as_str())
        .unwrap_or_default();
    if !permitted_license(license) {
        return Err("Se requiere una licencia Creative Commons reconocida. Puedes importar una copia autorizada manualmente".into());
    }
    let file = value["files"]
        .as_array()
        .ok_or("El elemento no tiene archivos")?
        .iter()
        .find_map(|f| {
            let name = f["name"].as_str()?;
            let lower = name.to_lowercase();
            if [".jpg", ".jpeg", ".png", ".webp"]
                .iter()
                .any(|ext| lower.ends_with(ext))
                && !name.contains('/')
                && !name.contains('\\')
            {
                Some(name)
            } else {
                None
            }
        })
        .ok_or("No se encontró una imagen estática")?;
    let mut url = reqwest::Url::parse("https://archive.org/download/").unwrap();
    url.path_segments_mut()
        .map_err(|_| "URL inválida")?
        .pop_if_empty()
        .push(id)
        .push(file);
    let bytes = resources::get(url, MAX_IMAGE)?;
    let title = m["title"].as_str().unwrap_or_default();
    let artist = m["creator"]
        .as_str()
        .or_else(|| m["creator"][0].as_str())
        .unwrap_or_default();
    store(
        s,
        &bytes,
        Manifest {
            version: 1,
            provider: "archive-org".into(),
            path: String::new(),
            title: title.into(),
            artist: artist.into(),
            album: title.into(),
            source_url: Some(format!("https://archive.org/details/{id}")),
            license: license.into(),
            attribution: format!("{artist} · {title} · Internet Archive · {license}"),
            permitted_storage: true,
        },
        t,
    )
}
fn permitted_license(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("creativecommons.org")
            && [
                "/licenses/by/",
                "/licenses/by-sa/",
                "/licenses/by-nc/",
                "/licenses/by-nc-sa/",
                "/licenses/by-nd/",
                "/licenses/by-nc-nd/",
                "/publicdomain/zero/",
                "/publicdomain/mark/",
            ]
            .iter()
            .any(|p| u.path().starts_with(p))
    })
}
pub fn execute(s: &StorageManager, track_id: String, action: Action) -> Result<Snapshot, String> {
    let track = {
        let _lock = s.library_lock();
        library::load(s)?
            .tracks
            .into_iter()
            .find(|t| t.id == track_id)
            .ok_or("Selecciona una canción de la biblioteca")?
    };
    let mut assets = Vec::new();
    let mut search = None;
    let mut warnings = Vec::new();
    match &action {
        Action::Local => {
            assets = local(s, &track);
        }
        Action::Snapshot if load(s)?.tracks.get(&track_id).is_none_or(|a| a.is_empty()) => {
            assets = local(s, &track);
        }
        Action::Discover { force } => {
            let fingerprint = apple::identity(&track);
            let (should_search, existing) = {
                let _lock = s.library_lock();
                let doc = load(s)?;
                let allowed = doc.preferences.external || crate::sources::external_enabled(s);
                let recent = doc.apple_checked.get(&track_id).is_some_and(|(key, expires)|
                    key == &fingerprint && *expires > now());
                let existing = doc.tracks.get(&track_id).and_then(|assets| assets.iter()
                    .filter(|asset| asset.provider == "apple-music" && image_path(s, &asset.id, "preview").is_ok_and(|path| path.is_file()))
                    .max_by_key(|asset| (asset.animated, asset.saved_at))).cloned();
                let recent = recent && (existing.is_some() || doc.apple_warnings.get(&track_id)
                    .is_some_and(|warnings| !warnings.is_empty()));
                (allowed && (*force || !recent), existing)
            };
            if should_search {
                let ttl = match apple::search(s, &track, None, existing.as_ref(), *force) {
                    Ok(Some(found)) => {
                        let ttl = if found.warning.is_some() { 60 }
                            else if found.asset.animated { 6 * 3600 } else { 30 * 60 };
                        if let Some(warning) = found.warning { warnings.push(warning); }
                        if !found.asset.animated && warnings.is_empty() {
                            warnings.push("Apple Music no devolvió una portada animada para esta edición del álbum".into());
                        }
                        assets.push(found.asset);
                        ttl
                    }
                    Ok(None) => { warnings.push("No se encontró el álbum y artista en Apple Music".into()); 30 * 60 }
                    Err(error) => { warnings.push(error); 60 }
                };
                let _lock = s.library_lock();
                let mut doc = load(s)?;
                doc.apple_checked.insert(track_id.clone(), (fingerprint, now() + ttl));
                doc.apple_warnings.insert(track_id.clone(), warnings.clone());
                s.save_document(DOCUMENT, &doc).map_err(|e| e.to_string())?;
            }
        }
        Action::Import { path } => assets.push(store(
            s,
            &bounded_file(Path::new(path))?,
            local_manifest(path.clone(), &track),
            &track,
        )?),
        Action::ImportManifest { path } => {
            let m: Manifest = serde_json::from_str(&resources::read_text(Path::new(path), 32768)?)
                .map_err(|_| "Manifiesto de portada inválido")?;
            let relative = Path::new(&m.path);
            let media = if relative.is_absolute() {
                relative.to_path_buf()
            } else {
                Path::new(path)
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(relative)
            };
            assets.push(store(s, &bounded_file(&media)?, m, &track)?);
        }
        Action::Search { query } => {
            if !load(s)?.preferences.external {
                return Err("Activa las consultas externas de portadas primero".into());
            }
            match archive_search(&track, query.clone()) {
                Ok(found) => search = Some(found),
                Err(e) => warnings.push(e),
            }
            let existing = load(s)?.tracks.get(&track_id).and_then(|assets| assets.iter()
                .filter(|asset| asset.provider == "apple-music" && image_path(s, &asset.id, "preview").is_ok_and(|path| path.is_file()))
                .max_by_key(|asset| (asset.animated, asset.saved_at))).cloned();
            match apple::search(s, &track, query.as_deref(), existing.as_ref(), true) {
                Ok(Some(found)) => {
                    if let Some(warning) = found.warning { warnings.push(warning); }
                    let asset = found.asset;
                    if let Some(candidates) = &mut search { candidates.push(asset.clone()); }
                    assets.push(asset);
                }
                Ok(None) => {},
                Err(error) => warnings.push(error),
            }
        }
        Action::Archive { identifier } => {
            if !load(s)?.preferences.external {
                return Err("Las consultas externas están desactivadas".into());
            }
            assets.push(archive(s, &track, identifier)?);
        }
        _ => {}
    }
    let _lock = s.library_lock();
    let mut doc = load(s)?;
    let mut changed = !assets.is_empty();
    if matches!(&action, Action::Search { .. }) {
        doc.apple_warnings.insert(track_id.clone(), warnings.clone());
        changed = true;
    }
    for a in assets {
        let entries = doc.tracks.entry(track_id.clone()).or_default();
        if entries.len() >= 100 && !entries.iter().any(|old| old.id == a.id) {
            return Err("Límite de 100 portadas por canción".into());
        }
        entries.retain(|old| old.id != a.id || old.provider != a.provider);
        entries.push(a);
    }
    match action {
        Action::Select { id } => {
            if !doc
                .tracks
                .get(&track_id)
                .is_some_and(|v| v.iter().any(|a| a.id == id))
            {
                return Err("Portada no guardada".into());
            }
            doc.selected.insert(track_id.clone(), id);
            changed = true;
        }
        Action::Remove => {
            doc.tracks.remove(&track_id);
            doc.selected.remove(&track_id);
            changed = true;
        }
        Action::Preferences { preferences } => {
            if !["thumbnail", "preview"].contains(&preferences.quality.as_str()) {
                return Err("Calidad inválida".into());
            }
            doc.preferences = preferences;
            changed = true;
        }
        Action::Export { path, id } => {
            if !doc
                .tracks
                .get(&track_id)
                .is_some_and(|v| v.iter().any(|a| a.id == id))
            {
                return Err("Portada no guardada".into());
            }
            if Path::new(&path).extension().and_then(|x| x.to_str()) != id.rsplit('.').next() {
                return Err("La extensión debe coincidir con el original".into());
            }
            let bytes = bounded_file(&image_path(s, &id, "original")?)?;
            crate::storage::write_atomic(Path::new(&path), &bytes).map_err(|e| e.to_string())?;
        }
        _ => {}
    }
    if changed {
        s.save_document(DOCUMENT, &doc).map_err(|e| e.to_string())?;
    }
    let resolved = resolve(s, &doc, &track);
    if let Some(saved) = doc.apple_warnings.get(&track_id) {
        for warning in saved {
            if !warnings.contains(warning) { warnings.push(warning.clone()); }
        }
    }
    let mut candidates = search.unwrap_or_else(|| doc.tracks.remove(&track_id).unwrap_or_default());
    let providers = crate::settings::load(s)?.settings.artwork.provider_priority;
    candidates.sort_by_key(|a| configured_priority(a, &providers));
    Ok(Snapshot {
        identity: apple::identity(&track),
        resolved,
        candidates,
        preferences: doc.preferences,
        warnings,
    })
}

/// A prepared source already has permission for its catalog and cover request.
pub(crate) fn source_artwork(s: &StorageManager, track: &TrackEntry, thumbnail: Option<&str>) -> Result<(), String> {
    let discovered = execute(s, track.id.clone(), Action::Discover { force: false })?;
    let asset = match discovered.candidates.into_iter().find(|asset| asset.provider == "apple-music") {
        Some(asset) => asset,
        _ => {
            let Some(thumbnail) = thumbnail else { return Ok(()); };
            let url = reqwest::Url::parse(thumbnail).map_err(|e| e.to_string())?;
            if url.scheme() != "https" || url.host_str() != Some("i.ytimg.com") {
                return Ok(());
            }
            let source_url = format!("https://www.youtube.com/watch?v={}",
                url.path().split('/').nth(2).unwrap_or_default());
            let bytes = resources::get(url, MAX_IMAGE)?;
            store(s, &bytes, Manifest {
                version: 1, provider: "youtube".into(), path: String::new(),
                title: track.title.clone(), artist: track.artist.clone().unwrap_or_default(),
                album: track.album.clone().unwrap_or_default(), source_url: Some(source_url),
                license: "YouTube source thumbnail".into(), attribution: "YouTube".into(),
                permitted_storage: true,
            }, track)?
        }
    };
    let _lock = s.library_lock();
    let mut doc = load(s)?;
    if asset.provider == "apple-music" {
        let mut library = library::load(s)?;
        if let Some(saved) = library.tracks.iter_mut().find(|saved| saved.id == track.id) {
            // Use the catalog spelling after the title and artist have matched.
            saved.artist = Some(asset.artist.clone());
            if saved.album.as_ref().is_none_or(|album| album.trim().is_empty()) {
                saved.album = Some(asset.album.clone());
            }
            metadata::prepare(saved)?;
            library::save(s, &library)?;
        }
    }
    let entries = doc.tracks.entry(track.id.clone()).or_default();
    entries.retain(|old| old.id != asset.id || old.provider != asset.provider);
    if entries.len() < 100 { entries.push(asset); }
    s.save_document(DOCUMENT, &doc).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn artwork_action(
    app: tauri::AppHandle,
    track_id: String,
    action: Action,
) -> Result<Snapshot, String> {
    use tauri::{Emitter, Manager};
    tauri::async_runtime::spawn_blocking(move || {
        let changed = !matches!(
            action,
            Action::Snapshot | Action::Search { .. } | Action::Export { .. }
        );
        let snapshot = execute(&app.state::<StorageManager>(), track_id, action)?;
        if changed {
            let _ = app.emit("artwork:changed", ());
        }
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    fn png() -> Vec<u8> {
        encode_png(&image::DynamicImage::new_rgb8(40, 60)).unwrap()
    }
    #[test]
    fn invalid_images_and_permissions() {
        let root = std::env::temp_dir().join(format!("zenith-art-{}", rand::random::<u64>()));
        let s = StorageManager::open(root.clone(), true).unwrap();
        let t = TrackEntry::default();
        assert!(store(&s, b"not image", local_manifest(String::new(), &t), &t).is_err());
        let mut m = local_manifest(String::new(), &t);
        m.permitted_storage = false;
        assert!(store(&s, &png(), m, &t).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn asset_validation_and_restart() {
        let root = std::env::temp_dir().join(format!("zenith-art-{}", rand::random::<u64>()));
        let s = StorageManager::open(root.clone(), true).unwrap();
        let t = TrackEntry {
            id: "t".into(),
            title: "Song".into(),
            ..Default::default()
        };
        library::save(
            &s,
            &crate::storage::indexes::TracksDocument {
                tracks: vec![t.clone()],
                locations: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        let a = store(&s, &png(), local_manifest(String::new(), &t), &t).unwrap();
        let b = store(&s, &png(), local_manifest(String::new(), &t), &t).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!((a.width, a.height), (40, 60));
        let mut doc = Document::default();
        doc.tracks.insert("t".into(), vec![a.clone()]);
        s.save_document(DOCUMENT, &doc).unwrap();
        drop(s);
        let s = StorageManager::open(root.clone(), true).unwrap();
        let snap = execute(&s, "t".into(), Action::Snapshot).unwrap();
        assert_eq!(snap.resolved.unwrap().id, a.id);
        assert!(execute(&s, "t".into(), Action::Search { query: None }).is_err());
        fs::remove_file(image_path(&s, &a.id, "preview").unwrap()).unwrap();
        assert!(
            execute(&s, "t".into(), Action::Snapshot)
                .unwrap()
                .resolved
                .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn identifiers_do_not_escape_assets() {
        for id in [
            "../x.png",
            "file:///tmp/x",
            "archive:abc",
            &format!("{}.svg", "a".repeat(64)),
        ] {
            assert!(!valid_id(id));
        }
        assert!(!safe_identifier("../secret"));
    }
    #[test]
    fn licenses_require_the_real_domain() {
        assert!(permitted_license(
            "https://creativecommons.org/licenses/by/4.0/"
        ));
        assert!(!permitted_license(
            "https://creativecommons.org.evil/licenses/by/4.0/"
        ));
        assert!(!permitted_license("https://example.com/copyright"));
    }
    #[test]
    fn priorities_are_stable() {
        let a = Asset {
            id: String::new(),
            provider: "apple-music".into(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            source_url: None,
            license: String::new(),
            attribution: String::new(),
            saved_at: 0,
            width: 0,
            height: 0,
            animated: true,
            motion_url: None,
            score: 0,
        };
        assert_eq!(priority(&a), 0);
        let mut b = a.clone();
        b.animated = false;
        assert_eq!(priority(&b), 1);
        b.provider = "archive-org".into();
        assert_eq!(priority(&b), 2);
        b.provider = "youtube".into();
        assert_eq!(priority(&b), 3);
        b.provider = "local".into();
        assert_eq!(priority(&b), 4);
    }
}
