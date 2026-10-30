//! Tauri commands exposing the storage layer to the interface (phase 2).
//!
//! Commands are thin wrappers: validation, atomic writes, backups and
//! recovery live in [`crate::storage::StorageManager`], which is covered by
//! unit tests. Errors are converted to strings because that is what the
//! interface displays.

use std::path::Path;

use tauri::State;

use crate::storage::{
    BackupReport, CacheReport, ExportReport, ImportReport, StorageManager, StorageStatus,
    indexes::TrackHit, schema::AppearanceData,
};

/// Snapshot of the storage layer: root, permissions, documents, notices and
/// index counters.
#[tauri::command]
pub fn storage_status(manager: State<'_, StorageManager>) -> StorageStatus {
    manager.status()
}

/// Current appearance preferences, with defaults when nothing is stored yet.
#[tauri::command]
pub fn appearance_get(manager: State<'_, StorageManager>) -> Result<AppearanceData, String> {
    manager.load_appearance().map_err(|error| error.to_string())
}

/// Persists one of the accepted themes (`light`, `dark`, `system`).
#[tauri::command]
pub fn appearance_save(
    manager: State<'_, StorageManager>,
    theme: String,
) -> Result<AppearanceData, String> {
    manager
        .save_appearance(&theme)
        .map_err(|error| error.to_string())
}

/// Copies every existing document into `backups/`.
#[tauri::command]
pub fn storage_backup(manager: State<'_, StorageManager>) -> Result<BackupReport, String> {
    manager.create_backup().map_err(|error| error.to_string())
}

/// Exports configuration, library and lyrics without credentials.
#[tauri::command]
pub fn storage_export(manager: State<'_, StorageManager>) -> Result<ExportReport, String> {
    manager.export_bundle().map_err(|error| error.to_string())
}

/// Imports a previously exported bundle; an empty path selects the newest one.
#[tauri::command]
pub fn storage_import(
    manager: State<'_, StorageManager>,
    path: Option<String>,
) -> Result<ImportReport, String> {
    manager
        .import_bundle(path.as_deref().map(Path::new))
        .map_err(|error| error.to_string())
}

/// Clears the cache directories without touching documents or music files.
#[tauri::command]
pub fn storage_clear_cache(manager: State<'_, StorageManager>) -> Result<CacheReport, String> {
    manager.clear_cache().map_err(|error| error.to_string())
}

/// Case-insensitive search over the in-memory indexes.
#[tauri::command]
pub fn storage_search(manager: State<'_, StorageManager>, query: String) -> Vec<TrackHit> {
    manager.search(&query)
}
