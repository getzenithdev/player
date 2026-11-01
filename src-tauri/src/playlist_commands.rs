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
