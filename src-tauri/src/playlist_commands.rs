use crate::{
    playlists::{self, PlaylistAction, PlaylistSnapshot},
    storage::StorageManager,
};
use tauri::{Emitter, Manager};
#[tauri::command]
pub async fn playlist_action(
    app: tauri::AppHandle,
    action: PlaylistAction,
) -> Result<PlaylistSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mutation = !matches!(
            action,
            PlaylistAction::Snapshot | PlaylistAction::Export { .. }
        );
        let result = playlists::execute(&app.state::<StorageManager>(), action)?;
        if mutation {
            let _ = app.emit("playlists:changed", ());
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn history_snapshot(
    app: tauri::AppHandle,
) -> Result<crate::session::HistoryDocument, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let storage = app.state::<StorageManager>();
        let _operation = storage.library_lock();
        let mut doc = crate::session::load(&storage)?;
        let library = playlists::library_tracks(&storage)?;
        for visit in &mut doc.visits {
            playlists::resolve_entries(std::slice::from_mut(&mut visit.entry), &library);
        }
        Ok(doc)
    })
    .await
    .map_err(|e| e.to_string())?
}
