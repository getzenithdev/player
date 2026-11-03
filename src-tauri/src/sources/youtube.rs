//! Local yt-dlp adapter. No shell, user configuration, plugins or account cookies.
//! The packaged Node 22 runtime keeps the helper compatible with macOS 12.
use super::{Entry, MAX_AUDIO, MAX_JSON, ProviderId, StorageManager};
use std::{
    fs,
    io::{Read, Seek},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

static JOB: Mutex<()> = Mutex::new(());
static GENERATION: AtomicU64 = AtomicU64::new(0);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
const AUDIO_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) fn cancel() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

fn video_id(id: &str) -> Result<&str, String> {
    if id.len() == 11
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        Ok(id)
    } else {
        Err("Identificador de YouTube inválido".into())
    }
}

fn command(tools: &Path) -> Result<Command, String> {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let downloader = tools.join(format!("yt-dlp{suffix}"));
    let runtime = tools.join(format!("node{suffix}"));
    if !downloader.is_file() || !runtime.is_file() {
        return Err("Faltan los componentes de YouTube. En desarrollo ejecuta npm run youtube:prepare y reinicia la app; si está instalada, reinstala una versión completa. Puedes usar YouTube API para buscar metadatos.".into());
    }
    let mut command = Command::new(downloader);
    command.args([
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-cache-dir",
        "--no-playlist",
        "--no-progress",
        "--no-warnings",
        "--no-color",
        "--no-js-runtimes",
        "--js-runtimes",
    ]);
    command.arg(format!("node:{}", runtime.display()));
    command.args([
        "--socket-timeout",
        "15",
        "--retries",
        "1",
        "--fragment-retries",
        "1",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    Ok(command)
}

/// Drain both pipes concurrently; retain bounded output and terminate a noisy
/// helper instead of deadlocking on a full OS pipe. Never surface raw stderr:
/// an upstream diagnostic can contain a signed URL or other sensitive data.
fn capture(mut input: impl Read, max: usize, overflow: &AtomicBool) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match input.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                let keep = count.min(max.saturating_sub(output.len()));
                output.extend_from_slice(&buffer[..keep]);
                if keep < count {
                    overflow.store(true, Ordering::Relaxed);
                }
            }
        }
    }
    output
}

fn terminate_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    // Each helper starts in its own process group. Kill Node and the extractor
    // together so inherited pipes cannot outlive cancellation or a deadline.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

fn run(
    command: &mut Command,
    timeout: Duration,
    generation: u64,
    download: Option<&Path>,
) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|_| "No se pudo iniciar YouTube. Comprueba los componentes, permisos y compatibilidad del sistema.".to_string())?;
    let stdout = child.stdout.take().ok_or("No se pudo leer YouTube")?;
    let stderr = child.stderr.take().ok_or("No se pudo leer YouTube")?;
    let overflow = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let out = scope.spawn(|| capture(stdout, MAX_JSON, &overflow));
        let err = scope.spawn(|| capture(stderr, 64 * 1024, &overflow));
        let start = Instant::now();
        let result = loop {
            let failure = if GENERATION.load(Ordering::Relaxed) != generation {
                Some("Operación de YouTube cancelada")
            } else if start.elapsed() >= timeout {
                Some("YouTube tardó demasiado; inténtalo de nuevo")
            } else if overflow.load(Ordering::Relaxed) {
                Some("Respuesta de YouTube demasiado grande")
            } else if download
                .and_then(|path| fs::metadata(path).ok())
                .is_some_and(|meta| meta.len() > MAX_AUDIO)
            {
                Some("El audio supera el límite de 128 MiB")
            } else {
                None
            };
            if let Some(message) = failure {
                terminate_tree(&mut child);
                let _ = child.wait();
                break Err(message.to_string());
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    break if status.success() {
                        Ok(())
                    } else {
                        Err("YouTube no pudo completar la operación. El video puede estar restringido, no ofrecer audio compatible o requerir una actualización del extractor. Prueba otro resultado; la API alternativa solo permite búsquedas.".into())
                    };
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => {
                    terminate_tree(&mut child);
                    let _ = child.wait();
                    break Err("No se pudo consultar el proceso de YouTube".into());
                }
            }
        };
        let output = out
            .join()
            .map_err(|_| "No se pudo leer la respuesta de YouTube")?;
        let _ = err.join();
        result?;
        if overflow.load(Ordering::Relaxed) {
            return Err("Respuesta de YouTube demasiado grande".into());
        }
        Ok(output)
    })
}

fn parse_results(raw: &[u8]) -> Result<Vec<Entry>, String> {
    let json: serde_json::Value =
        serde_json::from_slice(raw).map_err(|_| "Respuesta de búsqueda inválida")?;
    let items = json["entries"]
        .as_array()
        .ok_or("Respuesta de búsqueda inválida")?;
    let mut entries = Vec::new();
    for item in items.iter().take(25) {
        let Some(id) = item["id"].as_str().filter(|id| video_id(id).is_ok()) else {
            continue;
        };
        if item["is_live"].as_bool() == Some(true)
            || item["live_status"].as_str() == Some("is_live")
        {
            continue;
        }
        let title = item["title"].as_str().unwrap_or("Video");
        let channel = item["channel"]
            .as_str()
            .or_else(|| item["uploader"].as_str())
            .unwrap_or_default();
        entries.push(Entry {
            provider: ProviderId::YoutubeAuto,
            id: id.into(),
            name: title.chars().take(512).collect(),
            kind: "audio".into(),
            detail: channel.chars().take(256).collect(),
            size: None,
            source_url: Some(format!("https://www.youtube.com/watch?v={id}")),
        });
    }
    Ok(entries)
}

pub(super) fn search(tools: &Path, query: &str) -> Result<Vec<Entry>, String> {
    let _job = JOB
        .try_lock()
        .map_err(|_| "Ya hay una operación de YouTube; espera o cancélala")?;
    let generation = GENERATION.load(Ordering::Relaxed);
    let mut command = command(tools)?;
    // Flat results avoid extracting formats for every search hit.
    command.args([
        "--flat-playlist",
        "--dump-single-json",
        "--skip-download",
        "--",
    ]);
    command.arg(format!("ytsearch25:{query}"));
    parse_results(&run(&mut command, SEARCH_TIMEOUT, generation, None)?)
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn download(storage: &StorageManager, tools: &Path, id: &str) -> Result<String, String> {
    let id = video_id(id)?;
    let _job = JOB
        .try_lock()
        .map_err(|_| "Ya hay una operación de YouTube; espera o cancélala")?;
    let generation = GENERATION.load(Ordering::Relaxed);
    let directory = storage.document_path("cache/audio");
    fs::create_dir_all(&directory).map_err(|_| "No se pudo preparar la caché")?;
    let target = directory.join(format!("youtube-{id}.m4a"));
    if valid_audio(&target) {
        return Ok(target.to_string_lossy().into_owned());
    }
    let temp = Temporary(directory.join(format!(".youtube-{:032x}", rand::random::<u128>())));
    fs::create_dir(&temp.0).map_err(|_| "No se pudo preparar la descarga")?;
    let audio = temp.0.join("audio.m4a");
    let mut command = command(tools)?;
    // Select native AAC, which the existing Rodio/Symphonia MP4 decoder handles.
    // No FFmpeg/transcoding or Opus dependency is necessary. Fail explicitly if
    // this format is unavailable rather than mislabelling an unsupported file.
    command.args([
        "--format",
        "bestaudio[ext=m4a][acodec^=mp4a]",
        "--fixup",
        "never",
        "--max-filesize",
        "128M",
        "--no-continue",
        "--no-part",
        "--no-mtime",
        "--match-filters",
        "!is_live & !is_upcoming",
        "--output",
    ]);
    command
        .arg(&audio)
        .arg("--")
        .arg(format!("https://www.youtube.com/watch?v={id}"));
    run(&mut command, AUDIO_TIMEOUT, generation, Some(&audio))?;
    if !valid_audio(&audio) {
        return Err("YouTube no entregó un archivo M4A válido de hasta 128 MiB".into());
    }
    if target.exists() {
        fs::remove_file(&target).map_err(|_| "No se pudo renovar la caché")?;
    }
    fs::rename(&audio, &target).map_err(|_| "No se pudo guardar el audio en caché")?;
    Ok(target.to_string_lossy().into_owned())
}

fn valid_audio(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() < 12 || meta.len() > MAX_AUDIO {
        return false;
    }
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 12];
    if file.read_exact(&mut head).is_err() || &head[4..8] != b"ftyp" || file.rewind().is_err() {
        return false;
    }
    // A successful helper exit or a container header alone does not prove the
    // download is playable (proxies can return truncated or substituted data).
    rodio::Decoder::try_from(file)
        .ok()
        .and_then(|mut decoder| decoder.next())
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_urls_and_arguments_as_video_ids() {
        for bad in [
            "../anything",
            "https://youtube.com/watch?v=BaW_jenozKc",
            "--version",
            "abcdefghij/",
            "",
        ] {
            assert!(video_id(bad).is_err());
        }
        assert!(video_id("BaW_jenozKc").is_ok());
    }

    #[test]
    fn filters_malformed_and_live_search_hits() {
        let raw = br#"{"entries":[{"id":"BaW_jenozKc","title":"Test","channel":"Channel"},{"id":"../evil"},{"id":"abcdefghijk","is_live":true},null]}"#;
        let entries = parse_results(raw).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].provider, ProviderId::YoutubeAuto);
        assert_eq!(entries[0].kind, "audio");
        assert!(parse_results(b"bad json").is_err());
        assert!(parse_results(br#"{"entries":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn rejects_missing_packaged_components() {
        assert!(command(Path::new("/missing-zenith-tools")).is_err());
    }

    #[test]
    fn validates_decodable_aac_and_reuses_cache_without_a_helper() {
        let _guard = super::super::tests::serial();
        use base64::Engine as _;
        let root = std::env::temp_dir().join(format!(
            "zenith-youtube-test-{:032x}",
            rand::random::<u128>()
        ));
        let cleanup = Temporary(root.clone());
        let storage = StorageManager::open(root, false).unwrap();
        let dir = storage.document_path("cache/audio");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("youtube-BaW_jenozKc.m4a");
        fs::write(&file, b"\0\0\0\x0cftypM4A ").unwrap();
        assert!(!valid_audio(&file));
        let audio = base64::engine::general_purpose::STANDARD
            .decode(include_str!("../../tests/fixtures/aac.m4a.base64").trim())
            .unwrap();
        fs::write(&file, audio).unwrap();
        assert!(valid_audio(&file));
        assert_eq!(
            download(&storage, Path::new("/no-tools"), "BaW_jenozKc").unwrap(),
            file.to_string_lossy()
        );
        assert!(download(&storage, Path::new("/no-tools"), "../bad-file").is_err());
        drop(cleanup);
    }

    // This subprocess fixture exercises pipe draining, deadlines and sanitized
    // errors on every OS without needing YouTube or a Python/Node install.
    #[test]
    fn helper() {
        match std::env::var("ZENITH_YOUTUBE_TEST").as_deref() {
            Ok("wait") => std::thread::sleep(Duration::from_secs(10)),
            Ok("fail") => {
                eprintln!("secret-signed-url");
                std::process::exit(1);
            }
            Ok("large") => {
                use std::io::Write;
                let _ = std::io::stdout().write_all(&vec![b'x'; MAX_JSON + 1]);
            }
            _ => {}
        }
    }

    fn helper_command(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "sources::youtube::tests::helper", "--nocapture"]);
        command.env("ZENITH_YOUTUBE_TEST", mode);
        command
    }

    #[test]
    fn bounds_output_timeout_and_sanitizes_helper_errors() {
        let _guard = super::super::tests::serial();
        let generation = GENERATION.load(Ordering::Relaxed);
        let error = run(
            &mut helper_command("fail"),
            Duration::from_secs(5),
            generation,
            None,
        )
        .unwrap_err();
        assert!(!error.contains("secret-signed-url"));
        assert!(
            run(
                &mut helper_command("wait"),
                Duration::from_millis(100),
                generation,
                None
            )
            .unwrap_err()
            .contains("tardó")
        );
        assert!(
            run(
                &mut helper_command("large"),
                Duration::from_secs(5),
                generation,
                None
            )
            .unwrap_err()
            .contains("grande")
        );
        assert!(
            run(
                &mut helper_command("wait"),
                Duration::from_secs(5),
                generation.wrapping_sub(1),
                None
            )
            .unwrap_err()
            .contains("cancelada")
        );
    }
}
