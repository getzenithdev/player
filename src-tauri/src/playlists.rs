//! Playlist edits never mutate a queue or original media. A document mutation is
//! serialized with library scans/imports and committed through StorageManager.
use crate::storage::{StorageManager, indexes::TracksDocument};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const DOCUMENT: &str = "library/playlists.json";
const MAX_ENTRIES: usize = 100_000;

pub fn new_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    #[serde(default)]
    pub available: bool,
    pub track_id: Option<String>,
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub format: String,
}
impl Entry {
    pub fn from_path(path: &Path) -> Self {
        Self {
            id: new_id(),
            available: path.is_file(),
            track_id: None,
            path: path.to_string_lossy().into_owned(),
            title: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            format: path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_uppercase(),
            duration_ms: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub entries: Vec<Entry>,
    pub updated_at: u64,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistDocument {
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}
#[derive(Serialize)]
pub struct PlaylistSnapshot {
    pub playlists: Vec<Playlist>,
    pub notice: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlaylistAction {
    Snapshot,
    Create {
        name: String,
        #[serde(default)]
        entries: Vec<Entry>,
    },
    Rename {
        id: String,
        name: String,
    },
    Duplicate {
        id: String,
    },
    Delete {
        id: String,
    },
    Add {
        id: String,
        entries: Vec<Entry>,
    },
    Remove {
        id: String,
        entry_id: String,
    },
    Move {
        id: String,
        entry_id: String,
        before_id: Option<String>,
    },
    Import {
        path: String,
    },
    Export {
        id: String,
        path: String,
        format: String,
    },
}
pub fn load(storage: &StorageManager) -> Result<PlaylistDocument, String> {
    storage
        .load_document::<PlaylistDocument>(DOCUMENT)
        .map(|d| d.map(|d| d.data).unwrap_or_default())
        .map_err(|e| e.to_string())
}
pub fn library_tracks(storage: &StorageManager) -> Result<TracksDocument, String> {
    storage
        .load_document::<TracksDocument>("library/tracks.json")
        .map(|d| d.map(|d| d.data).unwrap_or_default())
        .map_err(|e| e.to_string())
}
pub fn resolve_entries(entries: &mut [Entry], library: &TracksDocument) {
    for entry in entries {
        if let Some(t) = library
            .tracks
            .iter()
            .find(|t| entry.track_id.as_ref().is_some_and(|id| &t.id == id))
            .or_else(|| {
                library
                    .tracks
                    .iter()
                    .find(|t| t.local_path.as_ref() == Some(&entry.path))
            })
            && let Some(path) = &t.local_path
        {
            entry.path = path.clone();
            entry.track_id = Some(t.id.clone());
            entry.title = t.title.clone();
            entry.duration_ms = t.duration.map(|s| s.saturating_mul(1000));
            entry.format = t.format.clone();
        }
        entry.available = Path::new(&entry.path).is_file();
    }
}
fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        Err("El nombre debe tener entre 1 y 128 caracteres".into())
    } else {
        Ok(name.into())
    }
}
pub fn validate_entries(entries: &[Entry]) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES {
        return Err("La lista supera 100000 entradas".into());
    }
    for e in entries {
        if e.path.is_empty()
            || e.path.len() > 32768
            || e.path.contains(['\r', '\n', '\0'])
            || e.title.len() > 16384
            || e.path.contains("://")
        {
            return Err("Solo se admiten rutas locales válidas".into());
        }
    }
    Ok(())
}
pub fn move_before(entries: &mut Vec<Entry>, id: &str, before: Option<&str>) -> Result<(), String> {
    let from = entries
        .iter()
        .position(|e| e.id == id)
        .ok_or("La entrada ya no existe")?;
    if before == Some(id) {
        return Ok(());
    }
    if before.is_some_and(|id| !entries.iter().any(|e| e.id == id)) {
        return Err("La posición de destino ya no existe".into());
    }
    let entry = entries.remove(from);
    let to = before
        .and_then(|id| entries.iter().position(|e| e.id == id))
        .unwrap_or(entries.len());
    entries.insert(to, entry);
    Ok(())
}
pub fn execute(
    storage: &StorageManager,
    action: PlaylistAction,
) -> Result<PlaylistSnapshot, String> {
    let _operation = storage.library_lock();
    let mut doc = load(storage)?;
    let before = doc.clone();
    let mut notice = None;
    let library = library_tracks(storage)?;
    match action {
        PlaylistAction::Snapshot => {}
        PlaylistAction::Create { name, mut entries } => {
            validate_entries(&entries)?;
            resolve_entries(&mut entries, &library);
            for e in &mut entries {
                e.id = new_id();
            }
            doc.playlists.push(Playlist {
                id: new_id(),
                name: validate_name(&name)?,
                entries,
                updated_at: now(),
            });
        }
        PlaylistAction::Rename { id, name } => {
            let p = find(&mut doc, &id)?;
            p.name = validate_name(&name)?;
            p.updated_at = now();
        }
        PlaylistAction::Duplicate { id } => {
            let mut p = find(&mut doc, &id)?.clone();
            p.id = new_id();
            p.name = validate_name(&format!(
                "{} (copia)",
                p.name.chars().take(119).collect::<String>()
            ))?;
            for e in &mut p.entries {
                e.id = new_id();
            }
            p.updated_at = now();
            doc.playlists.push(p);
        }
        PlaylistAction::Delete { id } => {
            find(&mut doc, &id)?;
            doc.playlists.retain(|p| p.id != id);
        }
        PlaylistAction::Add { id, mut entries } => {
            validate_entries(&entries)?;
            resolve_entries(&mut entries, &library);
            let p = find(&mut doc, &id)?;
            if p.entries.len().saturating_add(entries.len()) > MAX_ENTRIES {
                return Err("La lista supera 100000 entradas".into());
            }
            for e in &mut entries {
                e.id = new_id();
            }
            p.entries.extend(entries);
            p.updated_at = now();
        }
        PlaylistAction::Remove { id, entry_id } => {
            let p = find(&mut doc, &id)?;
            if !p.entries.iter().any(|e| e.id == entry_id) {
                return Err("La entrada ya no existe".into());
            }
            p.entries.retain(|e| e.id != entry_id);
            p.updated_at = now();
        }
        PlaylistAction::Move {
            id,
            entry_id,
            before_id,
        } => {
            let p = find(&mut doc, &id)?;
            move_before(&mut p.entries, &entry_id, before_id.as_deref())?;
            p.updated_at = now();
        }
        PlaylistAction::Import { path } => {
            let mut p = import_file(Path::new(&path))?;
            resolve_entries(&mut p.entries, &library);
            doc.playlists.push(p);
        }
        PlaylistAction::Export { id, path, format } => {
            let mut p = find(&mut doc, &id)?.clone();
            resolve_entries(&mut p.entries, &library);
            export_file(&p, Path::new(&path), &format)?;
            notice = Some(format!("Lista exportada a {path}"));
        }
    }
    if doc != before {
        storage
            .save_document(DOCUMENT, &doc)
            .map_err(|e| e.to_string())?;
    }
    for p in &mut doc.playlists {
        resolve_entries(&mut p.entries, &library);
    }
    Ok(PlaylistSnapshot {
        playlists: doc.playlists,
        notice,
    })
}
fn find<'a>(doc: &'a mut PlaylistDocument, id: &str) -> Result<&'a mut Playlist, String> {
    doc.playlists
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "La lista ya no existe".into())
}
#[derive(Serialize, Deserialize)]
struct Exchange {
    format: String,
    version: u32,
    name: String,
    entries: Vec<Entry>,
}
fn normalize_path(base: &Path, value: &str) -> String {
    // Preserve foreign absolute paths (e.g. a Windows playlist on macOS).
    if value.as_bytes().get(1) == Some(&b':') || value.starts_with("\\\\") {
        return value.into();
    }
    let path = PathBuf::from(value);
    let joined = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized.to_string_lossy().into_owned()
}
pub fn import_file(path: &Path) -> Result<Playlist, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 10 * 1024 * 1024 {
        return Err("La lista supera 10 MiB".into());
    }
    let text = fs::read_to_string(path).map_err(|e| format!("La lista debe usar UTF-8: {e}"))?;
    let base = path.parent().unwrap_or(Path::new("."));
    let (name, mut entries) = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        let exchange: Exchange = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if exchange.format != "zenith-playlist" || exchange.version != 1 {
            return Err("Formato de lista JSON no compatible".into());
        }
        (validate_name(&exchange.name)?, exchange.entries)
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("m3u") || e.eq_ignore_ascii_case("m3u8"))
    {
        let mut entries = Vec::new();
        let mut info = None;
        for line in text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(value) = line.strip_prefix("#EXTINF:") {
                let (seconds, title) = value.split_once(',').unwrap_or((value, ""));
                info = Some((
                    seconds.parse::<u64>().ok().map(|n| n.saturating_mul(1000)),
                    title.to_string(),
                ));
                continue;
            }
            if line.starts_with('#') {
                continue;
            }
            if line.contains("://") {
                return Err("Solo se admiten rutas locales, no URLs".into());
            }
            let resolved = normalize_path(base, line);
            let mut entry = Entry::from_path(Path::new(&resolved));
            if let Some((duration, title)) = info.take() {
                entry.duration_ms = duration;
                if !title.is_empty() {
                    entry.title = title;
                }
            }
            entries.push(entry);
            if entries.len() > MAX_ENTRIES {
                return Err("La lista supera 100000 entradas".into());
            }
        }
        (
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .take(128)
                .collect(),
            entries,
        )
    } else {
        return Err("Selecciona M3U, M3U8 o JSON".into());
    };
    validate_entries(&entries)?;
    for e in &mut entries {
        e.id = new_id();
        e.track_id = None;
        e.path = normalize_path(base, &e.path);
    }
    Ok(Playlist {
        id: new_id(),
        name: validate_name(&name)?,
        entries,
        updated_at: now(),
    })
}
pub fn export_file(playlist: &Playlist, path: &Path, format: &str) -> Result<(), String> {
    validate_entries(&playlist.entries)?;
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if extension != format || !matches!(format, "json" | "m3u" | "m3u8") {
        return Err("La extensión debe coincidir con el formato de exportación".into());
    }
    let text = if format == "json" {
        serde_json::to_string_pretty(&Exchange {
            format: "zenith-playlist".into(),
            version: 1,
            name: playlist.name.clone(),
            entries: playlist.entries.clone(),
        })
        .map_err(|e| e.to_string())?
    } else {
        let base = path.parent().unwrap_or(Path::new("."));
        let mut text = String::from("#EXTM3U\n");
        for e in &playlist.entries {
            let media = Path::new(&e.path);
            let relative = media
                .strip_prefix(base)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .filter(|p| !p.starts_with('#'));
            let value = relative.as_deref().unwrap_or(&e.path);
            text.push_str(&format!(
                "#EXTINF:{},{}\n{}\n",
                e.duration_ms
                    .map(|n| (n / 1000).to_string())
                    .unwrap_or("-1".into()),
                e.title.replace(['\r', '\n'], " "),
                value
            ));
        }
        text
    };
    let tmp = path.with_extension(format!("{}-{}.tmp", format, new_id()));
    let result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
#[cfg(test)]
#[path = "playlists_tests.rs"]
mod tests;
