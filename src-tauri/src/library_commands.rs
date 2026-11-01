use super::library::*;
use crate::storage::StorageManager;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::fs;
use tauri::{Emitter, Manager};

#[tauri::command]
pub async fn library_action(
    app: tauri::AppHandle,
    action: LibraryAction,
) -> Result<LibrarySnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mutation = !matches!(action, LibraryAction::Snapshot);
        let snapshot = execute(&app.state::<StorageManager>(), action)?;
        // Events carry no library payload, preventing large snapshots from
        // being duplicated in every open window.
        if mutation {
            let _ = app.emit("library:changed", ());
        }
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn library_artwork(
    storage: tauri::State<'_, StorageManager>,
    id: String,
) -> Result<String, String> {
    let (hash, ext) = id.split_once('.').ok_or("Portada inválida")?;
    if hash.len() != 64
        || !hash.bytes().all(|b| b.is_ascii_hexdigit())
        || !matches!(ext, "png" | "jpg")
    {
        return Err("Portada inválida".into());
    }
    let data = fs::read(storage.document_path(&format!("artwork/local/{id}")))
        .map_err(|e| e.to_string())?;
    if data.len() > 10 * 1024 * 1024 {
        return Err("Portada demasiado grande".into());
    }
    Ok(format!(
        "data:image/{};base64,{}",
        if ext == "jpg" { "jpeg" } else { ext },
        STANDARD.encode(data)
    ))
}
/// Portable watcher: stat-based polling reconciles every ten seconds, including
/// moves and changes made while the app was closed. Hash/tag reads are cached.
pub fn start_monitor(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
            let storage = app.state::<StorageManager>();
            let _operation = storage.library_lock();
            let mut warnings = Vec::new();
            match load(&storage) {
                Ok(mut doc) => {
                    let before = doc.clone();
                    reconcile(&storage, &mut doc, &mut warnings);
                    if doc != before {
                        match save(&storage, &doc) {
                            Ok(()) => {
                                let _ = app.emit("library:changed", ());
                            }
                            Err(error) => warnings.push(error),
                        }
                    }
                }
                Err(error) => warnings.push(error),
            }
            let _ = app.emit("library:warnings", warnings);
        }
    });
}
