mod artwork;
mod audio;
mod audio_persistence;
mod audio_processing;
mod commands;
mod library;
mod library_removal;
mod library_commands;
mod lyrics;
mod metadata;
mod native;
mod network_audio;
mod playlist_commands;
mod playlists;
mod resources;
mod session;
mod settings;
mod sources;
mod storage;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Storage preparation never fails: a broken location degrades the manager
    // to read-only instead of blocking the application from starting.
    let storage = storage::StorageManager::initialize();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if native::shortcut_pressed(event.state()) {
                        native::on_global_shortcut(app, shortcut);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            native::setup(app.handle());
            app.manage(audio::AudioEngine::new(app.handle().clone()));
            library_commands::start_monitor(app.handle().clone());
            Ok(())
        })
        .on_menu_event(|app, event| native::on_menu_event(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(focused) = event {
                let visible = *focused && window.is_visible().unwrap_or(false)
                    && !window.is_minimized().unwrap_or(false);
                let _ = window.emit("artwork:visibility", visible);
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Closing to the tray keeps playback alive; the application
                // only exits through the tray or menu quit action.
                if native::should_hide_on_close(window) {
                    api.prevent_close();
                    let _ = window.emit("artwork:visibility", false);
                    let _ = window.hide();
                    return;
                }
                if let Some(engine) = window.try_state::<audio::AudioEngine>()
                    && let Err(error) = engine.flush()
                {
                    eprintln!("No se pudo guardar la sesión: {error}");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            library_commands::library_action,
            lyrics::lyrics_action,
            artwork::artwork_action,
            artwork::artwork_data,
            artwork::artwork_frames,
            artwork::artist_artwork,
            library_commands::library_artwork,
            playlist_commands::playlist_action,
            playlist_commands::history_snapshot,
            sources::sources_action,
            commands::storage_status,
            commands::appearance_get,
            commands::appearance_save,
            commands::storage_backup,
            commands::storage_export,
            commands::storage_import,
            commands::storage_clear_cache,
            commands::storage_search,
            settings::settings_get,
            settings::settings_save,
            settings::settings_reset,
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
            audio::audio_processing_get,
            audio::audio_processing_save,
            audio::audio_capabilities,
            native::integration_get,
            native::integration_save,
            native::integration_notification_test,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zenith Player");
}
