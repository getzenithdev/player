//! External music sources (roadmap phase 9).
//!
//! Every provider implements one contract: declared capabilities, a
//! connection state, network operations behind an explicit opt-in and
//! credentials that live in the operating system keyring instead of a JSON
//! document. Local content keeps working when a provider is unreachable,
//! expired or disabled, and playback always goes through files the audio
//! engine already understands.

use crate::storage::{StorageManager, indexes::TrackHit};
mod youtube;
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read as _, Write as _},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Connections, non-secret settings and the external opt-in.
const DOCUMENT: &str = "config/providers.json";
/// Service name used by the operating system credential store.
const VAULT_SERVICE: &str = "Zenith Player";
const USER_AGENT: &str = "ZenithPlayer/0.1.0 (https://github.com/getzenithdev/player)";
const MAX_JSON: usize = 4 * 1024 * 1024;
const MAX_XML: usize = 4 * 1024 * 1024;
const MAX_AUDIO: u64 = 128 * 1024 * 1024;
const MAX_QUERY: usize = 200;
const MAX_RESULTS: usize = 200;
const MAX_ENTRIES: usize = 5_000;
const MAX_HEAD: usize = 16 * 1024;
const AUTH_TIMEOUT: Duration = Duration::from_secs(300);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Operations a provider is able to perform. The interface must not offer an
/// action the provider does not declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Auth,
    Search,
    Read,
    Playback,
    Download,
    Revoke,
}

/// How a provider proves who the user is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    None,
    ApiKey,
    OAuth2,
    Password,
}

/// Stable identifier of a source, used as JSON key and vault account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProviderId {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "youtube")]
    Youtube,
    #[serde(rename = "youtube-auto")]
    YoutubeAuto,
    #[serde(rename = "google-drive")]
    GoogleDrive,
    #[serde(rename = "dropbox")]
    Dropbox,
    #[serde(rename = "onedrive")]
    OneDrive,
    #[serde(rename = "webdav")]
    Webdav,
}

impl ProviderId {
    fn key(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Youtube => "youtube",
            Self::YoutubeAuto => "youtube-auto",
            Self::GoogleDrive => "google-drive",
            Self::Dropbox => "dropbox",
            Self::OneDrive => "onedrive",
            Self::Webdav => "webdav",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Local => "Biblioteca local",
            Self::Youtube => "YouTube API (alternativa)",
            Self::YoutubeAuto => "YouTube automático",
            Self::GoogleDrive => "Google Drive",
            Self::Dropbox => "Dropbox",
            Self::OneDrive => "OneDrive",
            Self::Webdav => "WebDAV",
        }
    }

    fn is_oauth(self) -> bool {
        matches!(self, Self::GoogleDrive | Self::Dropbox | Self::OneDrive)
    }
}

/// Lifecycle of one connection. Secrets are never part of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Pending,
    Connected,
    Expired,
    Revoked,
    Error,
}

/// Persisted, non-secret state of a provider connection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Connection {
    pub state: ConnectionState,
    /// Account label or WebDAV root, shown without exposing credentials.
    pub label: String,
    pub endpoint: String,
    pub connected_at: u64,
    pub expires_at: Option<u64>,
    pub last_error: Option<String>,
    /// Whether a credential exists in the keyring; never the credential.
    pub has_secret: bool,
}

/// Non-secret configuration the user provides for a provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSettings {
    pub client_id: String,
    pub endpoint: String,
    pub username: String,
}

/// Secrets of one provider, serialized only into the credential store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Secrets {
    password: Option<String>,
    api_key: Option<String>,
    client_secret: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
}

impl Secrets {
    fn is_empty(&self) -> bool {
        self.password.is_none()
            && self.api_key.is_none()
            && self.access_token.is_none()
            && self.refresh_token.is_none()
            && self.client_secret.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Document {
    external: bool,
    connections: BTreeMap<ProviderId, Connection>,
    settings: BTreeMap<ProviderId, ProviderSettings>,
}

/// One browsable or searchable item returned by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub provider: ProviderId,
    pub id: String,
    pub name: String,
    /// `folder`, `audio`, `video` or `file`.
    pub kind: String,
    pub detail: String,
    pub size: Option<u64>,
    pub source_url: Option<String>,
}

/// Static description of a provider: contract, not state.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: ProviderId,
    pub name: String,
    pub auth: AuthKind,
    pub capabilities: Vec<Capability>,
    /// Fields the connection form has to collect.
    pub required: Vec<String>,
    pub note: String,
}

/// Provider description combined with its stored connection.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    #[serde(flatten)]
    pub info: ProviderInfo,
    pub state: ConnectionState,
    pub label: String,
    pub endpoint: String,
    pub expires_at: Option<u64>,
    pub last_error: Option<String>,
    pub has_secret: bool,
    pub settings: ProviderSettings,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub providers: Vec<ProviderStatus>,
    pub results: Vec<Entry>,
    pub external: bool,
    /// Authorization URL of a connection waiting for the browser.
    pub pending_url: Option<String>,
    /// Absolute local path of the last download, ready for the audio engine.
    pub downloaded: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Snapshot,
    CancelYoutube,
    OpenGuide {
        provider: ProviderId,
    },
    Downloads,
    External {
        enabled: bool,
    },
    Settings {
        provider: ProviderId,
        settings: ProviderSettings,
    },
    Connect {
        provider: ProviderId,
        fields: BTreeMap<String, String>,
    },
    Finish {
        provider: ProviderId,
    },
    Disconnect {
        provider: ProviderId,
    },
    Revoke {
        provider: ProviderId,
    },
    Search {
        provider: ProviderId,
        query: String,
    },
    Browse {
        provider: ProviderId,
        path: Option<String>,
    },
    Download {
        provider: ProviderId,
        id: String,
    },
}

/// Static provider contract of the application.
fn registry() -> Vec<ProviderInfo> {
    let oauth_note = "OAuth 2.0 con PKCE usando tu propio ID de cliente. Los tokens se guardan en el almacén de credenciales del sistema.";
    vec![
        ProviderInfo {
            id: ProviderId::Local,
            name: ProviderId::Local.label().into(),
            auth: AuthKind::None,
            capabilities: vec![Capability::Search, Capability::Read, Capability::Playback],
            required: vec![],
            note: "Biblioteca y reproducción locales, sin red.".into(),
        },
        ProviderInfo {
            id: ProviderId::YoutubeAuto,
            name: ProviderId::YoutubeAuto.label().into(),
            auth: AuthKind::None,
            capabilities: vec![Capability::Search, Capability::Read, Capability::Playback, Capability::Download],
            required: vec![],
            note: "Opción principal, sin API key. Usa yt-dlp local para buscar y preparar audio AAC/M4A en caché antes de reproducir. Puede fallar por cambios o restricciones de YouTube; la API alternativa solo ofrece metadatos.".into(),
        },
        ProviderInfo {
            id: ProviderId::Youtube,
            name: ProviderId::Youtube.label().into(),
            auth: AuthKind::ApiKey,
            capabilities: vec![Capability::Auth, Capability::Search, Capability::Read],
            required: vec!["api_key".into()],
            note: "Opción alternativa: YouTube Data API con clave propia para búsqueda y metadatos; no proporciona audio ni sustituye la extracción cuando falla."
                .into(),
        },
        ProviderInfo {
            id: ProviderId::GoogleDrive,
            name: ProviderId::GoogleDrive.label().into(),
            auth: AuthKind::OAuth2,
            capabilities: vec![
                Capability::Auth,
                Capability::Search,
                Capability::Read,
                Capability::Playback,
                Capability::Download,
                Capability::Revoke,
            ],
            required: vec!["client_id".into()],
            note: oauth_note.into(),
        },
        ProviderInfo {
            id: ProviderId::Dropbox,
            name: ProviderId::Dropbox.label().into(),
            auth: AuthKind::OAuth2,
            capabilities: vec![
                Capability::Auth,
                Capability::Search,
                Capability::Read,
                Capability::Playback,
                Capability::Download,
                Capability::Revoke,
            ],
            required: vec!["client_id".into()],
            note: oauth_note.into(),
        },
        ProviderInfo {
            id: ProviderId::OneDrive,
            name: ProviderId::OneDrive.label().into(),
            auth: AuthKind::OAuth2,
            capabilities: vec![
                Capability::Auth,
                Capability::Search,
                Capability::Read,
                Capability::Playback,
                Capability::Download,
            ],
            required: vec!["client_id".into()],
            note: format!("{oauth_note} Desconectar elimina las credenciales locales; retira el consentimiento desde tu cuenta Microsoft."),
        },
        ProviderInfo {
            id: ProviderId::Webdav,
            name: ProviderId::Webdav.label().into(),
            auth: AuthKind::Password,
            capabilities: vec![
                Capability::Auth,
                Capability::Search,
                Capability::Read,
                Capability::Playback,
                Capability::Download,
            ],
            required: vec!["endpoint".into(), "username".into(), "password".into()],
            note: "HTTPS con usuario y contraseña. La descarga se guarda en la caché local para reproducirla."
                .into(),
        },
    ]
}

#[cfg(test)]
fn spec(provider: ProviderId) -> ProviderInfo {
    registry()
        .into_iter()
        .find(|item| item.id == provider)
        .expect("every provider of the contract has a specification")
}

/// Endpoints of one provider. Tests replace them with a local server.
#[derive(Debug, Clone)]
struct Endpoints {
    authorize: String,
    token: String,
    revoke: Option<String>,
    api: String,
    /// Extra base used for downloads when the content host differs.
    content: String,
}

fn endpoints(provider: ProviderId) -> Endpoints {
    match provider {
        ProviderId::GoogleDrive => Endpoints {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: Some("https://oauth2.googleapis.com/revoke".into()),
            api: "https://www.googleapis.com/drive/v3/".into(),
            content: String::new(),
        },
        ProviderId::Dropbox => Endpoints {
            authorize: "https://www.dropbox.com/oauth2/authorize".into(),
            token: "https://api.dropboxapi.com/oauth2/token".into(),
            revoke: Some("https://api.dropboxapi.com/2/auth/token/revoke".into()),
            api: "https://api.dropboxapi.com/2/".into(),
            content: "https://content.dropboxapi.com/2/".into(),
        },
        ProviderId::OneDrive => Endpoints {
            authorize: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
            token: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
            revoke: None,
            api: "https://graph.microsoft.com/v1.0/".into(),
            content: String::new(),
        },
        ProviderId::Youtube => Endpoints {
            authorize: String::new(),
            token: String::new(),
            revoke: None,
            api: "https://www.googleapis.com/youtube/v3/".into(),
            content: String::new(),
        },
        ProviderId::Local | ProviderId::Webdav | ProviderId::YoutubeAuto => Endpoints {
            authorize: String::new(),
            token: String::new(),
            revoke: None,
            api: String::new(),
            content: String::new(),
        },
    }
}

fn scope(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::GoogleDrive => "https://www.googleapis.com/auth/drive.readonly",
        ProviderId::Dropbox => "files.metadata.read files.content.read account_info.read",
        ProviderId::OneDrive => "Files.Read offline_access",
        _ => "",
    }
}

/// Test hooks. Production uses [`Env::default`].
#[derive(Debug, Clone)]
struct Env {
    open_browser: bool,
    endpoints: BTreeMap<ProviderId, Endpoints>,
    youtube_tools: PathBuf,
}

impl Default for Env {
    fn default() -> Self {
        Self {
            open_browser: true,
            endpoints: BTreeMap::new(),
            youtube_tools: std::env::current_exe()
                .unwrap_or_default()
                .with_file_name("tools"),
        }
    }
}

impl Env {
    fn endpoints(&self, provider: ProviderId) -> Endpoints {
        self.endpoints
            .get(&provider)
            .cloned()
            .unwrap_or_else(|| endpoints(provider))
    }
}

/// Credential store. Production writes to the operating system keyring; tests
/// use an in-memory implementation so no platform service is required.
trait SecretStore: Send + Sync {
    fn write(&self, provider: ProviderId, secrets: &Secrets) -> Result<(), String>;
    fn read(&self, provider: ProviderId) -> Result<Secrets, String>;
    fn remove(&self, provider: ProviderId) -> Result<(), String>;
}

struct OsVault;

impl SecretStore for OsVault {
    fn write(&self, provider: ProviderId, secrets: &Secrets) -> Result<(), String> {
        let raw =
            serde_json::to_string(secrets).map_err(|_| "Credenciales inválidas".to_string())?;
        entry(provider)?
            .set_password(&raw)
            .map_err(|error| format!("No se pudo guardar la credencial: {error}"))
    }

    fn read(&self, provider: ProviderId) -> Result<Secrets, String> {
        let entry = entry(provider)?;
        match entry.get_password() {
            Ok(raw) => serde_json::from_str(&raw).map_err(|_| "Credencial dañada".to_string()),
            Err(keyring::Error::NoEntry) => Ok(Secrets::default()),
            Err(error) => Err(format!("Almacén de credenciales no disponible: {error}")),
        }
    }

    fn remove(&self, provider: ProviderId) -> Result<(), String> {
        match entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!("No se pudo olvidar la credencial: {error}")),
        }
    }
}

fn entry(provider: ProviderId) -> Result<keyring::Entry, String> {
    keyring::Entry::new(VAULT_SERVICE, provider.key())
        .map_err(|error| format!("Almacén de credenciales no disponible: {error}"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load(storage: &StorageManager) -> Result<Document, String> {
    storage
        .load_document::<Document>(DOCUMENT)
        .map(|stored| stored.map(|stored| stored.data).unwrap_or_default())
        .map_err(|error| error.to_string())
}

fn save(storage: &StorageManager, document: &Document) -> Result<(), String> {
    storage
        .save_document(DOCUMENT, document)
        .map_err(|error| error.to_string())
}

/// Network actions are refused until the user opts in, like lyrics and artwork.
fn ensure_external(storage: &StorageManager) -> Result<(), String> {
    let external = {
        let _lock = storage.library_lock();
        load(storage)?.external
    };
    if external {
        Ok(())
    } else {
        Err("Activa las fuentes externas primero".into())
    }
}

fn edit(
    storage: &StorageManager,
    provider: ProviderId,
    change: impl FnOnce(&mut Connection),
) -> Result<(), String> {
    let _lock = storage.library_lock();
    let mut document = load(storage)?;
    let connection = document.connections.entry(provider).or_default();
    change(connection);
    save(storage, &document)
}

// --- bounded transport -------------------------------------------------------

struct Reply {
    status: u16,
    body: Vec<u8>,
}

impl Reply {
    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    fn text(&self) -> Result<&str, String> {
        std::str::from_utf8(&self.body).map_err(|_| "Respuesta no legible".to_string())
    }

    fn json(&self) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&self.body).map_err(|_| "Respuesta inválida del proveedor".into())
    }
}

/// HTTPS everywhere, except loopback HTTP used by the OAuth redirect and by
/// local tests. Credentials inside URLs are rejected.
fn secure_url(raw: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "URL inválida".to_string())?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if !url.username().is_empty() || url.password().is_some() {
        return Err("La dirección no debe incluir credenciales".into());
    }
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err("La fuente debe usar HTTPS".into());
    }
    Ok(url)
}

fn client(timeout: Duration) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| error.to_string())
}

fn send(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    body: Option<&[u8]>,
    max: usize,
) -> Result<Reply, String> {
    send_with(method, url, headers, body, max, REQUEST_TIMEOUT)
}

fn send_with(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    body: Option<&[u8]>,
    max: usize,
    timeout: Duration,
) -> Result<Reply, String> {
    let target = secure_url(url)?;
    let method: reqwest::Method = method
        .parse()
        .map_err(|_| "Método no admitido".to_string())?;
    let mut request = client(timeout)?.request(method, target);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    if let Some(body) = body {
        request = request.body(body.to_vec());
    }
    let response = request
        .send()
        .map_err(|_| "Sin conexión con el proveedor".to_string())?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    response
        .take(max as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|_| "Respuesta interrumpida".to_string())?;
    if body.len() > max {
        return Err("Respuesta demasiado grande".into());
    }
    Ok(Reply { status, body })
}

fn token_header(token: &str) -> Vec<(&'static str, String)> {
    vec![("Authorization", format!("Bearer {token}"))]
}

fn basic_auth(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    )
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

// --- WebDAV ------------------------------------------------------------------

const PROPFIND_BODY: &str = "<?xml version=\"1.0\" encoding=\"utf-8\" ?><d:propfind xmlns:d=\"DAV:\"><d:prop><d:displayname/><d:getcontentlength/><d:getcontenttype/><d:resourcetype/></d:prop></d:propfind>";

/// Local name of the tag starting at `start` and the position right after its
/// closing `>`. Prefixes, case and quoted attribute values are handled.
fn tag_local(xml: &str, start: usize) -> Option<(&str, usize)> {
    let body = &xml[start + 1..];
    let name_end = body
        .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
        .unwrap_or(body.len());
    let name = &body[..name_end];
    if name.is_empty()
        || name.starts_with('/')
        || !name.starts_with(|c: char| c.is_ascii_alphabetic())
    {
        return None;
    }
    let mut index = name_end;
    let mut quote = None;
    while index < body.len() {
        let character = body.as_bytes()[index] as char;
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '>' => {
                let local = name.rsplit(':').next().unwrap_or(name);
                return Some((local, start + index + 2));
            }
            None => {}
        }
        index += 1;
    }
    None
}

/// Position of the closing tag of `wanted` at or after `from`.
fn find_close(xml: &str, from: usize, wanted: &str) -> Option<usize> {
    let mut cursor = from;
    while let Some(offset) = xml[cursor..].find('<') {
        let start = cursor + offset;
        let rest = &xml[start + 1..];
        if !rest.starts_with('/') {
            cursor = start + 1;
            continue;
        }
        let body = &rest[1..];
        let name_end = body
            .find(|c: char| c.is_ascii_whitespace() || c == '>')
            .unwrap_or(body.len());
        let name = &body[..name_end];
        let local = name.rsplit(':').next().unwrap_or(name);
        if local.eq_ignore_ascii_case(wanted) {
            return Some(start);
        }
        cursor = start + 1;
    }
    None
}

/// Ranges of the inner text of every element whose local name matches.
fn elements(xml: &str, wanted: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut from = 0;
    while let Some(offset) = xml[from..].find('<') {
        let start = from + offset;
        let Some((local, after)) = tag_local(xml, start) else {
            from = start + 1;
            continue;
        };
        if !local.eq_ignore_ascii_case(wanted) {
            from = start + 1;
            continue;
        }
        if xml[..after].ends_with("/>") {
            ranges.push((after, after));
            from = after;
            continue;
        }
        match find_close(xml, after, wanted) {
            Some(close) => {
                ranges.push((after, close));
                from = close + 1;
            }
            None => from = after,
        }
    }
    ranges
}

fn decode_entities(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(';').map(|offset| start + offset) else {
            output.push('&');
            return output;
        };
        let entity = &rest[start + 1..end];
        match entity {
            "amp" => output.push('&'),
            "lt" => output.push('<'),
            "gt" => output.push('>'),
            "quot" => output.push('"'),
            "apos" => output.push('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                match u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(character) => output.push(character),
                    None => output.push_str(&rest[start..=end]),
                }
            }
            _ if entity.starts_with('#') => {
                match entity[1..].parse::<u32>().ok().and_then(char::from_u32) {
                    Some(character) => output.push(character),
                    None => output.push_str(&rest[start..=end]),
                }
            }
            _ => output.push_str(&rest[start..=end]),
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}

fn element_text(block: &str, wanted: &str) -> Option<String> {
    let (start, end) = elements(block, wanted).into_iter().next()?;
    Some(decode_entities(block[start..end].trim()))
}

fn has_element(block: &str, wanted: &str) -> bool {
    !elements(block, wanted).is_empty()
}

/// Decodes a percent-encoded path segment without decoding `+` as a space.
fn decode_path(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16)
        {
            output.push(byte);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

/// Relative, traversal-free identifier used as the stored entry id.
fn relative_id(path: &str) -> Result<String, String> {
    let parts: Vec<String> = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .map(decode_path)
        .collect();
    if parts.iter().any(|part| part == ".." || part.contains('\0')) {
        return Err("Ruta no admitida".into());
    }
    Ok(parts.join("/"))
}

/// Validates an identifier that came back from the interface.
fn safe_id(id: &str) -> Result<String, String> {
    if id.is_empty() || id.len() > 1024 || id.chars().any(|c| c.is_control()) {
        return Err("Identificador inválido".into());
    }
    relative_id(id)
}

fn webdav_url(endpoint: &str, id: &str) -> Result<reqwest::Url, String> {
    let mut url = secure_url(endpoint)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "Dirección WebDAV inválida".to_string())?;
        segments.pop_if_empty();
        for part in id.split('/').filter(|part| !part.is_empty()) {
            segments.push(part);
        }
    }
    Ok(url)
}

fn audio_extension(name: &str, content_type: Option<&str>) -> Option<&'static str> {
    let known = match Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp3") => Some("mp3"),
        Some("flac") => Some("flac"),
        Some("wav") => Some("wav"),
        Some("ogg" | "oga") => Some("ogg"),
        Some("opus") => Some("ogg"),
        Some("m4a" | "mp4") => Some("m4a"),
        Some("aac") => Some("aac"),
        Some("aiff" | "aif" | "aifc") => Some("aiff"),
        Some("caf") => Some("caf"),
        _ => None,
    };
    if known.is_some() {
        return known;
    }
    match content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default()
    {
        "audio/mpeg" => Some("mp3"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        "audio/ogg" | "application/ogg" => Some("ogg"),
        "audio/mp4" | "audio/x-m4a" | "audio/aac" => Some("m4a"),
        "audio/aiff" | "audio/x-aiff" => Some("aiff"),
        _ => None,
    }
}

fn is_audio_name(name: &str) -> bool {
    audio_extension(name, None).is_some()
}

fn entry_kind(name: &str, folder: bool, content_type: Option<&str>) -> String {
    if folder {
        return "folder".into();
    }
    if is_audio_name(name) || audio_extension(name, content_type).is_some() {
        return "audio".into();
    }
    "file".into()
}

fn webdav_auth(settings: &ProviderSettings, secrets: &Secrets) -> Result<String, String> {
    let password = secrets
        .password
        .as_deref()
        .ok_or("Falta la contraseña de la fuente")?;
    if settings.username.is_empty() {
        return Err("Falta el usuario de la fuente".into());
    }
    Ok(basic_auth(&settings.username, password))
}

fn webdav_entries(
    settings: &ProviderSettings,
    secrets: &Secrets,
    id: &str,
) -> Result<Vec<Entry>, String> {
    let url = webdav_url(&settings.endpoint, id)?;
    let auth = webdav_auth(settings, secrets)?;
    let headers = [("Authorization", auth), ("Depth", "1".into())];
    let reply = send(
        "PROPFIND",
        url.as_str(),
        &headers,
        Some(PROPFIND_BODY.as_bytes()),
        MAX_XML,
    )?;
    if reply.status == 401 {
        return Err("Usuario o contraseña rechazados".into());
    }
    if !reply.ok() {
        return Err(format!("WebDAV respondió {}", reply.status));
    }
    let xml = reply.text()?;
    if xml.len() > MAX_XML {
        return Err("Respuesta WebDAV demasiado grande".into());
    }
    let base = url.clone();
    let mut entries = Vec::new();
    for (start, end) in elements(xml, "response") {
        if entries.len() >= MAX_ENTRIES {
            return Err("Demasiadas entradas en la carpeta".into());
        }
        let block = &xml[start..end];
        let status = element_text(block, "status").unwrap_or_default();
        if !status.contains(" 200") && !status.contains(" 204") {
            continue;
        }
        let Some(href) = element_text(block, "href") else {
            continue;
        };
        let folder = has_element(block, "collection");
        let name = element_text(block, "displayname")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                href.rsplit('/')
                    .find(|part| !part.is_empty())
                    .map(decode_path)
                    .unwrap_or_default()
            });
        if name.is_empty() {
            continue;
        }
        let target = reqwest::Url::parse(&href)
            .or_else(|_| base.join(&href))
            .map_err(|_| "Referencia WebDAV inválida".to_string())?;
        let prefix = base.path().trim_end_matches('/');
        let mut path = target.path();
        if !prefix.is_empty() && path.starts_with(prefix) {
            path = &path[prefix.len()..];
        }
        let id = relative_id(path)?;
        if id.is_empty() {
            continue;
        }
        let size =
            element_text(block, "getcontentlength").and_then(|value| value.parse::<u64>().ok());
        let content_type = element_text(block, "getcontenttype");
        let kind = entry_kind(&name, folder, content_type.as_deref());
        entries.push(Entry {
            provider: ProviderId::Webdav,
            id,
            name,
            kind,
            detail: if folder {
                "Carpeta".into()
            } else {
                content_type.unwrap_or_else(|| "Archivo".into())
            },
            size,
            source_url: None,
        });
    }
    entries.truncate(MAX_RESULTS);
    Ok(entries)
}

fn webdav_search(
    settings: &ProviderSettings,
    secrets: &Secrets,
    query: &str,
) -> Result<Vec<Entry>, String> {
    let needle = query.to_lowercase();
    Ok(webdav_entries(settings, secrets, "")?
        .into_iter()
        .filter(|entry| entry.name.to_lowercase().contains(&needle))
        .collect())
}

// --- OAuth 2 -----------------------------------------------------------------

struct Pending {
    provider: ProviderId,
    url: String,
    deadline: Instant,
    verifier: String,
    redirect_uri: String,
    rx: Receiver<Result<String, String>>,
    cancel: Arc<AtomicBool>,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

fn random_token(bytes: usize) -> String {
    let mut raw = Vec::with_capacity(bytes);
    while raw.len() < bytes {
        raw.extend_from_slice(&rand::random::<u64>().to_le_bytes());
    }
    raw.truncate(bytes);
    URL_SAFE_NO_PAD.encode(raw)
}

fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn authorize_url(
    provider: ProviderId,
    endpoints: &Endpoints,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    verifier: &str,
) -> Result<String, String> {
    let mut url = reqwest::Url::parse(&endpoints.authorize)
        .map_err(|_| "Dirección de autorización inválida".to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state)
            .append_pair("scope", scope(provider))
            .append_pair("code_challenge", &challenge(verifier))
            .append_pair("code_challenge_method", "S256");
        match provider {
            ProviderId::GoogleDrive => {
                query.append_pair("access_type", "offline");
            }
            ProviderId::Dropbox => {
                query.append_pair("token_access_type", "offline");
            }
            _ => {}
        }
    }
    Ok(url.to_string())
}

/// Opens the authorization page in the default browser without a shell, so
/// query parameters are never interpreted by a command interpreter.
fn open_browser(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") || url.contains(['"', '\n', '\r']) {
        return Err("Dirección de autorización no admitida".into());
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler").arg(url);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    command
        .status()
        .map(|_| ())
        .map_err(|_| "No se pudo abrir el navegador".to_string())
}

fn begin_oauth(
    provider: ProviderId,
    settings: &ProviderSettings,
    env: &Env,
) -> Result<String, String> {
    let client_id = settings.client_id.trim();
    if client_id.is_empty() {
        return Err("Configura el ID de cliente del proveedor".into());
    }
    let endpoints = env.endpoints(provider);
    let state = format!("{:032x}", rand::random::<u128>());
    let verifier = random_token(32);
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx_port, rx_port) = mpsc::channel();
    let (tx_result, rx_result) = mpsc::channel();
    let listener_cancel = cancel.clone();
    let listener_state = state.clone();
    // Dropbox requires an exact registered URI; Microsoft ignores the port
    // only for localhost URIs. Google desktop clients allow an ephemeral port.
    let bind_address = if provider == ProviderId::Dropbox {
        "127.0.0.1:53682"
    } else {
        "127.0.0.1:0"
    };
    let listener = TcpListener::bind(bind_address).map_err(|_| {
        if provider == ProviderId::Dropbox {
            "El puerto 53682 de Dropbox está ocupado"
        } else {
            "No se pudo preparar la autorización local"
        }
        .to_string()
    })?;
    std::thread::spawn(move || {
        listen_for_redirect(
            listener,
            provider,
            listener_cancel,
            listener_state,
            tx_port,
            tx_result,
        );
    });
    let redirect_uri = rx_port
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "No se pudo preparar la autorización".to_string())??;
    let url = authorize_url(
        provider,
        &endpoints,
        client_id,
        &redirect_uri,
        &state,
        &verifier,
    )?;
    {
        let mut pending = PENDING.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(previous) = pending.take() {
            previous.cancel.store(true, Ordering::Relaxed);
        }
        *pending = Some(Pending {
            provider,
            url: url.clone(),
            deadline: Instant::now() + AUTH_TIMEOUT,
            verifier,
            redirect_uri,
            rx: rx_result,
            cancel,
        });
    }
    // The authorization URL stays visible in the interface, so a browser that
    // cannot be launched never blocks the connection.
    if env.open_browser {
        let _ = open_browser(&url);
    }
    Ok(url)
}

fn listen_for_redirect(
    listener: TcpListener,
    provider: ProviderId,
    cancel: Arc<AtomicBool>,
    expected_state: String,
    tx_port: mpsc::Sender<Result<String, String>>,
    tx_result: mpsc::Sender<Result<String, String>>,
) {
    let Ok(port) = listener.local_addr().map(|address| address.port()) else {
        return;
    };
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    let host = if provider == ProviderId::OneDrive {
        "localhost"
    } else {
        "127.0.0.1"
    };
    let _ = tx_port.send(Ok(format!("http://{host}:{port}/callback")));
    let deadline = Instant::now() + AUTH_TIMEOUT;
    let mut answered = false;
    while Instant::now() < deadline && !cancel.load(Ordering::Relaxed) {
        let Ok((mut stream, _)) = listener.accept() else {
            std::thread::sleep(Duration::from_millis(120));
            continue;
        };
        // The listening socket is non blocking; the accepted one must block or
        // the first read would fail while the browser is still sending.
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut head = Vec::new();
        let mut chunk = [0u8; 1024];
        while head.len() < MAX_HEAD {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    head.extend_from_slice(&chunk[..read]);
                    if head.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let request = String::from_utf8_lossy(&head);
        let target = request.split_whitespace().nth(1).unwrap_or_default();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if path != "/callback" {
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            continue;
        }
        let parameters = parse_query(query);
        let response = if parameters.get("state").map(String::as_str)
            != Some(expected_state.as_str())
        {
            answered = true;
            let _ = tx_result.send(Err("La autorización no coincide con esta solicitud".into()));
            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<html><body><p>Estado no reconocido. Cierra esta ventana.</p></body></html>"
        } else if let Some(error) = parameters.get("error") {
            answered = true;
            let _ = tx_result.send(Err(if error == "access_denied" {
                "Autorización cancelada por el usuario".into()
            } else {
                format!("El proveedor respondió: {error}")
            }));
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<html><body><p>Cancelado. Puedes cerrar esta ventana.</p></body></html>"
        } else if let Some(code) = parameters.get("code") {
            answered = true;
            let _ = tx_result.send(Ok(code.clone()));
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<html><body><p>Conectado. Puedes cerrar esta ventana.</p></body></html>"
        } else {
            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<html><body><p>Falta el código. Cierra esta ventana.</p></body></html>"
        };
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        if answered {
            break;
        }
    }
    if !answered {
        let _ = tx_result.send(Err("Se agotó el tiempo de autorización".into()));
    }
}

fn parse_query(query: &str) -> BTreeMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| {
            (
                decode_path(&value.replace('+', " ")),
                decode_path(&key.replace('+', " ")),
            )
        })
        .map(|(value, key)| (key, value))
        .collect()
}

struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
}

fn token_result(reply: &Reply, previous: &Secrets) -> Result<Tokens, String> {
    #[derive(Deserialize)]
    struct TokenJson {
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_in: Option<u64>,
        error: Option<String>,
        error_description: Option<String>,
    }
    let parsed: Option<TokenJson> = serde_json::from_slice(&reply.body).ok();
    if let Some(error) = parsed.as_ref().and_then(|value| value.error.clone()) {
        return Err(match error.as_str() {
            "invalid_grant" => "La autorización caducó o fue revocada".into(),
            "access_denied" => "Autorización cancelada por el usuario".into(),
            other => parsed
                .as_ref()
                .and_then(|value| value.error_description.clone())
                .map(|description| format!("No se pudo autorizar: {description}"))
                .unwrap_or_else(|| format!("No se pudo autorizar: {other}")),
        });
    }
    if !reply.ok() {
        return Err(format!("El proveedor respondió {}", reply.status));
    }
    let parsed = parsed.ok_or("Respuesta de autorización inválida")?;
    let access_token = parsed
        .access_token
        .ok_or("El proveedor no devolvió un token")?;
    let expires_at = parsed
        .expires_in
        .map(|seconds| now() + seconds.saturating_sub(30))
        .or(Some(now() + 3600));
    Ok(Tokens {
        access_token,
        refresh_token: parsed
            .refresh_token
            .filter(|value| !value.is_empty())
            .or_else(|| previous.refresh_token.clone()),
        expires_at,
    })
}

fn token_request(
    endpoints: &Endpoints,
    settings: &ProviderSettings,
    secrets: &Secrets,
    pairs: &[(&str, &str)],
) -> Result<Tokens, String> {
    let mut fields: Vec<(&str, &str)> = pairs.to_vec();
    fields.push(("client_id", settings.client_id.trim()));
    if let Some(client_secret) = secrets.client_secret.as_deref() {
        fields.push(("client_secret", client_secret));
    }
    let body = form(&fields);
    let reply = send(
        "POST",
        &endpoints.token,
        &[("Content-Type", "application/x-www-form-urlencoded".into())],
        Some(body.as_bytes()),
        MAX_JSON,
    )?;
    token_result(&reply, secrets)
}

fn exchange_code(
    endpoints: &Endpoints,
    settings: &ProviderSettings,
    secrets: &Secrets,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<Tokens, String> {
    token_request(
        endpoints,
        settings,
        secrets,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
        ],
    )
}

fn refresh_access(
    endpoints: &Endpoints,
    settings: &ProviderSettings,
    secrets: &Secrets,
) -> Result<Tokens, String> {
    let refresh = secrets
        .refresh_token
        .as_deref()
        .ok_or("La autorización caducó. Conecta de nuevo")?;
    token_request(
        endpoints,
        settings,
        secrets,
        &[("grant_type", "refresh_token"), ("refresh_token", refresh)],
    )
}

/// Best-effort account label; a failure never blocks the connection.
fn account_label(endpoints: &Endpoints, provider: ProviderId, token: &str) -> Option<String> {
    let headers = token_header(token);
    let value = match provider {
        ProviderId::GoogleDrive => {
            let reply = send(
                "GET",
                &format!("{}about?fields=user", endpoints.api),
                &headers,
                None,
                MAX_JSON,
            )
            .ok()?;
            reply
                .json()
                .ok()?
                .get("user")?
                .get("emailAddress")?
                .as_str()?
                .to_string()
        }
        ProviderId::Dropbox => {
            let reply = send(
                "POST",
                &format!("{}users/get_current_account", endpoints.api),
                &[
                    ("Authorization", format!("Bearer {token}")),
                    ("Content-Type", "application/json".into()),
                ],
                Some(b"{}"),
                MAX_JSON,
            )
            .ok()?;
            let json = reply.json().ok()?;
            json.get("email")
                .or_else(|| json.get("name").and_then(|name| name.get("display_name")))?
                .as_str()?
                .to_string()
        }
        ProviderId::OneDrive => {
            let reply = send(
                "GET",
                &format!("{}me", endpoints.api),
                &headers,
                None,
                MAX_JSON,
            )
            .ok()?;
            let json = reply.json().ok()?;
            json.get("userPrincipalName")
                .or_else(|| json.get("mail"))
                .or_else(|| json.get("displayName"))?
                .as_str()?
                .to_string()
        }
        _ => return None,
    };
    Some(value)
}

fn finish_oauth(
    storage: &StorageManager,
    store: &dyn SecretStore,
    env: &Env,
    provider: ProviderId,
) -> Result<(), String> {
    let pending = {
        let mut guard = PENDING.lock().unwrap_or_else(|error| error.into_inner());
        let pending = guard.take().ok_or("No hay una conexión en espera")?;
        if pending.provider != provider {
            *guard = Some(pending);
            return Err("La conexión en espera pertenece a otra fuente".into());
        }
        pending
    };
    let received = pending.rx.recv_timeout(AUTH_TIMEOUT);
    // The listener is not needed anymore as soon as the browser answered.
    pending.cancel.store(true, Ordering::Relaxed);
    let code = received.map_err(|_| "Se agotó el tiempo de autorización".to_string())??;
    let settings = {
        let _lock = storage.library_lock();
        load(storage)?
            .settings
            .get(&provider)
            .cloned()
            .unwrap_or_default()
    };
    let mut secrets = store.read(provider)?;
    let endpoints = env.endpoints(provider);
    let tokens = exchange_code(
        &endpoints,
        &settings,
        &secrets,
        &code,
        &pending.redirect_uri,
        &pending.verifier,
    )?;
    let label = account_label(&endpoints, provider, &tokens.access_token)
        .unwrap_or_else(|| provider.label().to_string());
    secrets.access_token = Some(tokens.access_token.clone());
    secrets.refresh_token = tokens.refresh_token.clone();
    secrets.expires_at = tokens.expires_at;
    store.write(provider, &secrets)?;
    edit(storage, provider, |connection| {
        connection.state = ConnectionState::Connected;
        connection.label = label;
        connection.connected_at = now();
        connection.expires_at = tokens.expires_at;
        connection.last_error = None;
        connection.has_secret = true;
    })?;
    Ok(())
}

fn cancel_pending(provider: Option<ProviderId>) {
    let mut guard = PENDING.lock().unwrap_or_else(|error| error.into_inner());
    if guard
        .as_ref()
        .is_some_and(|pending| provider.is_none_or(|provider| pending.provider == provider))
        && let Some(pending) = guard.take()
    {
        pending.cancel.store(true, Ordering::Relaxed);
    }
}

fn disconnect(
    storage: &StorageManager,
    store: &dyn SecretStore,
    provider: ProviderId,
) -> Result<(), String> {
    cancel_pending(Some(provider));
    store.remove(provider)?;
    edit(storage, provider, |connection| {
        *connection = Connection::default();
    })
}

fn revoke(
    storage: &StorageManager,
    store: &dyn SecretStore,
    env: &Env,
    provider: ProviderId,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    if !provider.is_oauth() || env.endpoints(provider).revoke.is_none() {
        return disconnect(storage, store, provider);
    }
    cancel_pending(Some(provider));
    let secrets = store.read(provider)?;
    if let (Some(token), Some(revoke_url)) = (
        if provider == ProviderId::GoogleDrive {
            secrets
                .refresh_token
                .as_deref()
                .or(secrets.access_token.as_deref())
        } else {
            secrets.access_token.as_deref()
        },
        env.endpoints(provider).revoke,
    ) {
        let body = form(&[("token", token)]);
        let headers = if provider == ProviderId::Dropbox {
            token_header(token)
        } else {
            vec![("Content-Type", "application/x-www-form-urlencoded".into())]
        };
        match send(
            "POST",
            &revoke_url,
            &headers,
            if provider == ProviderId::Dropbox {
                None
            } else {
                Some(body.as_bytes())
            },
            MAX_JSON,
        ) {
            Ok(reply) if reply.ok() => {}
            Ok(_) => warnings.push(
                "El proveedor no confirmó el revoke; la credencial local sí se eliminó".into(),
            ),
            Err(_) => warnings.push(
                "Sin conexión para revocar en el proveedor; la credencial local se eliminó".into(),
            ),
        }
    }
    store.remove(provider)?;
    edit(storage, provider, |connection| {
        *connection = Connection {
            state: ConnectionState::Revoked,
            label: String::new(),
            endpoint: String::new(),
            connected_at: 0,
            expires_at: None,
            last_error: None,
            has_secret: false,
        };
    })?;
    Ok(())
}

fn field<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    fields
        .get(key)
        .map(String::as_str)
        .unwrap_or_default()
        .trim()
}

fn validate_secret(value: &str, limit: usize, name: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("Falta {name}"));
    }
    if value.len() > limit || value.chars().any(|character| character.is_control()) {
        return Err(format!("{name} inválido"));
    }
    Ok(())
}

fn connect(
    storage: &StorageManager,
    store: &dyn SecretStore,
    env: &Env,
    provider: ProviderId,
    fields: &BTreeMap<String, String>,
) -> Result<Option<String>, String> {
    match provider {
        ProviderId::Local => Err("La biblioteca local ya está disponible".into()),
        ProviderId::Webdav => {
            let endpoint = field(fields, "endpoint");
            let username = field(fields, "username");
            let password = field(fields, "password");
            validate_secret(password, 1024, "la contraseña")?;
            validate_secret(username, 256, "el usuario")?;
            let url = secure_url(endpoint)?;
            if url.query().is_some() || url.fragment().is_some() {
                return Err("La dirección WebDAV no debe incluir consultas ni fragmentos".into());
            }
            let endpoint = url.to_string();
            store.write(
                provider,
                &Secrets {
                    password: Some(password.to_string()),
                    ..Secrets::default()
                },
            )?;
            {
                let _lock = storage.library_lock();
                let mut document = load(storage)?;
                document.settings.insert(
                    provider,
                    ProviderSettings {
                        endpoint: endpoint.clone(),
                        username: username.to_string(),
                        ..ProviderSettings::default()
                    },
                );
                let connection = document.connections.entry(provider).or_default();
                connection.state = ConnectionState::Connected;
                connection.label = endpoint.clone();
                connection.endpoint = endpoint;
                connection.connected_at = now();
                connection.expires_at = None;
                connection.last_error = None;
                connection.has_secret = true;
                save(storage, &document)?;
            }
            Ok(None)
        }
        ProviderId::Youtube => {
            let api_key = field(fields, "api_key");
            validate_secret(api_key, 512, "la clave de la API")?;
            store.write(
                provider,
                &Secrets {
                    api_key: Some(api_key.to_string()),
                    ..Secrets::default()
                },
            )?;
            edit(storage, provider, |connection| {
                connection.state = ConnectionState::Connected;
                connection.label = "Clave de API configurada".into();
                connection.endpoint = String::new();
                connection.connected_at = now();
                connection.expires_at = None;
                connection.last_error = None;
                connection.has_secret = true;
            })?;
            Ok(None)
        }
        provider if provider.is_oauth() => {
            let client_id = field(fields, "client_id");
            let client_secret = field(fields, "client_secret");
            let mut settings = {
                let _lock = storage.library_lock();
                load(storage)?
                    .settings
                    .get(&provider)
                    .cloned()
                    .unwrap_or_default()
            };
            if !client_id.is_empty() {
                settings.client_id = client_id.to_string();
            }
            validate_settings(&settings)?;
            let mut secrets = store.read(provider)?;
            if !client_secret.is_empty() {
                validate_secret(client_secret, 512, "la clave del cliente")?;
                secrets.client_secret = Some(client_secret.to_string());
            }
            if !client_secret.is_empty() {
                store.write(provider, &secrets)?;
            }
            {
                let _lock = storage.library_lock();
                let mut document = load(storage)?;
                document.settings.insert(provider, settings.clone());
                save(storage, &document)?;
            }
            let url = begin_oauth(provider, &settings, env)?;
            edit(storage, provider, |connection| {
                connection.state = ConnectionState::Pending;
                connection.label = String::new();
                connection.endpoint = String::new();
                connection.expires_at = None;
                connection.last_error = None;
                connection.has_secret = !secrets.is_empty();
            })?;
            Ok(Some(url))
        }
        _ => Err("Proveedor no admitido".into()),
    }
}

fn validate_settings(settings: &ProviderSettings) -> Result<(), String> {
    if settings.client_id.len() > 512
        || settings.username.len() > 256
        || settings.endpoint.len() > 2048
        || [
            settings.client_id.as_str(),
            settings.username.as_str(),
            settings.endpoint.as_str(),
        ]
        .iter()
        .any(|value| value.chars().any(|character| character.is_control()))
    {
        return Err("Configuración inválida".into());
    }
    if !settings.endpoint.is_empty() {
        secure_url(&settings.endpoint)?;
    }
    Ok(())
}

/// Connection, settings and credentials of a provider that can be queried now.
struct Active {
    settings: ProviderSettings,
    secrets: Secrets,
}

fn active(
    storage: &StorageManager,
    store: &dyn SecretStore,
    env: &Env,
    provider: ProviderId,
) -> Result<Active, String> {
    let (connection, settings, mut secrets) = {
        let _lock = storage.library_lock();
        let document = load(storage)?;
        let connection = document
            .connections
            .get(&provider)
            .cloned()
            .unwrap_or_default();
        if !matches!(
            connection.state,
            ConnectionState::Connected | ConnectionState::Expired
        ) {
            return Err("Conecta la fuente primero".into());
        }
        let settings = document
            .settings
            .get(&provider)
            .cloned()
            .unwrap_or_default();
        let secrets = store.read(provider)?;
        (connection, settings, secrets)
    };
    if provider.is_oauth() {
        if connection
            .expires_at
            .is_some_and(|value| value <= now() + 30)
        {
            let endpoints = env.endpoints(provider);
            match refresh_access(&endpoints, &settings, &secrets) {
                Ok(updated) => {
                    secrets.access_token = Some(updated.access_token.clone());
                    secrets.refresh_token = updated.refresh_token.clone();
                    secrets.expires_at = updated.expires_at;
                    let _lock = storage.library_lock();
                    let mut document = load(storage)?;
                    let stored = document.connections.entry(provider).or_default();
                    stored.state = ConnectionState::Connected;
                    stored.expires_at = updated.expires_at;
                    stored.last_error = None;
                    stored.has_secret = true;
                    save(storage, &document)?;
                    store.write(provider, &secrets)?;
                }
                Err(error) => {
                    let _lock = storage.library_lock();
                    let mut document = load(storage)?;
                    let stored = document.connections.entry(provider).or_default();
                    stored.state = ConnectionState::Expired;
                    stored.last_error = Some(error.clone());
                    save(storage, &document)?;
                    return Err(error);
                }
            }
        }
        if secrets.access_token.is_none() {
            return Err("La autorización caducó. Conecta de nuevo".into());
        }
    }
    match provider {
        ProviderId::Webdav if secrets.password.is_none() => {
            Err("Vuelve a conectar la fuente".into())
        }
        ProviderId::Youtube if secrets.api_key.as_deref().unwrap_or_default().is_empty() => {
            Err("Configura la clave de la API".into())
        }
        _ => Ok(Active { settings, secrets }),
    }
}

// --- adapters ----------------------------------------------------------------

fn local_search(storage: &StorageManager, query: &str) -> Vec<Entry> {
    storage
        .search(query)
        .into_iter()
        .map(|hit: TrackHit| Entry {
            provider: ProviderId::Local,
            id: hit.id,
            name: hit.title,
            kind: "audio".into(),
            detail: [hit.artist, hit.album]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
            size: None,
            source_url: None,
        })
        .collect()
}

fn local_download(storage: &StorageManager, id: &str) -> Result<String, String> {
    let track = {
        let _lock = storage.library_lock();
        crate::library::load(storage)?
            .tracks
            .into_iter()
            .find(|track| track.id == id)
            .ok_or("Canción no encontrada")?
    };
    let path = track
        .local_path
        .ok_or("La canción no tiene archivo local")?;
    let metadata = std::fs::metadata(&path).map_err(|_| "El archivo ya no está disponible")?;
    if !metadata.is_file() {
        return Err("El archivo ya no está disponible".into());
    }
    Ok(path)
}

fn drive_search(
    endpoints: &Endpoints,
    active: &Active,
    query: &str,
    folder: Option<&str>,
) -> Result<Vec<Entry>, String> {
    let token = active
        .secrets
        .access_token
        .as_deref()
        .ok_or("La autorización caducó. Conecta de nuevo")?;
    let mut url = reqwest::Url::parse(&format!("{}files", endpoints.api))
        .map_err(|_| "Dirección de Google Drive inválida".to_string())?;
    let escaped = |value: &str| value.replace('\\', "\\\\").replace('\'', "\\'");
    let filter = if let Some(folder) = folder {
        format!("'{}' in parents and trashed = false", escaped(folder))
    } else {
        format!(
            "(name contains '{needle}' or fullText contains '{needle}') and trashed = false",
            needle = escaped(query)
        )
    };
    url.query_pairs_mut()
        .append_pair("q", &filter)
        .append_pair("pageSize", "100")
        .append_pair("spaces", "drive")
        .append_pair("fields", "files(id,name,mimeType,size)");
    let reply = send("GET", url.as_str(), &token_header(token), None, MAX_JSON)?;
    if reply.status == 401 || reply.status == 403 {
        return Err("Autorización rechazada por Google Drive".into());
    }
    if !reply.ok() {
        return Err(format!("Google Drive respondió {}", reply.status));
    }
    let json = reply.json()?;
    let files = json
        .get("files")
        .and_then(|value| value.as_array())
        .ok_or("Respuesta de Google Drive inválida")?;
    let mut entries = Vec::new();
    for file in files.iter().take(MAX_ENTRIES) {
        let id = file
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let name = file
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if id.is_empty() || name.is_empty() {
            continue;
        }
        let mime = file
            .get("mimeType")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let is_folder = mime == "application/vnd.google-apps.folder";
        let kind = entry_kind(name, is_folder, None);
        // Search only offers folders and playable audio; browsing shows all.
        if folder.is_none() && !is_folder && kind != "audio" {
            continue;
        }
        entries.push(Entry {
            provider: ProviderId::GoogleDrive,
            id: id.to_string(),
            name: name.to_string(),
            kind,
            detail: if is_folder {
                "Carpeta".into()
            } else {
                mime.to_string()
            },
            size: file
                .get("size")
                .and_then(|value| value.as_str())
                .and_then(|value| value.parse().ok()),
            source_url: None,
        });
        if entries.len() >= MAX_RESULTS {
            break;
        }
    }
    Ok(entries)
}

fn dropbox_search(
    endpoints: &Endpoints,
    active: &Active,
    query: &str,
) -> Result<Vec<Entry>, String> {
    let token = active
        .secrets
        .access_token
        .as_deref()
        .ok_or("La autorización caducó. Conecta de nuevo")?;
    let body = serde_json::json!({
        "query": query,
        "options": { "max_results": 50, "file_status": "active" }
    });
    let reply = send(
        "POST",
        &format!("{}files/search_v2", endpoints.api),
        &[
            ("Authorization", format!("Bearer {token}")),
            ("Content-Type", "application/json".into()),
        ],
        Some(body.to_string().as_bytes()),
        MAX_JSON,
    )?;
    if reply.status == 401 {
        return Err("Autorización rechazada por Dropbox".into());
    }
    if !reply.ok() {
        return Err(format!("Dropbox respondió {}", reply.status));
    }
    let json = reply.json()?;
    let matches = json
        .get("matches")
        .and_then(|value| value.as_array())
        .ok_or("Respuesta de Dropbox inválida")?;
    let mut entries = Vec::new();
    for item in matches.iter().take(MAX_ENTRIES) {
        let metadata = item
            .get("metadata")
            .and_then(|value| value.get("metadata"))
            .ok_or("Respuesta de Dropbox inválida")?;
        let entry = dropbox_entry(metadata, ProviderId::Dropbox)?;
        if entry.kind == "file" && !is_audio_name(&entry.name) {
            continue;
        }
        entries.push(entry);
        if entries.len() >= MAX_RESULTS {
            break;
        }
    }
    Ok(entries)
}

fn dropbox_entry(metadata: &serde_json::Value, provider: ProviderId) -> Result<Entry, String> {
    let tag = metadata
        .get(".tag")
        .and_then(|value| value.as_str())
        .unwrap_or("file");
    let name = metadata
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or("Respuesta de Dropbox inválida")?;
    let path = metadata
        .get("path_display")
        .or_else(|| metadata.get("path_lower"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let folder = tag == "folder";
    Ok(Entry {
        provider,
        id: path.to_string(),
        name: name.to_string(),
        kind: entry_kind(name, folder, None),
        detail: if folder {
            "Carpeta".into()
        } else {
            "Archivo".into()
        },
        size: metadata.get("size").and_then(|value| value.as_u64()),
        source_url: None,
    })
}

fn dropbox_browse(
    endpoints: &Endpoints,
    active: &Active,
    path: &str,
) -> Result<Vec<Entry>, String> {
    let token = active
        .secrets
        .access_token
        .as_deref()
        .ok_or("La autorización caducó. Conecta de nuevo")?;
    let body = serde_json::json!({ "path": path, "limit": 100, "recursive": false });
    let reply = send(
        "POST",
        &format!("{}files/list_folder", endpoints.api),
        &[
            ("Authorization", format!("Bearer {token}")),
            ("Content-Type", "application/json".into()),
        ],
        Some(body.to_string().as_bytes()),
        MAX_JSON,
    )?;
    if reply.status == 401 {
        return Err("Autorización rechazada por Dropbox".into());
    }
    if reply.status == 409 {
        return Err("La carpeta de Dropbox ya no existe".into());
    }
    if !reply.ok() {
        return Err(format!("Dropbox respondió {}", reply.status));
    }
    let json = reply.json()?;
    let entries = json
        .get("entries")
        .and_then(|value| value.as_array())
        .ok_or("Respuesta de Dropbox inválida")?;
    let mut result = Vec::new();
    for metadata in entries.iter().take(MAX_ENTRIES) {
        result.push(dropbox_entry(metadata, ProviderId::Dropbox)?);
        if result.len() >= MAX_RESULTS {
            break;
        }
    }
    Ok(result)
}

const GRAPH_FIELDS: &str = "id,name,size,folder,audio,file,@microsoft.graph.downloadUrl";

fn onedrive_entries(active: &Active, url: &str) -> Result<Vec<Entry>, String> {
    let token = active
        .secrets
        .access_token
        .as_deref()
        .ok_or("La autorización caducó. Conecta de nuevo")?;
    let reply = send("GET", url, &token_header(token), None, MAX_JSON)?;
    if reply.status == 401 || reply.status == 403 {
        return Err("Autorización rechazada por OneDrive".into());
    }
    if !reply.ok() {
        return Err(format!("OneDrive respondió {}", reply.status));
    }
    let json = reply.json()?;
    let items = json
        .get("value")
        .and_then(|value| value.as_array())
        .ok_or("Respuesta de OneDrive inválida")?;
    let mut result = Vec::new();
    for item in items.iter().take(MAX_ENTRIES) {
        let id = item
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let name = item
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if id.is_empty() || name.is_empty() {
            continue;
        }
        let folder = item.get("folder").is_some();
        result.push(Entry {
            provider: ProviderId::OneDrive,
            id: id.to_string(),
            name: name.to_string(),
            kind: entry_kind(name, folder, None),
            detail: if folder {
                "Carpeta".into()
            } else if item.get("audio").is_some() {
                "Audio".into()
            } else {
                "Archivo".into()
            },
            size: item.get("size").and_then(|value| value.as_u64()),
            source_url: item
                .get("@microsoft.graph.downloadUrl")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        });
        if result.len() >= MAX_RESULTS {
            break;
        }
    }
    Ok(result)
}

fn onedrive_search(
    endpoints: &Endpoints,
    active: &Active,
    query: &str,
) -> Result<Vec<Entry>, String> {
    let url = format!(
        "{api}me/drive/root/search(q='{query}')?$select={fields}",
        api = endpoints.api,
        query = query.replace('\'', "''"),
        fields = GRAPH_FIELDS
    );
    onedrive_entries(active, &url)
}

fn onedrive_browse(endpoints: &Endpoints, active: &Active, id: &str) -> Result<Vec<Entry>, String> {
    let target = if id.is_empty() {
        format!(
            "{}me/drive/root/children?$select={}",
            endpoints.api, GRAPH_FIELDS
        )
    } else {
        format!(
            "{api}me/drive/items/{id}/children?$select={fields}",
            api = endpoints.api,
            id = percent_encode(id),
            fields = GRAPH_FIELDS
        )
    };
    onedrive_entries(active, &target)
}

fn iso_duration(value: &str) -> String {
    let mut seconds = 0u64;
    let mut number = 0u64;
    // The unit that follows a number decides its weight; anything else (`P`,
    // `T`, a stray `S` at the end) contributes nothing.
    for character in value.chars().chain(std::iter::once('S')) {
        if character.is_ascii_digit() {
            number = number * 10 + character.to_digit(10).unwrap_or(0) as u64;
        } else {
            seconds += match character {
                'H' => number * 3600,
                'M' => number * 60,
                'S' => number,
                _ => 0,
            };
            number = 0;
        }
    }
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let rest = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes}:{rest:02}")
    }
}

fn youtube_search(
    endpoints: &Endpoints,
    active: &Active,
    query: &str,
) -> Result<Vec<Entry>, String> {
    let key = active
        .secrets
        .api_key
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or("Configura la clave de la API")?;
    let mut url = reqwest::Url::parse(&format!("{}search", endpoints.api))
        .map_err(|_| "Dirección de YouTube inválida".to_string())?;
    url.query_pairs_mut()
        .append_pair("part", "snippet")
        .append_pair("type", "video")
        .append_pair("maxResults", "25")
        .append_pair("q", query)
        .append_pair("key", key);
    let reply = send("GET", url.as_str(), &[], None, MAX_JSON)?;
    if reply.status == 400 || reply.status == 403 {
        return Err("YouTube rechazó la clave de la API".into());
    }
    if !reply.ok() {
        return Err(format!("YouTube respondió {}", reply.status));
    }
    let json = reply.json()?;
    let items = json
        .get("items")
        .and_then(|value| value.as_array())
        .ok_or("Respuesta de YouTube inválida")?;
    let mut entries = Vec::new();
    let mut ids = Vec::new();
    for item in items.iter().take(25) {
        let id = item
            .get("id")
            .and_then(|value| value.get("videoId"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let snippet = item
            .get("snippet")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let title = snippet
            .get("title")
            .and_then(|value| value.as_str())
            .map(decode_entities)
            .unwrap_or_else(|| "Video".into());
        let channel = snippet
            .get("channelTitle")
            .and_then(|value| value.as_str())
            .map(decode_entities)
            .unwrap_or_default();
        ids.push(id.to_string());
        entries.push(Entry {
            provider: ProviderId::Youtube,
            id: id.to_string(),
            name: title,
            kind: "video".into(),
            detail: channel,
            size: None,
            source_url: Some(format!("https://www.youtube.com/watch?v={id}")),
        });
    }
    if !ids.is_empty() {
        let mut details = reqwest::Url::parse(&format!("{}videos", endpoints.api))
            .map_err(|_| "Dirección de YouTube inválida".to_string())?;
        details
            .query_pairs_mut()
            .append_pair("part", "contentDetails")
            .append_pair("id", &ids.join(","))
            .append_pair("key", key);
        if let Ok(reply) = send("GET", details.as_str(), &[], None, MAX_JSON)
            && let Ok(json) = reply.json()
            && let Some(rows) = json.get("items").and_then(|value| value.as_array())
        {
            for row in rows {
                let Some(id) = row.get("id").and_then(|value| value.as_str()) else {
                    continue;
                };
                let duration = row
                    .get("contentDetails")
                    .and_then(|value| value.get("duration"))
                    .and_then(|value| value.as_str())
                    .map(iso_duration);
                if let (Some(duration), Some(entry)) =
                    (duration, entries.iter_mut().find(|entry| entry.id == id))
                {
                    entry.detail = format!("{} · {duration}", entry.detail);
                }
            }
        }
    }
    Ok(entries)
}

// --- downloads ---------------------------------------------------------------

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "ogg", "m4a", "aac", "aiff", "caf"];

fn digest(provider: ProviderId, id: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{}\0{id}", provider.key()).as_bytes())
    )
}

fn cached(storage: &StorageManager, provider: ProviderId, id: &str) -> Option<PathBuf> {
    let digest = digest(provider, id);
    let directory = storage.document_path("cache/audio");
    AUDIO_EXTENSIONS.iter().find_map(|extension| {
        let candidate = directory.join(format!("{digest}.{extension}"));
        candidate
            .metadata()
            .ok()
            .filter(|metadata| metadata.is_file() && metadata.len() > 0)
            .map(|_| candidate)
    })
}

fn stream_to(
    url: &str,
    headers: &[(&str, String)],
    path: &Path,
    max: u64,
) -> Result<(u16, u64, Option<String>), String> {
    let target = secure_url(url)?;
    let mut request = client(DOWNLOAD_TIMEOUT)?.get(target);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    let response = request
        .send()
        .map_err(|_| "Sin conexión con el proveedor")?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let mut file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    let written = std::io::copy(&mut response.take(max + 1), &mut file)
        .map_err(|_| "Descarga interrumpida".to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    Ok((status, written, content_type))
}

fn download_cached(
    storage: &StorageManager,
    provider: ProviderId,
    id: &str,
    url: &str,
    headers: &[(&str, String)],
) -> Result<String, String> {
    if let Some(hit) = cached(storage, provider, id) {
        return Ok(hit.to_string_lossy().into_owned());
    }
    let digest = digest(provider, id);
    let directory = storage.document_path("cache/audio");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let part = directory.join(format!("{digest}.part"));
    let outcome = stream_to(url, headers, &part, MAX_AUDIO);
    let (status, bytes, content_type) = match outcome {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_file(&part);
            return Err(error);
        }
    };
    let failure = if status == 401 || status == 403 {
        Some(format!("Autorización rechazada ({status})"))
    } else if !(200..300).contains(&status) {
        Some(format!("El proveedor respondió {status}"))
    } else if bytes == 0 {
        Some("Descarga vacía".into())
    } else if bytes > MAX_AUDIO {
        Some("Archivo mayor a 128 MiB".into())
    } else {
        None
    };
    if let Some(message) = failure {
        let _ = std::fs::remove_file(&part);
        return Err(message);
    }
    let Some(extension) = audio_extension(id, content_type.as_deref()) else {
        let _ = std::fs::remove_file(&part);
        return Err("Formato de audio no reconocido".into());
    };
    let target = directory.join(format!("{digest}.{extension}"));
    std::fs::rename(&part, &target).map_err(|error| {
        let _ = std::fs::remove_file(&part);
        error.to_string()
    })?;
    Ok(target.to_string_lossy().into_owned())
}

fn download(
    storage: &StorageManager,
    store: &dyn SecretStore,
    env: &Env,
    provider: ProviderId,
    id: &str,
) -> Result<String, String> {
    if provider == ProviderId::Local {
        return local_download(storage, id);
    }
    if provider == ProviderId::YoutubeAuto {
        return youtube::download(storage, &env.youtube_tools, id);
    }
    if provider == ProviderId::Youtube {
        return Err("YouTube no permite descargas".into());
    }
    let id = &safe_id(id)?;
    let active = active(storage, store, env, provider)?;
    let endpoints = env.endpoints(provider);
    match provider {
        ProviderId::Webdav => {
            let url = webdav_url(&active.settings.endpoint, id)?;
            let auth = webdav_auth(&active.settings, &active.secrets)?;
            let headers = [("Authorization", auth)];
            download_cached(storage, provider, id, url.as_str(), &headers)
        }
        ProviderId::GoogleDrive => {
            let token = active
                .secrets
                .access_token
                .as_deref()
                .ok_or("La autorización caducó. Conecta de nuevo")?;
            let url = format!("{}files/{}?alt=media", endpoints.api, percent_encode(id));
            let headers = token_header(token);
            download_cached(storage, provider, id, &url, &headers)
        }
        ProviderId::Dropbox => {
            let token = active
                .secrets
                .access_token
                .as_deref()
                .ok_or("La autorización caducó. Conecta de nuevo")?;
            let path = serde_json::to_string(id).map_err(|_| "Ruta inválida".to_string())?;
            let base = if endpoints.content.is_empty() {
                &endpoints.api
            } else {
                &endpoints.content
            };
            let headers = [
                ("Authorization", format!("Bearer {token}")),
                ("Dropbox-API-Arg", format!("{{\"path\":{path}}}")),
            ];
            download_cached(
                storage,
                provider,
                id,
                &format!("{base}files/download"),
                &headers,
            )
        }
        ProviderId::OneDrive => {
            let token = active
                .secrets
                .access_token
                .as_deref()
                .ok_or("La autorización caducó. Conecta de nuevo")?;
            let metadata = send(
                "GET",
                &format!(
                    "{api}me/drive/items/{id}?$select=@microsoft.graph.downloadUrl",
                    api = endpoints.api,
                    id = percent_encode(id)
                ),
                &token_header(token),
                None,
                MAX_JSON,
            )?;
            if !metadata.ok() {
                return Err(format!("OneDrive respondió {}", metadata.status));
            }
            let url = metadata
                .json()?
                .get("@microsoft.graph.downloadUrl")
                .and_then(|value| value.as_str())
                .ok_or("OneDrive no ofreció una descarga autorizada")?
                .to_string();
            download_cached(storage, provider, id, &url, &[])
        }
        _ => Err("Proveedor no admitido".into()),
    }
}

// --- execution ---------------------------------------------------------------

fn search(
    storage: &StorageManager,
    store: &dyn SecretStore,
    env: &Env,
    provider: ProviderId,
    query: &str,
) -> Result<Vec<Entry>, String> {
    let query = query.trim();
    if query.is_empty() || query.len() > MAX_QUERY {
        return Err(format!(
            "Escribe una búsqueda de hasta {MAX_QUERY} caracteres"
        ));
    }
    if provider == ProviderId::Local {
        return Ok(local_search(storage, query));
    }
    if provider == ProviderId::YoutubeAuto {
        return youtube::search(&env.youtube_tools, query);
    }
    let active = active(storage, store, env, provider)?;
    let endpoints = env.endpoints(provider);
    match provider {
        ProviderId::Youtube => youtube_search(&endpoints, &active, query),
        ProviderId::GoogleDrive => drive_search(&endpoints, &active, query, None),
        ProviderId::Dropbox => dropbox_search(&endpoints, &active, query),
        ProviderId::OneDrive => onedrive_search(&endpoints, &active, query),
        ProviderId::Webdav => webdav_search(&active.settings, &active.secrets, query),
        _ => Err("Proveedor no admitido".into()),
    }
}

fn browse(
    storage: &StorageManager,
    store: &dyn SecretStore,
    env: &Env,
    provider: ProviderId,
    path: Option<String>,
) -> Result<Vec<Entry>, String> {
    let id = path
        .map(|value| safe_id(&value))
        .transpose()?
        .unwrap_or_default();
    match provider {
        ProviderId::Local => {
            return Err("La biblioteca local se recorre en la pantalla Biblioteca".into());
        }
        ProviderId::Youtube => {
            return Err("YouTube no organiza el contenido en carpetas".into());
        }
        _ => {}
    }
    let active = active(storage, store, env, provider)?;
    let endpoints = env.endpoints(provider);
    match provider {
        ProviderId::Webdav => webdav_entries(&active.settings, &active.secrets, &id),
        ProviderId::GoogleDrive => {
            let folder = if id.is_empty() { "root".into() } else { id };
            drive_search(&endpoints, &active, "", Some(&folder))
        }
        ProviderId::Dropbox => dropbox_browse(&endpoints, &active, &id),
        ProviderId::OneDrive => onedrive_browse(&endpoints, &active, &id),
        _ => Err("Proveedor no admitido".into()),
    }
}

fn snapshot(
    storage: &StorageManager,
    results: Vec<Entry>,
    pending_url: Option<String>,
    downloaded: Option<String>,
    warnings: Vec<String>,
) -> Result<Snapshot, String> {
    let document = load(storage)?;
    let pending = PENDING.lock().unwrap_or_else(|error| error.into_inner());
    let pending = pending.as_ref().filter(|pending| {
        document.external
            && !pending.cancel.load(Ordering::Relaxed)
            && Instant::now() < pending.deadline
    });
    let pending_url = pending_url.or_else(|| pending.map(|pending| pending.url.clone()));
    let providers = registry()
        .into_iter()
        .map(|info| {
            let mut connection = document
                .connections
                .get(&info.id)
                .cloned()
                .unwrap_or_default();
            if info.id == ProviderId::Local {
                connection.state = ConnectionState::Connected;
            } else if info.id == ProviderId::YoutubeAuto {
                connection.state = ConnectionState::Connected;
                connection.label = "Sin clave · proveedor principal".into();
            } else if connection.state == ConnectionState::Pending
                && !pending.is_some_and(|pending| pending.provider == info.id)
            {
                connection.state = ConnectionState::Error;
                connection.last_error = Some(
                    "La autorización terminó o se canceló. Vuelve a conectar la fuente.".into(),
                );
            }
            ProviderStatus {
                state: connection.state,
                label: connection.label,
                endpoint: connection.endpoint,
                expires_at: connection.expires_at,
                last_error: connection.last_error,
                has_secret: connection.has_secret,
                settings: document.settings.get(&info.id).cloned().unwrap_or_default(),
                info,
            }
        })
        .collect();
    Ok(Snapshot {
        providers,
        results,
        external: document.external,
        pending_url,
        downloaded,
        warnings,
    })
}

fn execute_with(
    storage: &StorageManager,
    store: &dyn SecretStore,
    env: &Env,
    action: Action,
) -> Result<Snapshot, String> {
    let mut warnings: Vec<String> = Vec::new();
    let mut results: Vec<Entry> = Vec::new();
    let mut pending_url: Option<String> = None;
    let mut downloaded: Option<String> = None;
    match action {
        Action::Snapshot => {}
        Action::OpenGuide { provider } => {
            let url = match provider {
                ProviderId::YoutubeAuto => "https://github.com/yt-dlp/yt-dlp",
                ProviderId::Youtube | ProviderId::GoogleDrive => {
                    "https://console.cloud.google.com/"
                }
                ProviderId::Dropbox => "https://www.dropbox.com/developers/apps",
                ProviderId::OneDrive => "https://entra.microsoft.com/",
                ProviderId::Webdav => {
                    "https://docs.nextcloud.com/server/latest/user_manual/en/files/access_webdav.html"
                }
                ProviderId::Local => return Err("La biblioteca local no necesita una API".into()),
            };
            if env.open_browser {
                open_browser(url)?;
            }
        }
        Action::CancelYoutube => youtube::cancel(),
        Action::Downloads => {
            let directory = storage.document_path("cache/audio");
            if directory.exists() {
                for item in std::fs::read_dir(&directory)
                    .map_err(|_| "No se pudieron leer las descargas".to_string())?
                    .flatten()
                {
                    let path = item.path();
                    let name = item.file_name().to_string_lossy().to_string();
                    if !item.file_type().is_ok_and(|kind| kind.is_file()) || !is_audio_name(&name) {
                        continue;
                    }
                    results.push(Entry {
                        provider: ProviderId::Local,
                        id: path.to_string_lossy().to_string(),
                        name,
                        kind: "audio".into(),
                        detail: "Descarga guardada".into(),
                        size: item.metadata().ok().map(|metadata| metadata.len()),
                        source_url: None,
                    });
                }
                results.sort_by(|a, b| a.name.cmp(&b.name));
            }
        }
        Action::External { enabled } => {
            let _lock = storage.library_lock();
            let mut document = load(storage)?;
            document.external = enabled;
            save(storage, &document)?;
            if !enabled {
                youtube::cancel();
                cancel_pending(None);
            }
        }
        Action::Settings { provider, settings } => {
            validate_settings(&settings)?;
            let _lock = storage.library_lock();
            let mut document = load(storage)?;
            document.settings.insert(provider, settings);
            save(storage, &document)?;
        }
        Action::Connect { provider, fields } => {
            ensure_external(storage)?;
            pending_url = connect(storage, store, env, provider, &fields)?;
        }
        Action::Finish { provider } => {
            ensure_external(storage)?;
            if let Err(error) = finish_oauth(storage, store, env, provider) {
                edit(storage, provider, |connection| {
                    connection.state = ConnectionState::Error;
                    connection.last_error = Some(error.clone());
                })?;
                return Err(error);
            }
        }
        Action::Disconnect { provider } => disconnect(storage, store, provider)?,
        Action::Revoke { provider } => {
            ensure_external(storage)?;
            revoke(storage, store, env, provider, &mut warnings)?;
        }
        Action::Search { provider, query } => {
            if provider != ProviderId::Local {
                ensure_external(storage)?;
            }
            results = search(storage, store, env, provider, &query)?;
        }
        Action::Browse { provider, path } => {
            ensure_external(storage)?;
            results = browse(storage, store, env, provider, path)?;
        }
        Action::Download { provider, id } => {
            if provider != ProviderId::Local {
                ensure_external(storage)?;
            }
            downloaded = Some(download(storage, store, env, provider, &id)?);
        }
    }
    snapshot(storage, results, pending_url, downloaded, warnings)
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;

#[tauri::command]
pub async fn sources_action(app: tauri::AppHandle, action: Action) -> Result<Snapshot, String> {
    use tauri::Manager;
    let env = Env {
        youtube_tools: app
            .path()
            .resource_dir()
            .map_err(|error| error.to_string())?
            .join("tools"),
        ..Env::default()
    };
    tauri::async_runtime::spawn_blocking(move || {
        execute_with(&app.state::<StorageManager>(), &OsVault, &env, action)
    })
    .await
    .map_err(|error| error.to_string())?
}
