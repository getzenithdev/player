mod audio;
mod commands;
mod storage;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Storage preparation never fails: a broken location degrades the manager
    // to read-only instead of blocking the application from starting.
    let storage = storage::StorageManager::initialize();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(storage)
        .setup(|app| {
            // The audio worker opens the output device in the background so a
            // missing or busy device never blocks application startup.
            app.manage(audio::AudioEngine::new(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::storage_status,
            commands::appearance_get,
            commands::appearance_save,
            commands::storage_backup,
            commands::storage_export,
            commands::storage_import,
            commands::storage_clear_cache,
            commands::storage_search,
            audio::audio_get_state,
            audio::audio_open_files,
            audio::audio_play_index,
            audio::audio_play,
            audio::audio_pause,
            audio::audio_toggle,
            audio::audio_stop,
            audio::audio_next,
            audio::audio_previous,
            audio::audio_seek,
            audio::audio_set_volume,
            audio::audio_set_muted,
            audio::audio_set_repeat,
            audio::audio_set_shuffle,
            audio::audio_restart_output,
            audio::audio_clear_queue,
            audio::audio_capabilities,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zenith Player");
}
