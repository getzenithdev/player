//! Fully local storage for the player (roadmap phase 2).
//!
//! [`StorageManager`] owns one root directory resolved from the operating
//! system conventions, creates the logical structure described in the
//! roadmap, reads and writes versioned JSON documents through a temporary
//! file plus replacement, keeps backups of the last valid copy and recovers
//! it when a document is damaged.
//!
//! Locking order is always `operation` → `write` → `state`; no path acquires
//! them in a different order, so concurrent commands cannot deadlock.

#[cfg(test)]
mod tests;

pub mod error;
pub mod indexes;
pub mod schema;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use error::{StorageError, StorageResult};
pub use indexes::TrackHit;
use indexes::{INDEX_FIELDS, LibraryIndexes, TracksDocument};
use schema::{AppearanceData, SCHEMA_VERSION, StoredDocument};

/// Directory name used for the default (non-portable) root.
pub const APP_DIR_NAME: &str = "Zenith Player";
/// Presence of this file next to the executable enables portable mode.
pub const PORTABLE_MARKER: &str = "zenith-portable";
/// Backups retained per document; older copies are pruned.
pub const MAX_BACKUPS_PER_DOCUMENT: usize = 5;
/// Format name of an exported bundle.
pub const EXPORT_FORMAT: &str = "zenith-player-export";
/// Format version of an exported bundle.
pub const EXPORT_FORMAT_VERSION: u32 = 1;

const PROTECTED_DOCUMENT: &str = "config/providers.json";
const APPEARANCE_DOCUMENT: &str = "config/appearance.json";
const TRACKS_DOCUMENT: &str = "library/tracks.json";
const BACKUP_DIR: &str = "backups";
const LOG_DIR: &str = "logs";
const LYRICS_DIR: &str = "lyrics";
const EXPORT_SUFFIX: &str = ".json";
const BACKUP_SUFFIX: &str = ".bak";
const MAX_NOTICES: usize = 20;
const LYRICS_MAX_FILES: usize = 10_000;
const LYRICS_MAX_BYTES: u64 = 1_048_576;

/// Directories created under the root, in roadmap order.
pub const SUBDIRS: &[&str] = &[
    "config",
    "library",
    "lyrics",
    "artwork/animated",
    "artwork/apple-music",
    "artwork/archive-org",
    "artwork/youtube",
    "artwork/local",
    "cache/audio",
    "cache/images",
    "cache/metadata",
    "cache/temporary",
    BACKUP_DIR,
    LOG_DIR,
];

/// Configuration documents (roadmap 2.2).
pub const CONFIG_FILES: &[&str] = &[
    "config/settings.json",
    APPEARANCE_DOCUMENT,
    "config/audio.json",
    PROTECTED_DOCUMENT,
    "config/shortcuts.json",
    "config/privacy.json",
];

/// Library documents (roadmap 2.2).
pub const LIBRARY_FILES: &[&str] = &[
    TRACKS_DOCUMENT,
    "library/albums.json",
    "library/lyrics.json",
    "library/artists.json",
    "library/playlists.json",
    "library/favorites.json",
    "library/history.json",
    "library/collections.json",
];

/// Cache directories emptied by [`StorageManager::clear_cache`].
pub const CACHE_DIRS: &[&str] = &[
    "cache/audio",
    "cache/images",
    "cache/metadata",
    "cache/temporary",
];

/// Every document the manager knows how to read, write and export.
pub fn known_documents() -> impl Iterator<Item = &'static str> {
    CONFIG_FILES.iter().chain(LIBRARY_FILES).copied()
}

/// A recoverable event recorded while opening or repairing storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveryNotice {
    pub file: String,
    /// `recovered`, `preserved` or `index`.
    pub kind: String,
    pub message: String,
}

/// Inspection result of one stored document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentStatus {
    pub path: String,
    pub exists: bool,
    pub version: Option<u32>,
    pub valid: bool,
    pub backups: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexStatus {
    pub fields: Vec<String>,
    pub tracks: usize,
}

/// Snapshot of the storage layer, shown in the settings screen.
#[derive(Debug, Clone, Serialize)]
pub struct StorageStatus {
    pub root: String,
    pub portable: bool,
    pub writable: bool,
    pub error: Option<String>,
    pub schema_version: u32,
    pub directories_ready: bool,
    pub documents: Vec<DocumentStatus>,
    pub recovery: Vec<RecoveryNotice>,
    pub index: IndexStatus,
    pub cache_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    pub created: Vec<String>,
    pub skipped: usize,
    pub directory: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub path: String,
    pub config: usize,
    pub library: usize,
    pub lyrics: usize,
    pub excluded: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub path: String,
    pub config: usize,
    pub library: usize,
    pub lyrics: usize,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheReport {
    pub removed_files: usize,
    pub freed_bytes: u64,
    pub errors: Vec<String>,
}

/// Exported bundle. Secrets are never part of it: `config/providers.json` is
/// listed under `excluded` and skipped while reading and writing.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportBundle {
    format: String,
    format_version: u32,
    schema_version: u32,
    exported_at: u64,
    includes_secrets: bool,
    excluded: Vec<String>,
    config: BTreeMap<String, Value>,
    library: BTreeMap<String, Value>,
    lyrics: Vec<LyricsEntry>,
}

/// Read result of one document during an export.
enum RawDocument {
    Missing,
    Valid(Value),
    Damaged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LyricsEntry {
    file: String,
    content: String,
}

#[derive(Debug, Default)]
struct ManagerState {
    indexes: LibraryIndexes,
    recovery: Vec<RecoveryNotice>,
}

/// Local persistence for configuration, library and resources.
#[derive(Debug)]
pub struct StorageManager {
    root: PathBuf,
    portable: bool,
    writable: bool,
    init_error: Option<String>,
    operation_lock: Mutex<()>,
    write_lock: Mutex<()>,
    state: Mutex<ManagerState>,
}

impl StorageManager {
    /// Prepares the real storage location for the current process.
    ///
    /// Never fails: when the location cannot be prepared the manager stays
    /// available in a read-only degraded mode, so a storage problem never
    /// prevents the application from starting (project principle).
    pub fn initialize() -> Self {
        let (root, portable) = resolve_location();
        match Self::open(root.clone(), portable) {
            Ok(manager) => manager,
            Err(error) => Self::unavailable(root, portable, error),
        }
    }

    /// Opens an explicit root, creating the logical layout and rebuilding the
    /// in-memory indexes from `library/tracks.json`.
    pub fn open(root: PathBuf, portable: bool) -> StorageResult<Self> {
        ensure_layout(&root)?;
        let manager = Self {
            root,
            portable,
            writable: true,
            init_error: None,
            operation_lock: Mutex::new(()),
            write_lock: Mutex::new(()),
            state: Mutex::new(ManagerState::default()),
        };
        manager.rebuild_indexes();
        Ok(manager)
    }

    fn unavailable(root: PathBuf, portable: bool, error: StorageError) -> Self {
        Self {
            root,
            portable,
            writable: false,
            init_error: Some(error.to_string()),
            operation_lock: Mutex::new(()),
            write_lock: Mutex::new(()),
            state: Mutex::new(ManagerState::default()),
        }
    }

    pub(crate) fn library_lock(&self) -> MutexGuard<'_, ()> {
        lock(&self.operation_lock)
    }

    /// Absolute path of a document inside the root.
    pub fn document_path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Reads a stored document.
    ///
    /// Returns `Ok(None)` when the file does not exist yet. A damaged file is
    /// preserved under `logs/` and restored from the newest usable backup; if
    /// there is no backup the original error is returned and the damaged copy
    /// stays available for inspection.
    pub fn load_document<T: serde::de::DeserializeOwned>(
        &self,
        rel: &str,
    ) -> StorageResult<Option<StoredDocument<T>>> {
        ensure_known_document(rel)?;
        let _write = lock(&self.write_lock);
        let path = self.document_path(rel);
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(StorageError::Io {
                    path,
                    message: error.to_string(),
                });
            }
        };
        let decoded = match schema::decode_document(&path, &raw) {
            Ok(decoded) => decoded,
            Err(StorageError::InvalidJson { message, .. }) => {
                return self.recover_document(rel, &path, &message);
            }
            Err(other) => return Err(other),
        };
        let data: T = schema::payload(&decoded, &path)?;
        Ok(Some(StoredDocument {
            version: SCHEMA_VERSION,
            updated_at: decoded.updated_at,
            data,
        }))
    }

    /// Writes a document atomically: temporary file first, replacement after.
    ///
    /// Before overwriting, the current valid copy becomes a backup; a damaged
    /// copy is moved to `logs/` first so nothing is lost without a trace. A
    /// file written by a newer format version is never overwritten.
    pub fn save_document<T: Serialize>(&self, rel: &str, data: &T) -> StorageResult<()> {
        ensure_known_document(rel)?;
        self.ensure_writable()?;
        let _write = lock(&self.write_lock);
        let path = self.document_path(rel);
        if path.exists() {
            let raw = fs::read_to_string(&path).map_err(|error| StorageError::Io {
                path: path.clone(),
                message: error.to_string(),
            })?;
            match schema::decode_document(&path, &raw) {
                Ok(_) => {
                    self.backup_document(rel, &path)?;
                }
                Err(StorageError::InvalidJson { .. }) => {
                    self.preserve_corrupt(&path)?;
                }
                Err(other) => return Err(other),
            }
        }
        let bytes = schema::document_bytes(data, now_unix())?;
        write_atomic(&path, &bytes)
    }

    /// Stores the current appearance preferences.
    pub fn save_appearance(&self, theme: &str) -> StorageResult<AppearanceData> {
        if !schema::is_valid_theme(theme) {
            return Err(StorageError::InvalidValue {
                field: "theme".to_string(),
                message: format!("use light, dark o system, no {theme}"),
            });
        }
        let mut appearance = self.load_appearance()?;
        appearance.theme = theme.to_string();
        self.save_document(APPEARANCE_DOCUMENT, &appearance)?;
        Ok(appearance)
    }

    /// Reads the appearance preferences, falling back to the defaults.
    pub fn load_appearance(&self) -> StorageResult<AppearanceData> {
        Ok(self
            .load_document::<AppearanceData>(APPEARANCE_DOCUMENT)?
            .map(|document| document.data)
            .unwrap_or_default())
    }

    /// Copies every existing document into `backups/` (roadmap 2.1).
    pub fn create_backup(&self) -> StorageResult<BackupReport> {
        self.ensure_writable()?;
        let _operation = lock(&self.operation_lock);
        let mut created = Vec::new();
        let mut skipped = 0;
        for rel in known_documents() {
            let _write = lock(&self.write_lock);
            let path = self.document_path(rel);
            if path.exists() {
                self.backup_document(rel, &path)?;
                created.push(rel.to_string());
            } else {
                skipped += 1;
            }
        }
        Ok(BackupReport {
            created,
            skipped,
            directory: self.root.join(BACKUP_DIR).display().to_string(),
        })
    }

    /// Exports configuration, library and lyrics into one bundle.
    ///
    /// Provider credentials are excluded from ordinary exports (roadmap 2.3).
    pub fn export_bundle(&self) -> StorageResult<ExportReport> {
        self.ensure_writable()?;
        let _operation = lock(&self.operation_lock);
        let _write = lock(&self.write_lock);
        let mut bundle = ExportBundle {
            format: EXPORT_FORMAT.to_string(),
            format_version: EXPORT_FORMAT_VERSION,
            schema_version: SCHEMA_VERSION,
            exported_at: now_unix(),
            includes_secrets: false,
            excluded: vec![PROTECTED_DOCUMENT.to_string()],
            config: BTreeMap::new(),
            library: BTreeMap::new(),
            lyrics: Vec::new(),
        };
        let mut skipped = Vec::new();
        let mut config = 0;
        let mut library = 0;
        for rel in CONFIG_FILES {
            if *rel == PROTECTED_DOCUMENT {
                continue;
            }
            match self.read_document_value(rel)? {
                RawDocument::Missing => {}
                RawDocument::Valid(value) => {
                    bundle.config.insert(rel.to_string(), value);
                    config += 1;
                }
                RawDocument::Damaged => skipped.push(format!("{rel} (dañado)")),
            }
        }
        for rel in LIBRARY_FILES {
            match self.read_document_value(rel)? {
                RawDocument::Missing => {}
                RawDocument::Valid(value) => {
                    bundle.library.insert(rel.to_string(), value);
                    library += 1;
                }
                RawDocument::Damaged => skipped.push(format!("{rel} (dañado)")),
            }
        }
        let lyrics = self.read_lyrics(&mut skipped)?;
        bundle.lyrics = lyrics;
        let bytes = serde_json::to_vec_pretty(&bundle).map_err(|error| StorageError::Import {
            message: error.to_string(),
        })?;
        let path = unique_path(
            &self.root.join(BACKUP_DIR),
            "export@",
            EXPORT_SUFFIX,
            now_unix(),
        );
        write_atomic(&path, &bytes)?;
        Ok(ExportReport {
            path: path.display().to_string(),
            config,
            library,
            lyrics: bundle.lyrics.len(),
            excluded: bundle.excluded,
            skipped,
        })
    }

    /// Restores a bundle produced by [`StorageManager::export_bundle`].
    ///
    /// Entries outside the known document list are skipped instead of being
    /// written, and `config/providers.json` is never touched. When `source` is
    /// empty the newest export in `backups/` is used.
    pub fn import_bundle(&self, source: Option<&Path>) -> StorageResult<ImportReport> {
        self.ensure_writable()?;
        let _operation = lock(&self.operation_lock);
        let path = match source {
            Some(path) => path.to_path_buf(),
            None => self.newest_export()?,
        };
        let raw = fs::read_to_string(&path).map_err(|error| StorageError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        let bundle: ExportBundle =
            serde_json::from_str(&raw).map_err(|error| StorageError::Import {
                message: format!(
                    "{} no es un archivo de Zenith Player: {error}",
                    path.display()
                ),
            })?;
        if bundle.format != EXPORT_FORMAT {
            return Err(StorageError::Import {
                message: format!("formato desconocido: {}", bundle.format),
            });
        }
        if bundle.format_version > EXPORT_FORMAT_VERSION {
            return Err(StorageError::Import {
                message: format!(
                    "versión {} de exportación no soportada por este build",
                    bundle.format_version
                ),
            });
        }
        let mut report = ImportReport {
            path: path.display().to_string(),
            config: 0,
            library: 0,
            lyrics: 0,
            skipped: Vec::new(),
        };
        for (rel, value) in bundle.config.iter().chain(bundle.library.iter()) {
            if rel == PROTECTED_DOCUMENT {
                report
                    .skipped
                    .push(format!("{rel} (credenciales excluidas)"));
                continue;
            }
            if !is_known_document(rel) {
                report.skipped.push(format!("{rel} (ruta desconocida)"));
                continue;
            }
            let decoded = match schema::decode_envelope_value(Path::new(rel), value.clone()) {
                Ok(decoded) => decoded,
                Err(error) => {
                    report.skipped.push(format!("{rel} ({error})"));
                    continue;
                }
            };
            match self.save_document(rel, &decoded.data) {
                Ok(()) => {
                    if rel.starts_with("config/") {
                        report.config += 1;
                    } else {
                        report.library += 1;
                    }
                }
                Err(error) => report.skipped.push(format!("{rel} ({error})")),
            }
        }
        report.lyrics = self.import_lyrics(bundle.lyrics, &mut report.skipped)?;
        self.rebuild_indexes();
        Ok(report)
    }

    /// Removes every cached resource, leaving documents and original music
    /// untouched (roadmap 2.1).
    pub fn clear_cache(&self) -> StorageResult<CacheReport> {
        self.ensure_writable()?;
        let _operation = lock(&self.operation_lock);
        let _write = lock(&self.write_lock);
        let mut report = CacheReport {
            removed_files: 0,
            freed_bytes: 0,
            errors: Vec::new(),
        };
        for directory in CACHE_DIRS {
            let path = self.root.join(directory);
            let entries = match fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    report.errors.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            for entry in entries.flatten() {
                let target = entry.path();
                let (files, bytes) = measure(&target);
                let removed = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    fs::remove_dir_all(&target)
                } else {
                    fs::remove_file(&target)
                };
                match removed {
                    Ok(()) => {
                        report.removed_files += files;
                        report.freed_bytes += bytes;
                    }
                    Err(error) => report.errors.push(format!("{}: {error}", target.display())),
                }
            }
        }
        Ok(report)
    }

    /// Snapshot used by the settings screen.
    pub fn status(&self) -> StorageStatus {
        let _write = lock(&self.write_lock);
        let documents = known_documents()
            .map(|rel| self.inspect_document(rel))
            .collect();
        let cache_bytes = self.measure_cache();
        let directories_ready =
            self.writable && SUBDIRS.iter().all(|rel| self.root.join(rel).is_dir());
        let state = lock(&self.state);
        StorageStatus {
            root: self.root.display().to_string(),
            portable: self.portable,
            writable: self.writable,
            error: self.init_error.clone(),
            schema_version: SCHEMA_VERSION,
            directories_ready,
            documents,
            recovery: state.recovery.clone(),
            index: IndexStatus {
                fields: INDEX_FIELDS.iter().map(|field| field.to_string()).collect(),
                tracks: state.indexes.track_count(),
            },
            cache_bytes,
        }
    }

    /// Case-insensitive search over the in-memory indexes.
    pub fn search(&self, query: &str) -> Vec<TrackHit> {
        lock(&self.state).indexes.search(query)
    }

    /// Rebuilds the in-memory indexes from `library/tracks.json`.
    ///
    /// Runs at startup and after an import; a damaged track file is reported
    /// as a notice instead of failing the application.
    pub fn rebuild_indexes(&self) {
        let tracks = match self.load_document::<TracksDocument>(TRACKS_DOCUMENT) {
            Ok(Some(document)) => document.data.tracks,
            Ok(None) => Vec::new(),
            Err(error) => {
                self.record_notice(
                    TRACKS_DOCUMENT,
                    "index",
                    &format!("No se pudieron cargar las pistas para indexar: {error}"),
                );
                Vec::new()
            }
        };
        let indexes = LibraryIndexes::rebuild(tracks);
        lock(&self.state).indexes = indexes;
    }

    fn ensure_writable(&self) -> StorageResult<()> {
        if self.writable {
            Ok(())
        } else {
            Err(StorageError::NotWritable {
                path: self.root.clone(),
            })
        }
    }

    /// Reads a document as raw JSON for export purposes.
    fn read_document_value(&self, rel: &str) -> StorageResult<RawDocument> {
        let path = self.document_path(rel);
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RawDocument::Missing);
            }
            Err(error) => {
                return Err(StorageError::Io {
                    path,
                    message: error.to_string(),
                });
            }
        };
        match serde_json::from_str::<StoredDocument<Value>>(&raw) {
            Ok(envelope) => match serde_json::to_value(envelope) {
                Ok(value) => Ok(RawDocument::Valid(value)),
                Err(_) => Ok(RawDocument::Damaged),
            },
            Err(_) => Ok(RawDocument::Damaged),
        }
    }

    fn read_lyrics(&self, skipped: &mut Vec<String>) -> StorageResult<Vec<LyricsEntry>> {
        let directory = self.root.join(LYRICS_DIR);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(StorageError::Io {
                    path: directory,
                    message: error.to_string(),
                });
            }
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        let mut lyrics = Vec::new();
        for path in files {
            if lyrics.len() >= LYRICS_MAX_FILES {
                skipped.push("lyrics (límite de archivos alcanzado)".to_string());
                break;
            }
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            if size > LYRICS_MAX_BYTES {
                skipped.push(format!("lyrics/{name} (demasiado grande)"));
                continue;
            }
            let bytes = fs::read(&path).map_err(|error| StorageError::Io {
                path: path.clone(),
                message: error.to_string(),
            })?;
            lyrics.push(LyricsEntry {
                file: name,
                content: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        Ok(lyrics)
    }

    fn import_lyrics(
        &self,
        lyrics: Vec<LyricsEntry>,
        skipped: &mut Vec<String>,
    ) -> StorageResult<usize> {
        if lyrics.is_empty() {
            return Ok(0);
        }
        let _write = lock(&self.write_lock);
        let directory = self.root.join(LYRICS_DIR);
        fs::create_dir_all(&directory).map_err(|error| StorageError::Io {
            path: directory.clone(),
            message: error.to_string(),
        })?;
        let mut written = 0;
        for entry in lyrics {
            if !safe_file_name(&entry.file) {
                skipped.push(format!("lyrics/{} (ruta no válida)", entry.file));
                continue;
            }
            let target = directory.join(&entry.file);
            if target.exists() {
                let current = fs::read(&target).unwrap_or_default();
                if current == entry.content.as_bytes() {
                    skipped.push(format!("lyrics/{} (sin cambios)", entry.file));
                    continue;
                }
                self.backup_lyric(&entry.file, &target)?;
            }
            write_atomic(&target, entry.content.as_bytes())?;
            written += 1;
        }
        Ok(written)
    }

    fn backup_lyric(&self, name: &str, source: &Path) -> StorageResult<()> {
        let directory = self.root.join(BACKUP_DIR).join(LYRICS_DIR);
        fs::create_dir_all(&directory).map_err(|error| StorageError::Io {
            path: directory.clone(),
            message: error.to_string(),
        })?;
        let stem = Path::new(name)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| name.to_string());
        let destination = unique_path(&directory, &format!("{stem}@"), BACKUP_SUFFIX, now_unix());
        fs::copy(source, &destination)
            .map_err(|error| StorageError::Io {
                path: destination,
                message: error.to_string(),
            })
            .map(|_| ())
    }

    fn newest_export(&self) -> StorageResult<PathBuf> {
        let directory = self.root.join(BACKUP_DIR);
        let entries = fs::read_dir(&directory).map_err(|error| StorageError::Io {
            path: directory.clone(),
            message: error.to_string(),
        })?;
        let mut newest: Option<(u64, PathBuf)> = None;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(timestamp) = parse_timestamp(&name, "export@", EXPORT_SUFFIX) {
                let candidate = (timestamp, entry.path());
                if newest
                    .as_ref()
                    .is_none_or(|current| candidate.0 > current.0)
                {
                    newest = Some(candidate);
                }
            }
        }
        newest
            .map(|(_, path)| path)
            .ok_or_else(|| StorageError::Import {
                message: "no hay ninguna exportación en el respaldo".to_string(),
            })
    }

    /// Copies the current valid content of a document into `backups/`.
    ///
    /// Requires the write lock to be held by the caller.
    fn backup_document(&self, rel: &str, source: &Path) -> StorageResult<PathBuf> {
        let directory = self.root.join(BACKUP_DIR);
        fs::create_dir_all(&directory).map_err(|error| StorageError::Io {
            path: directory.clone(),
            message: error.to_string(),
        })?;
        let stem = document_stem(rel);
        let destination = unique_path(&directory, &format!("{stem}@"), BACKUP_SUFFIX, now_unix());
        fs::copy(source, &destination).map_err(|error| StorageError::Io {
            path: destination.clone(),
            message: error.to_string(),
        })?;
        self.prune_backups(&directory, &stem, &destination)?;
        Ok(destination)
    }

    fn prune_backups(&self, directory: &Path, stem: &str, protect: &Path) -> StorageResult<()> {
        let prefix = format!("{stem}@");
        let mut candidates: Vec<(u64, PathBuf)> = Vec::new();
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&prefix)
                    && name.ends_with(BACKUP_SUFFIX)
                    && let Some(timestamp) = parse_timestamp(&name, &prefix, BACKUP_SUFFIX)
                {
                    candidates.push((timestamp, entry.path()));
                }
            }
        }
        candidates.sort_by(|left, right| {
            let left_protected = left.1 == protect;
            let right_protected = right.1 == protect;
            right_protected
                .cmp(&left_protected)
                .then_with(|| right.0.cmp(&left.0))
                .then_with(|| right.1.cmp(&left.1))
        });
        for (index, (_, path)) in candidates.into_iter().enumerate() {
            if index < MAX_BACKUPS_PER_DOCUMENT {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(StorageError::Io {
                        path,
                        message: error.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Newest usable backup of a document, newest first.
    fn backup_candidates(&self, rel: &str) -> Vec<(PathBuf, String)> {
        let directory = self.root.join(BACKUP_DIR);
        let prefix = format!("{}@", document_stem(rel));
        let mut candidates: Vec<(u64, PathBuf)> = Vec::new();
        if let Ok(entries) = fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&prefix)
                    && name.ends_with(BACKUP_SUFFIX)
                    && let Some(timestamp) = parse_timestamp(&name, &prefix, BACKUP_SUFFIX)
                {
                    candidates.push((timestamp, entry.path()));
                }
            }
        }
        candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
        candidates
            .into_iter()
            .filter_map(|(_, path)| fs::read_to_string(&path).ok().map(|raw| (path, raw)))
            .collect()
    }

    /// Moves a damaged file to `logs/` so it can be inspected later.
    ///
    /// Requires the write lock to be held by the caller.
    fn preserve_corrupt(&self, path: &Path) -> StorageResult<PathBuf> {
        let directory = self.root.join(LOG_DIR);
        fs::create_dir_all(&directory).map_err(|error| StorageError::Io {
            path: directory.clone(),
            message: error.to_string(),
        })?;
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "documento".to_string());
        let extension = path
            .extension()
            .map(|extension| format!(".{}", extension.to_string_lossy()))
            .unwrap_or_default();
        let destination = unique_path(
            &directory,
            &format!("{stem}.corrupt-{}", now_unix()),
            &extension,
            0,
        );
        fs::rename(path, &destination).map_err(|error| StorageError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Ok(destination)
    }

    /// Restores a damaged document from its newest usable backup.
    ///
    /// Requires the write lock to be held by the caller.
    fn recover_document<T: serde::de::DeserializeOwned>(
        &self,
        rel: &str,
        path: &Path,
        reason: &str,
    ) -> StorageResult<Option<StoredDocument<T>>> {
        let preserved = self.preserve_corrupt(path)?;
        for (backup_path, raw) in self.backup_candidates(rel) {
            let decoded = match schema::decode_document(&backup_path, &raw) {
                Ok(decoded) => decoded,
                Err(_) => continue,
            };
            let Ok(data) = serde_json::from_value::<T>(decoded.data.clone()) else {
                continue;
            };
            let bytes = schema::document_bytes(&decoded.data, decoded.updated_at)?;
            write_atomic(path, &bytes)?;
            let message = format!(
                "Se recuperó {rel} desde el respaldo {}. Los cambios posteriores a esa copia se conservan en {}.",
                backup_path.display(),
                preserved.display()
            );
            self.record_notice(rel, "recovered", &message);
            self.log_event(&message);
            return Ok(Some(StoredDocument {
                version: SCHEMA_VERSION,
                updated_at: decoded.updated_at,
                data,
            }));
        }
        let message = format!(
            "Se encontró {rel} dañado ({reason}) y no había un respaldo válido. La copia dañada se conserva en {} para inspección manual.",
            preserved.display()
        );
        self.record_notice(rel, "preserved", &message);
        self.log_event(&message);
        Err(StorageError::InvalidJson {
            path: path.to_path_buf(),
            message: reason.to_string(),
        })
    }

    /// Read-only inspection used by [`StorageManager::status`].
    ///
    /// Requires the write lock to be held by the caller.
    fn inspect_document(&self, rel: &str) -> DocumentStatus {
        let path = self.document_path(rel);
        let backups = self.backup_count(rel);
        let exists = path.exists();
        let (version, valid) = if !exists {
            (None, false)
        } else {
            match fs::read_to_string(&path) {
                Ok(raw) => match schema::decode_document(&path, &raw) {
                    Ok(decoded) => (Some(decoded.version), true),
                    Err(_) => (None, false),
                },
                Err(_) => (None, false),
            }
        };
        DocumentStatus {
            path: rel.to_string(),
            exists,
            version,
            valid,
            backups,
        }
    }

    fn backup_count(&self, rel: &str) -> usize {
        let prefix = format!("{}@", document_stem(rel));
        let directory = self.root.join(BACKUP_DIR);
        fs::read_dir(directory)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| {
                        let name = entry.file_name().to_string_lossy().to_string();
                        name.starts_with(&prefix) && name.ends_with(BACKUP_SUFFIX)
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    fn measure_cache(&self) -> u64 {
        CACHE_DIRS
            .iter()
            .map(|directory| measure(&self.root.join(directory)).1)
            .sum()
    }

    fn record_notice(&self, file: &str, kind: &str, message: &str) {
        let mut state = lock(&self.state);
        if state.recovery.len() >= MAX_NOTICES {
            state.recovery.remove(0);
        }
        state.recovery.push(RecoveryNotice {
            file: file.to_string(),
            kind: kind.to_string(),
            message: message.to_string(),
        });
    }

    /// Appends a line to `logs/storage.log`, ignoring failures: logging must
    /// never break a storage operation.
    fn log_event(&self, message: &str) {
        let path = self.root.join(LOG_DIR).join("storage.log");
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(file, "[{}] {message}", now_unix());
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn ensure_known_document(rel: &str) -> StorageResult<()> {
    if is_known_document(rel) {
        Ok(())
    } else {
        Err(StorageError::UnknownDocument(rel.to_string()))
    }
}

fn is_known_document(rel: &str) -> bool {
    CONFIG_FILES.contains(&rel) || LIBRARY_FILES.contains(&rel)
}

fn document_stem(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| rel.to_string())
}

/// Default (non-portable) root, following the platform conventions.
pub fn default_root() -> PathBuf {
    platform_data_dir().join(APP_DIR_NAME)
}

/// Root for a given executable directory: portable mode is an explicit choice
/// made by placing a marker file next to the executable.
pub fn root_for(executable_directory: &Path, portable: bool) -> PathBuf {
    if portable {
        executable_directory.join(APP_DIR_NAME)
    } else {
        default_root()
    }
}

fn resolve_location() -> (PathBuf, bool) {
    let directory = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf));
    match directory {
        Some(directory) => {
            let portable = directory.join(PORTABLE_MARKER).is_file();
            (root_for(&directory, portable), portable)
        }
        None => (default_root(), false),
    }
}

fn platform_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData").join("Roaming"))
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().join("Library").join("Application Support")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".local").join("share"))
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn ensure_layout(root: &Path) -> StorageResult<()> {
    for relative in SUBDIRS {
        let path = root.join(relative);
        fs::create_dir_all(&path).map_err(|error| StorageError::Io {
            path,
            message: error.to_string(),
        })?;
    }
    let probe = root.join("cache/temporary/.write-probe");
    fs::write(&probe, b"zenith").map_err(|error| StorageError::Io {
        path: probe.clone(),
        message: error.to_string(),
    })?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

/// Writes `bytes` to `path` through a temporary file in the same directory.
///
/// The temporary file is flushed to disk before being renamed over the
/// destination, so an interruption leaves the previous copy untouched. If the
/// replacement fails, the temporary file is removed and the destination keeps
/// its current content.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> StorageResult<()> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(directory).map_err(|error| StorageError::Io {
        path: directory.to_path_buf(),
        message: error.to_string(),
    })?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "documento".to_string());
    let temporary = directory.join(format!(".{name}.tmp"));
    let outcome: std::io::Result<()> = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    match outcome {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(StorageError::Io {
                path: path.to_path_buf(),
                message: error.to_string(),
            })
        }
    }
}

fn unique_path(directory: &Path, stem: &str, extension: &str, timestamp: u64) -> PathBuf {
    let mut attempt = 0u32;
    loop {
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let name = if extension.is_empty() {
            format!("{stem}{timestamp}{suffix}")
        } else {
            format!("{stem}{timestamp}{suffix}{extension}")
        };
        let candidate = directory.join(name);
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
        if attempt > 999 {
            return candidate;
        }
    }
}

fn parse_timestamp(name: &str, prefix: &str, suffix: &str) -> Option<u64> {
    let rest = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
    let digits: String = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && Path::new(name)
            .file_name()
            .is_some_and(|file_name| file_name.to_str() == Some(name))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Counts files and bytes under a path without following symlinks.
fn measure(path: &Path) -> (usize, u64) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return (0, 0);
    };
    if metadata.is_dir() {
        let mut files = 0;
        let mut bytes = 0;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let (child_files, child_bytes) = measure(&entry.path());
                files += child_files;
                bytes += child_bytes;
            }
        }
        (files, bytes)
    } else if metadata.is_file() {
        (1, metadata.len())
    } else {
        (0, 0)
    }
}
