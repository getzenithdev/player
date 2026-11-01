mod artwork;
mod audio;
mod audio_persistence;
mod commands;
mod library;
mod library_commands;
mod lyrics;
mod metadata;
mod playlist_commands;
mod playlists;
mod resources;
mod session;
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
            if let Err(error) = library::execute(
                &app.state::<storage::StorageManager>(),
                library::LibraryAction::Snapshot,
            ) {
                eprintln!("No se pudieron preparar los metadatos: {error}");
            }
            app.manage(audio::AudioEngine::new(app.handle().clone()));
            library_commands::start_monitor(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. })
                && let Some(engine) = window.try_state::<audio::AudioEngine>()
                && let Err(error) = engine.flush()
            {
                eprintln!("No se pudo guardar la sesión: {error}");
            }
        })
        .invoke_handler(tauri::generate_handler![
            library_commands::library_action,
            lyrics::lyrics_action,
            artwork::artwork_action,
            artwork::artwork_data,
            artwork::artwork_frames,
            library_commands::library_artwork,
            playlist_commands::playlist_action,
            playlist_commands::history_snapshot,
            commands::storage_status,
            commands::appearance_get,
            commands::appearance_save,
            commands::storage_backup,
            commands::storage_export,
            commands::storage_import,
            commands::storage_clear_cache,
            commands::storage_search,
            audio::audio_queue_action,
            audio::audio_open_entries,
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
