//! Tests for external music sources (roadmap phase 9).
//!
//! Every test runs against a temporary storage root, an in-memory credential
//! store and a loopback HTTP server, so no platform keyring, browser or real
//! provider is ever touched.

use super::*;
use std::sync::atomic::AtomicUsize;

/// The pending OAuth connection lives in one global slot, so the tests that
/// create, finish or cancel a connection run one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

pub(super) fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|error| error.into_inner())
}

// --- fixtures ----------------------------------------------------------------

#[derive(Default)]
struct MemoryVault {
    entries: Mutex<BTreeMap<ProviderId, Secrets>>,
}

impl SecretStore for MemoryVault {
    fn write(&self, provider: ProviderId, secrets: &Secrets) -> Result<(), String> {
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(provider, secrets.clone());
        Ok(())
    }

    fn read(&self, provider: ProviderId) -> Result<Secrets, String> {
        Ok(self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&provider)
            .cloned()
            .unwrap_or_default())
    }

    fn remove(&self, provider: ProviderId) -> Result<(), String> {
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&provider);
        Ok(())
    }
}

struct Fixture {
    root: PathBuf,
    storage: StorageManager,
    vault: MemoryVault,
}

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("zenith-sources-{}", crate::playlists::new_id()));
        let storage = StorageManager::open(root.clone(), false).expect("temporary storage");
        Self {
            root,
            storage,
            vault: MemoryVault::default(),
        }
    }

    fn run(&self, env: &Env, action: Action) -> Result<Snapshot, String> {
        execute_with(&self.storage, &self.vault, env, action)
    }

    /// Raw bytes of the document the interface persists, to prove that no
    /// credential is ever written next to the connections.
    fn document(&self) -> String {
        std::fs::read_to_string(self.storage.document_path(DOCUMENT)).unwrap_or_default()
    }

    fn enable_external(&self, env: &Env) {
        self.run(env, Action::External { enabled: true })
            .expect("external sources enabled");
    }

    fn seed_library(&self) {
        self.storage
            .save_document(
                "library/tracks.json",
                &serde_json::json!({
                    "tracks": [{
                        "id": "track-1",
                        "title": "Nocturne Drive",
                        "artist": "Aurora",
                        "album": "Neon",
                        "local_path": "/music/nocturne-drive.mp3"
                    }]
                }),
            )
            .expect("library document");
        self.storage.rebuild_indexes();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

fn provider(snapshot: &Snapshot, id: ProviderId) -> &ProviderStatus {
    snapshot
        .providers
        .iter()
        .find(|item| item.info.id == id)
        .expect("every provider is part of the snapshot")
}

/// WebDAV connection against `http://127.0.0.1:<port>/dav/`.
fn dav_fields(endpoint: &str) -> BTreeMap<String, String> {
    fields(&[
        ("endpoint", endpoint),
        ("username", "ana"),
        ("password", "secreto-del-servidor"),
    ])
}

// --- loopback HTTP server ----------------------------------------------------

type MockReply = (u16, &'static str, Vec<u8>);
type MockHandler = Arc<dyn Fn(&MockRequest) -> MockReply + Send + Sync>;

struct MockRequest {
    method: String,
    path: String,
    headers: String,
    body: Vec<u8>,
}

struct MockServer {
    base: String,
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        207 => "Multi-Status",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Status",
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> Option<MockRequest> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(position) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(read) => data.extend_from_slice(&chunk[..read]),
        }
        if data.len() > 64 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&data[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut parts = lines.next()?.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = data[header_end + 4..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => body.extend_from_slice(&chunk[..read]),
        }
    }
    let path = target.split('?').next().unwrap_or(&target).to_string();
    Some(MockRequest {
        method,
        path,
        headers: head,
        body,
    })
}

fn mock_server<F>(handler: F) -> MockServer
where
    F: Fn(&MockRequest) -> MockReply + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback port for the mock server");
    let port = listener.local_addr().expect("mock server address").port();
    listener
        .set_nonblocking(true)
        .expect("non blocking mock server");
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let hits = Arc::new(AtomicUsize::new(0));
    let thread_hits = hits.clone();
    let handler: MockHandler = Arc::new(handler);
    std::thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // On Windows the accepted socket inherits the non blocking
                    // mode of the listener, which would drop the first read.
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                    if let Some(request) = read_request(&mut stream) {
                        thread_hits.fetch_add(1, Ordering::Relaxed);
                        let (status, content_type, body) = handler(&request);
                        let head = format!(
                            "HTTP/1.1 {} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            status,
                            status_text(status),
                            body.len(),
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                }
                Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });
    MockServer {
        base: format!("http://127.0.0.1:{port}"),
        hits,
        stop,
    }
}

/// Endpoint whose port nobody listens on anymore: the connection attempt fails
/// exactly like a provider that went offline.
fn unreachable_endpoint() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback port");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}/dav/")
}

// --- WebDAV answers ----------------------------------------------------------

const AUDIO_BYTES: &[u8] = b"ID3\x03\x00\x00\x00\x00\x00\x00zenith-mock-audio";

const PROPFIND_ROOT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:">
<d:response>
<d:href>/dav/</d:href>
<d:propstat><d:prop>
<d:displayname>raíz</d:displayname>
<d:resourcetype><d:collection/></d:resourcetype>
</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
</d:response>
<d:response>
<d:href>/dav/canciones/</d:href>
<d:propstat><d:prop>
<d:displayname>canciones</d:displayname>
<d:resourcetype><d:collection/></d:resourcetype>
</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
</d:response>
<d:response>
<d:href>/dav/canciones/Nocturne.mp3</d:href>
<d:propstat><d:prop>
<d:displayname>Nocturne.mp3</d:displayname>
<d:getcontentlength>2048</d:getcontentlength>
<d:getcontenttype>audio/mpeg</d:getcontenttype>
<d:resourcetype/>
</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
</d:response>
</d:multistatus>"#;

const PROPFIND_FOLDER: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:">
<d:response>
<d:href>/dav/canciones/Nocturne.mp3</d:href>
<d:propstat><d:prop>
<d:displayname>Nocturne.mp3</d:displayname>
<d:getcontentlength>2048</d:getcontentlength>
<d:getcontenttype>audio/mpeg</d:getcontenttype>
<d:resourcetype/>
</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
</d:response>
</d:multistatus>"#;

fn webdav_server() -> MockServer {
    mock_server(
        |request| match (request.method.as_str(), request.path.as_str()) {
            ("PROPFIND", "/dav") | ("PROPFIND", "/dav/") => (
                207,
                "application/xml; charset=utf-8",
                PROPFIND_ROOT.as_bytes().to_vec(),
            ),
            ("PROPFIND", path) if path.starts_with("/dav/") => (
                207,
                "application/xml; charset=utf-8",
                PROPFIND_FOLDER.as_bytes().to_vec(),
            ),
            ("GET", path) if path.ends_with(".mp3") => (200, "audio/mpeg", AUDIO_BYTES.to_vec()),
            _ => (404, "text/plain", b"no encontrado".to_vec()),
        },
    )
}

/// Environment without a browser and without overridden endpoints: WebDAV and
/// the local library take their address from the stored settings.
fn plain_env() -> Env {
    Env {
        open_browser: false,
        endpoints: BTreeMap::new(),
        youtube_tools: PathBuf::from("missing-test-tools"),
    }
}

// --- contract ----------------------------------------------------------------

#[test]
fn each_provider_declares_only_the_capabilities_it_implements() {
    let _guard = serial();
    let local = spec(ProviderId::Local);
    assert!(!local.capabilities.contains(&Capability::Auth));
    assert!(!local.capabilities.contains(&Capability::Download));
    assert_eq!(local.auth, AuthKind::None);

    let youtube = spec(ProviderId::Youtube);
    assert!(!youtube.capabilities.contains(&Capability::Playback));
    assert!(!youtube.capabilities.contains(&Capability::Download));
    assert!(!youtube.capabilities.contains(&Capability::Revoke));
    assert!(youtube.capabilities.contains(&Capability::Search));

    let webdav = spec(ProviderId::Webdav);
    assert!(!webdav.capabilities.contains(&Capability::Revoke));
    assert!(webdav.capabilities.contains(&Capability::Download));

    for provider in [
        ProviderId::GoogleDrive,
        ProviderId::Dropbox,
        ProviderId::OneDrive,
    ] {
        let info = spec(provider);
        assert_eq!(info.auth, AuthKind::OAuth2);
        for capability in [
            Capability::Auth,
            Capability::Search,
            Capability::Read,
            Capability::Playback,
            Capability::Download,
        ] {
            assert!(
                info.capabilities.contains(&capability),
                "{provider:?} must declare {capability:?}"
            );
        }
        assert_eq!(
            info.capabilities.contains(&Capability::Revoke),
            provider != ProviderId::OneDrive
        );
        assert_eq!(info.required, vec!["client_id".to_string()]);
    }

    assert_eq!(registry().len(), 7);
    assert!(registry().iter().all(|info| !info.name.is_empty()));
}

#[test]
fn pending_authorization_survives_refresh_and_unrelated_disconnect() {
    let _guard = serial();
    cancel_pending(None);
    let f = Fixture::new();
    let env = plain_env();
    f.enable_external(&env);
    let connected = f
        .run(
            &env,
            Action::Connect {
                provider: ProviderId::GoogleDrive,
                fields: fields(&[("client_id", "client-123")]),
            },
        )
        .expect("start authorization");
    let url = connected.pending_url.expect("authorization URL");
    let refreshed = f.run(&env, Action::Snapshot).expect("refresh");
    assert_eq!(refreshed.pending_url.as_deref(), Some(url.as_str()));
    let disconnected = f
        .run(
            &env,
            Action::Disconnect {
                provider: ProviderId::Webdav,
            },
        )
        .expect("disconnect another provider");
    assert_eq!(disconnected.pending_url.as_deref(), Some(url.as_str()));
    assert_eq!(
        provider(&disconnected, ProviderId::GoogleDrive).state,
        ConnectionState::Pending
    );
    let disabled = f
        .run(&env, Action::External { enabled: false })
        .expect("disable network");
    assert!(disabled.pending_url.is_none());
    assert_eq!(
        provider(&disabled, ProviderId::GoogleDrive).state,
        ConnectionState::Error
    );
    assert_eq!(
        provider(&disabled, ProviderId::Local).state,
        ConnectionState::Connected
    );
}

#[test]
fn an_abandoned_authorization_can_be_reconnected_after_restart_or_timeout() {
    let _guard = serial();
    cancel_pending(None);
    let f = Fixture::new();
    let env = plain_env();
    f.enable_external(&env);
    edit(&f.storage, ProviderId::GoogleDrive, |connection| {
        connection.state = ConnectionState::Pending;
    })
    .expect("state left by a previous process");
    let snapshot = f.run(&env, Action::Snapshot).expect("restarted snapshot");
    assert_eq!(
        provider(&snapshot, ProviderId::GoogleDrive).state,
        ConnectionState::Error
    );
    f.run(
        &env,
        Action::Connect {
            provider: ProviderId::GoogleDrive,
            fields: fields(&[("client_id", "client-123")]),
        },
    )
    .expect("reconnect");
    {
        let mut pending = PENDING.lock().unwrap();
        pending.as_mut().unwrap().deadline = Instant::now() - Duration::from_secs(1);
    }
    let snapshot = f
        .run(&env, Action::Snapshot)
        .expect("expired authorization");
    assert!(snapshot.pending_url.is_none());
    assert_eq!(
        provider(&snapshot, ProviderId::GoogleDrive).state,
        ConnectionState::Error
    );
    cancel_pending(None);
}

#[test]
fn the_document_persists_connections_and_settings_but_never_credentials() {
    let _guard = serial();
    let f = Fixture::new();
    let env = plain_env();
    f.run(&env, Action::Snapshot).expect("snapshot");
    assert!(
        !f.document().contains("secreto"),
        "an empty document must not exist yet"
    );

    f.enable_external(&env);
    f.run(
        &env,
        Action::Settings {
            provider: ProviderId::Dropbox,
            settings: ProviderSettings {
                client_id: "client-abc".into(),
                ..ProviderSettings::default()
            },
        },
    )
    .expect("settings saved");
    f.run(
        &env,
        Action::Connect {
            provider: ProviderId::Webdav,
            fields: dav_fields("https://dav.example.com/music/"),
        },
    )
    .expect("webdav connection");

    let snapshot = f.run(&env, Action::Snapshot).expect("snapshot");
    assert!(snapshot.external);
    let dav = provider(&snapshot, ProviderId::Webdav);
    assert_eq!(dav.state, ConnectionState::Connected);
    assert!(dav.has_secret);
    assert_eq!(dav.label, "https://dav.example.com/music/");
    assert_eq!(
        provider(&snapshot, ProviderId::Dropbox).settings.client_id,
        "client-abc"
    );

    let document = f.document();
    assert!(
        !document.contains("secreto-del-servidor"),
        "credentials leaked into {DOCUMENT}: {document}"
    );
    assert!(document.contains("dav.example.com"));
    assert!(document.contains("client-abc"));

    // A restart reads the same non-secret state from disk.
    let reopened = StorageManager::open(f.root.clone(), false).expect("reopened storage");
    let snapshot = execute_with(&reopened, &f.vault, &env, Action::Snapshot).expect("snapshot");
    assert_eq!(
        provider(&snapshot, ProviderId::Webdav).state,
        ConnectionState::Connected
    );
    assert!(provider(&snapshot, ProviderId::Webdav).has_secret);

    // The credential itself lives only in the store, and disconnecting forgets it.
    assert!(
        f.vault
            .read(ProviderId::Webdav)
            .expect("vault")
            .password
            .is_some()
    );
    f.run(
        &env,
        Action::Disconnect {
            provider: ProviderId::Webdav,
        },
    )
    .expect("disconnect");
    assert!(
        f.vault
            .read(ProviderId::Webdav)
            .expect("vault")
            .password
            .is_none()
    );
    let snapshot = f.run(&env, Action::Snapshot).expect("snapshot");
    assert_eq!(
        provider(&snapshot, ProviderId::Webdav).state,
        ConnectionState::Disconnected
    );
}

#[test]
fn external_sources_stay_off_until_the_user_opts_in() {
    let _guard = serial();
    let f = Fixture::new();
    let env = plain_env();
    f.seed_library();

    let snapshot = f.run(&env, Action::Snapshot).expect("snapshot");
    assert!(!snapshot.external);

    let error = f
        .run(
            &env,
            Action::Search {
                provider: ProviderId::Webdav,
                query: "nocturne".into(),
            },
        )
        .expect_err("network actions need the opt-in");
    assert!(error.contains("fuentes externas"), "{error}");

    let error = f
        .run(
            &env,
            Action::Connect {
                provider: ProviderId::Youtube,
                fields: fields(&[("api_key", "AIza-demo")]),
            },
        )
        .expect_err("connecting needs the opt-in");
    assert!(error.contains("fuentes externas"), "{error}");

    // The local library keeps answering with the opt-in off.
    let snapshot = f
        .run(
            &env,
            Action::Search {
                provider: ProviderId::Local,
                query: "nocturne".into(),
            },
        )
        .expect("local search");
    assert_eq!(snapshot.results.len(), 1);
    assert_eq!(snapshot.results[0].provider, ProviderId::Local);
    assert_eq!(snapshot.results[0].id, "track-1");

    f.enable_external(&env);
    f.run(&env, Action::External { enabled: false })
        .expect("opt-in removed");
    let error = f
        .run(
            &env,
            Action::Browse {
                provider: ProviderId::Webdav,
                path: None,
            },
        )
        .expect_err("browsing needs the opt-in");
    assert!(error.contains("fuentes externas"), "{error}");
}

#[test]
fn webdav_browse_and_search_read_propfind_answers() {
    let _guard = serial();
    let f = Fixture::new();
    let server = webdav_server();
    let env = plain_env();
    f.enable_external(&env);
    f.run(
        &env,
        Action::Connect {
            provider: ProviderId::Webdav,
            fields: dav_fields(&format!("{}/dav/", server.base)),
        },
    )
    .expect("webdav connection");

    let snapshot = f
        .run(
            &env,
            Action::Browse {
                provider: ProviderId::Webdav,
                path: None,
            },
        )
        .expect("browse root");
    assert_eq!(snapshot.results.len(), 2);
    let folder = snapshot
        .results
        .iter()
        .find(|entry| entry.kind == "folder")
        .expect("folder entry");
    assert_eq!(folder.id, "canciones");
    let audio = snapshot
        .results
        .iter()
        .find(|entry| entry.kind == "audio")
        .expect("audio entry");
    assert_eq!(audio.name, "Nocturne.mp3");
    assert_eq!(audio.size, Some(2048));
    assert_eq!(audio.detail, "audio/mpeg");

    let snapshot = f
        .run(
            &env,
            Action::Browse {
                provider: ProviderId::Webdav,
                path: Some("canciones".into()),
            },
        )
        .expect("browse folder");
    assert_eq!(snapshot.results.len(), 1);
    assert_eq!(snapshot.results[0].id, "Nocturne.mp3");

    let snapshot = f
        .run(
            &env,
            Action::Search {
                provider: ProviderId::Webdav,
                query: "nocturne".into(),
            },
        )
        .expect("search");
    assert_eq!(snapshot.results.len(), 1);
    assert_eq!(snapshot.results[0].provider, ProviderId::Webdav);

    let error = f
        .run(
            &env,
            Action::Search {
                provider: ProviderId::Webdav,
                query: "   ".into(),
            },
        )
        .expect_err("empty query");
    assert!(error.contains("búsqueda"), "{error}");
}

#[test]
fn webdav_download_is_stored_once_for_the_audio_engine() {
    let _guard = serial();
    let f = Fixture::new();
    let server = webdav_server();
    let env = plain_env();
    f.enable_external(&env);
    f.run(
        &env,
        Action::Connect {
            provider: ProviderId::Webdav,
            fields: dav_fields(&format!("{}/dav/", server.base)),
        },
    )
    .expect("webdav connection");

    let snapshot = f
        .run(
            &env,
            Action::Download {
                provider: ProviderId::Webdav,
                id: "canciones/Nocturne.mp3".into(),
            },
        )
        .expect("download");
    let path = snapshot.downloaded.expect("downloaded path");
    assert!(path.ends_with(".mp3"), "{path}");
    assert_eq!(
        std::fs::read(&path).expect("cached audio"),
        AUDIO_BYTES.to_vec(),
        "the cached file keeps the bytes the provider sent"
    );

    let before = server.hits.load(Ordering::Relaxed);
    let snapshot = f
        .run(
            &env,
            Action::Download {
                provider: ProviderId::Webdav,
                id: "canciones/Nocturne.mp3".into(),
            },
        )
        .expect("cached download");
    assert_eq!(snapshot.downloaded.as_deref(), Some(path.as_str()));
    assert_eq!(
        server.hits.load(Ordering::Relaxed),
        before,
        "a cached file is never downloaded twice"
    );

    let error = f
        .run(
            &env,
            Action::Download {
                provider: ProviderId::Youtube,
                id: "dQw4w9WgXcQ".into(),
            },
        )
        .expect_err("youtube downloads are not offered");
    assert!(error.contains("descargas"), "{error}");
}

#[test]
fn insecure_endpoints_and_traversing_identifiers_are_rejected() {
    let _guard = serial();
    let f = Fixture::new();
    let env = plain_env();
    f.enable_external(&env);

    let error = f
        .run(
            &env,
            Action::Connect {
                provider: ProviderId::Webdav,
                fields: dav_fields("http://dav.example.com/music/"),
            },
        )
        .expect_err("plain http away from loopback is refused");
    assert!(error.contains("HTTPS"), "{error}");

    let error = f
        .run(
            &env,
            Action::Connect {
                provider: ProviderId::Webdav,
                fields: dav_fields("https://ana:secreto@dav.example.com/music/"),
            },
        )
        .expect_err("credentials inside the URL are refused");
    assert!(error.contains("credenciales"), "{error}");

    assert!(secure_url("https://dav.example.com/music/").is_ok());
    assert!(secure_url("http://127.0.0.1:8080/dav/").is_ok());
    assert!(secure_url("file:///etc/passwd").is_err());

    assert_eq!(
        relative_id("/canciones/tema.mp3").expect("id"),
        "canciones/tema.mp3"
    );
    assert!(relative_id("/canciones/../../etc/passwd").is_err());
    assert!(relative_id("/canciones/%2e%2e/%2e%2e/etc").is_err());
    assert!(safe_id("").is_err());
    assert!(safe_id("tema\ninjection").is_err());

    let error = f
        .run(
            &env,
            Action::Browse {
                provider: ProviderId::Webdav,
                path: Some("../etc/passwd".into()),
            },
        )
        .expect_err("traversal from the interface is refused");
    assert!(error.contains("Ruta no admitida"), "{error}");
}

// --- OAuth -------------------------------------------------------------------

/// Runs the two-step connection: the loopback listener is already listening
/// when the authorization URL comes back, so the test only has to answer it.
fn authorize(f: &Fixture, env: &Env, answer: impl FnOnce(&str, &str)) -> Result<Snapshot, String> {
    let snapshot = f.run(
        env,
        Action::Connect {
            provider: ProviderId::GoogleDrive,
            fields: fields(&[("client_id", "client-123")]),
        },
    )?;
    let url = snapshot
        .pending_url
        .clone()
        .expect("the authorization URL is shown in the interface");
    let parsed = reqwest::Url::parse(&url).expect("authorization url");
    let mut state = None;
    let mut redirect = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "state" => state = Some(value.to_string()),
            "redirect_uri" => redirect = Some(value.to_string()),
            _ => {}
        }
    }
    answer(
        &redirect.expect("loopback redirect"),
        &state.expect("state protects the exchange"),
    );
    f.run(
        env,
        Action::Finish {
            provider: ProviderId::GoogleDrive,
        },
    )
}

fn oauth_env(server: &MockServer) -> Env {
    Env {
        open_browser: false,
        youtube_tools: PathBuf::from("missing-test-tools"),
        endpoints: BTreeMap::from([(
            ProviderId::GoogleDrive,
            Endpoints {
                authorize: format!("{}/authorize", server.base),
                token: format!("{}/token", server.base),
                revoke: Some(format!("{}/revoke", server.base)),
                api: format!("{}/api/", server.base),
                content: String::new(),
            },
        )]),
    }
}

fn oauth_server(refused: Arc<AtomicBool>) -> MockServer {
    mock_server(
        move |request| match (request.method.as_str(), request.path.as_str()) {
            ("POST", "/token") => {
                let body = String::from_utf8_lossy(&request.body).to_string();
                if refused.load(Ordering::Relaxed) {
                    (
                        200,
                        "application/json",
                        br#"{"error":"invalid_grant","error_description":"token revocado"}"#
                            .to_vec(),
                    )
                } else if body.contains("grant_type=authorization_code") {
                    (
                    200,
                    "application/json",
                    br#"{"access_token":"access-1","refresh_token":"refresh-1","expires_in":3600}"#
                        .to_vec(),
                )
                } else {
                    (
                    200,
                    "application/json",
                    br#"{"access_token":"access-2","refresh_token":"refresh-2","expires_in":3600}"#
                        .to_vec(),
                )
                }
            }
            ("GET", "/api/about") => (
                200,
                "application/json",
                br#"{"user":{"emailAddress":"ana@example.com"}}"#.to_vec(),
            ),
            ("GET", "/api/files") => (200, "application/json", br#"{"files":[]}"#.to_vec()),
            ("POST", "/revoke") => (200, "application/json", b"{}".to_vec()),
            _ => (404, "text/plain", b"no encontrado".to_vec()),
        },
    )
}

fn answer_redirect(redirect: &str, callback_state: &str, code: &str) -> u16 {
    let url = format!("{redirect}?code={code}&state={callback_state}");
    let response = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("test client")
        .get(url)
        .send()
        .expect("the loopback listener answers");
    response.status().as_u16()
}

#[test]
fn oauth_connection_exchanges_the_code_and_keeps_tokens_in_the_vault() {
    let _guard = serial();
    let f = Fixture::new();
    let server = oauth_server(Arc::new(AtomicBool::new(false)));
    let env = oauth_env(&server);
    f.enable_external(&env);

    let snapshot = authorize(&f, &env, |redirect, state| {
        assert_eq!(answer_redirect(redirect, state, "code-123"), 200);
    })
    .expect("finish the authorization");
    assert!(snapshot.pending_url.is_none(), "the pending step is over");

    let drive = provider(&snapshot, ProviderId::GoogleDrive);
    assert_eq!(drive.state, ConnectionState::Connected);
    assert_eq!(drive.label, "ana@example.com");
    assert!(drive.has_secret);
    assert!(drive.expires_at.is_some());

    let secrets = f.vault.read(ProviderId::GoogleDrive).expect("vault");
    assert_eq!(secrets.access_token.as_deref(), Some("access-1"));
    assert_eq!(secrets.refresh_token.as_deref(), Some("refresh-1"));

    let document = f.document();
    assert!(!document.contains("access-1"), "token in {DOCUMENT}");
    assert!(!document.contains("refresh-1"), "token in {DOCUMENT}");
    assert!(document.contains("client-123"), "client id is not a secret");
}

#[test]
fn a_callback_with_another_state_is_rejected() {
    let _guard = serial();
    let f = Fixture::new();
    let server = oauth_server(Arc::new(AtomicBool::new(false)));
    let env = oauth_env(&server);
    f.enable_external(&env);

    let error = authorize(&f, &env, |redirect, _state| {
        assert_eq!(
            answer_redirect(redirect, "otro-estado", "code-123"),
            400,
            "the listener rejects a callback with another state"
        );
    })
    .expect_err("a mismatched state is not accepted");
    assert!(error.contains("no coincide"), "{error}");

    let snapshot = f.run(&env, Action::Snapshot).expect("snapshot");
    assert_eq!(
        provider(&snapshot, ProviderId::GoogleDrive).state,
        ConnectionState::Error
    );
    assert!(
        provider(&snapshot, ProviderId::GoogleDrive)
            .last_error
            .is_some()
    );
    assert_ne!(
        provider(&snapshot, ProviderId::GoogleDrive).state,
        ConnectionState::Connected
    );
}

#[test]
fn an_expired_authorization_is_refreshed_and_a_refused_refresh_marks_it_expired() {
    let _guard = serial();
    let f = Fixture::new();
    let refused = Arc::new(AtomicBool::new(false));
    let server = oauth_server(refused.clone());
    let env = oauth_env(&server);
    f.enable_external(&env);
    authorize(&f, &env, |redirect, state| {
        assert_eq!(answer_redirect(redirect, state, "code-123"), 200);
    })
    .expect("first authorization");

    // The stored expiry is in the past, so the next query refreshes the token.
    edit(&f.storage, ProviderId::GoogleDrive, |connection| {
        connection.expires_at = Some(now() - 10);
    })
    .expect("expiry moved to the past");
    let snapshot = f
        .run(
            &env,
            Action::Search {
                provider: ProviderId::GoogleDrive,
                query: "demo".into(),
            },
        )
        .expect("search after a refresh");
    assert_eq!(
        provider(&snapshot, ProviderId::GoogleDrive).state,
        ConnectionState::Connected
    );
    let secrets = f.vault.read(ProviderId::GoogleDrive).expect("vault");
    assert_eq!(secrets.access_token.as_deref(), Some("access-2"));

    // Without a usable refresh token the connection degrades instead of
    // pretending to work.
    refused.store(true, Ordering::Relaxed);
    edit(&f.storage, ProviderId::GoogleDrive, |connection| {
        connection.expires_at = Some(now() - 10);
    })
    .expect("expiry moved to the past");
    let error = f
        .run(
            &env,
            Action::Search {
                provider: ProviderId::GoogleDrive,
                query: "demo".into(),
            },
        )
        .expect_err("the provider refuses the refresh");
    assert!(error.contains("caducó"), "{error}");
    let snapshot = f.run(&env, Action::Snapshot).expect("snapshot");
    let drive = provider(&snapshot, ProviderId::GoogleDrive);
    assert_eq!(drive.state, ConnectionState::Expired);
    assert!(drive.last_error.is_some());
}

#[test]
fn losing_the_network_keeps_the_local_library_usable() {
    let _guard = serial();
    let f = Fixture::new();
    let env = plain_env();
    f.seed_library();
    f.enable_external(&env);
    f.run(
        &env,
        Action::Connect {
            provider: ProviderId::Webdav,
            fields: dav_fields(&unreachable_endpoint()),
        },
    )
    .expect("the connection itself only stores settings");

    let error = f
        .run(
            &env,
            Action::Search {
                provider: ProviderId::Webdav,
                query: "nocturne".into(),
            },
        )
        .expect_err("the provider is offline");
    assert!(error.contains("Sin conexión"), "{error}");

    let snapshot = f
        .run(
            &env,
            Action::Search {
                provider: ProviderId::Local,
                query: "nocturne".into(),
            },
        )
        .expect("local search");
    assert_eq!(snapshot.results.len(), 1);
    assert_eq!(snapshot.results[0].name, "Nocturne Drive");
}

#[test]
fn revoking_forgets_the_credential_of_every_provider_kind() {
    let _guard = serial();
    let f = Fixture::new();
    let refused = Arc::new(AtomicBool::new(false));
    let server = oauth_server(refused.clone());
    let dav = webdav_server();
    let env = oauth_env(&server);
    f.enable_external(&env);
    f.run(
        &env,
        Action::Connect {
            provider: ProviderId::Webdav,
            fields: dav_fields(&format!("{}/dav/", dav.base)),
        },
    )
    .expect("webdav connection");
    authorize(&f, &env, |redirect, state| {
        assert_eq!(answer_redirect(redirect, state, "code-123"), 200);
    })
    .expect("oauth connection");

    let snapshot = f
        .run(
            &env,
            Action::Revoke {
                provider: ProviderId::GoogleDrive,
            },
        )
        .expect("revoke");
    assert!(
        snapshot.warnings.is_empty(),
        "the provider confirmed the revoke: {:?}",
        snapshot.warnings
    );
    assert_eq!(
        provider(&snapshot, ProviderId::GoogleDrive).state,
        ConnectionState::Revoked
    );
    assert!(
        f.vault
            .read(ProviderId::GoogleDrive)
            .expect("vault")
            .is_empty()
    );

    // A provider without revoke support only forgets the local credential.
    let snapshot = f
        .run(
            &env,
            Action::Revoke {
                provider: ProviderId::Webdav,
            },
        )
        .expect("local revoke");
    assert!(
        snapshot.warnings.is_empty(),
        "no remote revoke is offered: {:?}",
        snapshot.warnings
    );
    assert!(f.vault.read(ProviderId::Webdav).expect("vault").is_empty());
    assert_eq!(
        provider(&snapshot, ProviderId::Webdav).state,
        ConnectionState::Disconnected
    );
}

// --- small helpers -----------------------------------------------------------

#[test]
fn dropbox_revocation_uses_bearer_auth_and_onedrive_only_disconnects() {
    let _guard = serial();
    cancel_pending(None);
    let f = Fixture::new();
    let server = mock_server(|request| {
        let valid = request.method == "POST"
            && request.path == "/2/auth/token/revoke"
            && request
                .headers
                .to_lowercase()
                .contains("authorization: bearer dropbox-access")
            && request.body.is_empty();
        (
            if valid { 200 } else { 400 },
            "application/json",
            b"{}".to_vec(),
        )
    });
    let mut env = plain_env();
    let mut dropbox = endpoints(ProviderId::Dropbox);
    assert!(
        dropbox
            .revoke
            .as_deref()
            .unwrap()
            .ends_with("/2/auth/token/revoke")
    );
    dropbox.revoke = Some(format!("{}/2/auth/token/revoke", server.base));
    env.endpoints.insert(ProviderId::Dropbox, dropbox);
    f.enable_external(&env);
    for id in [ProviderId::Dropbox, ProviderId::OneDrive] {
        f.vault
            .write(
                id,
                &Secrets {
                    access_token: Some("dropbox-access".into()),
                    ..Secrets::default()
                },
            )
            .expect("stored token");
        edit(&f.storage, id, |connection| {
            connection.state = ConnectionState::Connected;
            connection.has_secret = true;
        })
        .expect("connected state");
        let snapshot = f
            .run(&env, Action::Revoke { provider: id })
            .expect("remove access");
        assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
        assert!(f.vault.read(id).unwrap().is_empty());
        assert_eq!(
            provider(&snapshot, id).state,
            if id == ProviderId::Dropbox {
                ConnectionState::Revoked
            } else {
                ConnectionState::Disconnected
            }
        );
    }
    assert_eq!(server.hits.load(Ordering::Relaxed), 1);
}

#[test]
fn helper_rules_of_the_contract() {
    assert_eq!(iso_duration("PT1H2M3S"), "1:02:03");
    assert_eq!(iso_duration("PT4M5S"), "4:05");
    assert_eq!(iso_duration("PT12M"), "12:00");
    assert_eq!(audio_extension("tema.FLAC", None), Some("flac"));
    assert_eq!(audio_extension("portada.png", Some("image/png")), None);
    assert_eq!(
        audio_extension("portada.png", Some("audio/mpeg")),
        Some("mp3")
    );
    assert_eq!(entry_kind("tema.mp3", false, None), "audio");
    assert_eq!(entry_kind("nota.txt", false, None), "file");
    assert_eq!(entry_kind("cualquier cosa", true, None), "folder");
    assert_eq!(decode_entities("Caf&amp;eacute;"), "Caf&eacute;");
    assert_eq!(decode_entities("Rock &amp; Roll"), "Rock & Roll");
    assert_eq!(decode_path("canciones%2Funo"), "canciones/uno");
    assert_eq!(decode_path("a+b"), "a+b");
    assert_eq!(
        basic_auth("ana", "secreto"),
        format!("Basic {}", STANDARD.encode("ana:secreto"))
    );
    assert_eq!(percent_encode("a b/c"), "a%20b%2Fc");
    let parsed = parse_query("code=uno%20dos&state=abc");
    assert_eq!(parsed.get("code").map(String::as_str), Some("uno dos"));
    assert_eq!(parsed.get("state").map(String::as_str), Some("abc"));
}

#[test]
fn downloaded_audio_is_available_offline_and_excludes_unfinished_files() {
    let _guard = serial();
    let f = Fixture::new();
    let directory = f.storage.document_path("cache/audio");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("saved.mp3"), AUDIO_BYTES).unwrap();
    std::fs::write(directory.join("unfinished.tmp"), AUDIO_BYTES).unwrap();
    std::fs::create_dir_all(directory.join("folder.wav")).unwrap();
    let snapshot = f
        .run(&plain_env(), Action::Downloads)
        .expect("offline downloads");
    assert!(!snapshot.external);
    assert_eq!(snapshot.results.len(), 1);
    assert_eq!(snapshot.results[0].name, "saved.mp3");
    assert!(Path::new(&snapshot.results[0].id).is_file());
    assert!(f.vault.entries.lock().unwrap().is_empty());
}

#[test]
fn automatic_youtube_is_primary_without_secrets_and_respects_network_opt_in() {
    let _guard = serial();
    let fixture = Fixture::new();
    let env = plain_env();
    let snapshot = fixture.run(&env, Action::Snapshot).unwrap();
    let automatic = provider(&snapshot, ProviderId::YoutubeAuto);
    assert_eq!(automatic.state, ConnectionState::Connected);
    assert_eq!(automatic.info.auth, AuthKind::None);
    assert!(!automatic.has_secret);
    assert!(automatic.info.required.is_empty());
    assert!(automatic.info.capabilities.contains(&Capability::Playback));
    assert_eq!(snapshot.providers[1].info.id, ProviderId::YoutubeAuto);
    assert_eq!(snapshot.providers[2].info.id, ProviderId::Youtube);
    let error = fixture
        .run(
            &env,
            Action::Search {
                provider: ProviderId::YoutubeAuto,
                query: "Test".into(),
            },
        )
        .unwrap_err();
    assert!(error.contains("Activa"));
    fixture.enable_external(&env);
    let error = fixture
        .run(
            &env,
            Action::Search {
                provider: ProviderId::YoutubeAuto,
                query: "Test".into(),
            },
        )
        .unwrap_err();
    assert!(error.contains("Faltan los componentes"));
    assert!(!fixture.document().contains("api_key"));
    // A missing extractor never affects local search.
    fixture.seed_library();
    assert_eq!(
        fixture
            .run(
                &env,
                Action::Search {
                    provider: ProviderId::Local,
                    query: "Nocturne".into()
                }
            )
            .unwrap()
            .results
            .len(),
        1
    );
}

#[test]
fn oauth_callbacks_match_the_documented_provider_registrations() {
    let _guard = serial();
    cancel_pending(None);
    let env = plain_env();
    let settings = ProviderSettings {
        client_id: "client-test".into(),
        ..ProviderSettings::default()
    };
    for id in [ProviderId::Dropbox, ProviderId::OneDrive] {
        let url = begin_oauth(id, &settings, &env).unwrap();
        let parsed = reqwest::Url::parse(&url).unwrap();
        let redirect = parsed
            .query_pairs()
            .find(|(k, _)| k == "redirect_uri")
            .unwrap()
            .1
            .to_string();
        if id == ProviderId::Dropbox {
            assert_eq!(redirect, "http://127.0.0.1:53682/callback");
        } else {
            assert!(redirect.starts_with("http://localhost:"));
            assert!(redirect.ends_with("/callback"));
        }
        cancel_pending(None);
    }
}
