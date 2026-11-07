//! Native operating-system integration and accessibility (roadmap phase 11).
//!
//! The module keeps one persisted settings document (`config/shortcuts.json`)
//! and applies it to four independent integrations:
//!
//! - OS media controls through [`souvlaki`]: Windows SMTC, Linux MPRIS and
//!   macOS Now Playing.
//! - A system tray icon with playback and window actions.
//! - Global media keys through the Tauri global-shortcut plugin.
//! - Optional launch at login, close-to-tray and track notifications.
//!
//! Audio state is pushed here from the audio worker; commands received from
//! the operating system are sent back through the same `AudioEngine` API the
//! interface uses, so the two paths cannot drift apart.

use std::{
    ffi::c_void,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use raw_window_handle::HasWindowHandle;
use serde::{Deserialize, Serialize};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
#[cfg(target_os = "macos")]
use tauri::menu::SubmenuBuilder;
use tauri::{
    AppHandle, Emitter, Manager, State,
    menu::{Menu, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::{
    audio::{AudioEngine, PlaybackSnapshot, PlaybackStatus},
    storage::StorageManager,
};

/// Document shared with phase 13, which will centralize every preference.
pub const SETTINGS_DOCUMENT: &str = "config/shortcuts.json";
const TRAY_ID: &str = "main";
const MEDIA_DBUS_NAME: &str = "zenithplayer";
const MEDIA_DISPLAY_NAME: &str = "Zenith Player";
const SEEK_STEP: Duration = Duration::from_secs(10);
const RESUME_THRESHOLD: Duration = Duration::from_secs(5);

/// Persisted native and accessibility preferences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IntegrationSettings {
    pub media_controls: bool,
    pub global_shortcuts: bool,
    pub notifications: bool,
    pub tray: bool,
    pub close_to_tray: bool,
    pub autostart: bool,
    pub reduced_motion: bool,
    pub high_contrast: bool,
}

impl Default for IntegrationSettings {
    fn default() -> Self {
        Self {
            media_controls: true,
            global_shortcuts: true,
            notifications: true,
            tray: true,
            close_to_tray: false,
            autostart: false,
            reduced_motion: false,
            high_contrast: false,
        }
    }
}

impl IntegrationSettings {
    /// Drops combinations the operating systems cannot honor.
    pub fn normalized(mut self) -> Self {
        if !self.tray {
            self.close_to_tray = false;
        }
        self
    }
}

/// Settings plus the runtime status the settings screen displays.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationSnapshot {
    pub settings: IntegrationSettings,
    pub platform: &'static str,
    pub lightweight_player_motion: bool,
    pub media_controls_active: bool,
    pub tray_active: bool,
    pub shortcuts_active: bool,
    pub notifications_supported: bool,
    pub autostart_supported: bool,
    pub autostart_enabled: bool,
    pub notices: Vec<String>,
}

/// Tauri-managed state shared by the audio worker, menu handlers and commands.
pub struct NativeState {
    settings: Mutex<IntegrationSettings>,
    media: Mutex<Option<MediaControls>>,
    last_media_track: Mutex<Option<String>>,
    last_notified_track: Mutex<Option<String>>,
    last_progress: Mutex<Option<Instant>>,
    notices: Mutex<Vec<String>>,
    tray_active: AtomicBool,
    media_active: AtomicBool,
    shortcuts_active: AtomicBool,
    autostart_enabled: AtomicBool,
}

impl NativeState {
    fn new(settings: IntegrationSettings) -> Self {
        Self {
            settings: Mutex::new(settings),
            media: Mutex::new(None),
            last_media_track: Mutex::new(None),
            last_notified_track: Mutex::new(None),
            last_progress: Mutex::new(None),
            notices: Mutex::new(Vec::new()),
            tray_active: AtomicBool::new(false),
            media_active: AtomicBool::new(false),
            shortcuts_active: AtomicBool::new(false),
            autostart_enabled: AtomicBool::new(false),
        }
    }

    fn settings(&self) -> IntegrationSettings {
        lock(&self.settings).clone()
    }

    fn snapshot(&self) -> IntegrationSnapshot {
        IntegrationSnapshot {
            settings: self.settings(),
            platform: std::env::consts::OS,
            lightweight_player_motion: lightweight_player_motion(),
            media_controls_active: self.media_active.load(Ordering::Relaxed),
            tray_active: self.tray_active.load(Ordering::Relaxed),
            shortcuts_active: self.shortcuts_active.load(Ordering::Relaxed),
            notifications_supported: true,
            autostart_supported: true,
            autostart_enabled: self.autostart_enabled.load(Ordering::Relaxed),
            notices: lock(&self.notices).clone(),
        }
    }

    fn set_notices(&self, notices: Vec<String>) {
        *lock(&self.notices) = notices;
    }

    fn add_notice(&self, notice: impl Into<String>) {
        lock(&self.notices).push(notice.into());
    }
}

/// Only Monterey on Intel hardware. An x86_64 process under Rosetta is excluded.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn lightweight_player_motion() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        fn read(name: &std::ffi::CStr) -> Option<Vec<u8>> {
            let mut bytes = vec![0u8; 128];
            let mut size = bytes.len();
            // SAFETY: name is NUL-terminated and the writable buffer has size bytes.
            // Both new-value arguments are null/zero: this only reads the sysctl.
            let result = unsafe {
                libc::sysctlbyname(name.as_ptr(), bytes.as_mut_ptr().cast(), &mut size,
                    std::ptr::null_mut(), 0)
            };
            if result != 0 || size > bytes.len() { return None; }
            bytes.truncate(size);
            Some(bytes)
        }
        fn flag(name: &std::ffi::CStr) -> Option<i32> {
            Some(i32::from_ne_bytes(read(name)?.try_into().ok()?))
        }
        if flag(c"sysctl.proc_translated") == Some(1)
            || flag(c"hw.optional.arm64") == Some(1) { return false; }
        let Some(version) = read(c"kern.osproductversion") else { return false; };
        let Ok(version) = std::str::from_utf8(&version) else { return false; };
        version.trim_end_matches('\0').split('.').next() == Some("12")
    })
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn lightweight_player_motion() -> bool {
    false
}

/// Loads persisted settings, manages the state and applies every integration.
/// Failures are recorded as notices instead of blocking application startup.
pub fn setup(app: &AppHandle) {
    let settings = load_settings(&app.state::<StorageManager>()).normalized();
    app.manage(NativeState::new(settings));
    apply(app);
    if std::env::args().any(|argument| argument == "--hidden") {
        hide_main_window(app);
    }
}

fn load_settings(storage: &StorageManager) -> IntegrationSettings {
    storage
        .load_document::<IntegrationSettings>(SETTINGS_DOCUMENT)
        .ok()
        .flatten()
        .map(|document| document.data.normalized())
        .unwrap_or_default()
}

/// Reapplies the current settings and refreshes the reported status.
pub fn apply(app: &AppHandle) {
    let settings = match app.try_state::<NativeState>() {
        Some(state) => state.settings(),
        None => return,
    };
    let Some(state) = app.try_state::<NativeState>() else {
        return;
    };
    let mut notices = Vec::new();

    match ensure_tray(app, settings.tray) {
        Ok(active) => state.tray_active.store(active, Ordering::Relaxed),
        Err(error) => {
            state.tray_active.store(false, Ordering::Relaxed);
            notices.push(error);
        }
    }

    match ensure_media_controls(app, settings.media_controls) {
        Ok(active) => state.media_active.store(active, Ordering::Relaxed),
        Err(error) => {
            state.media_active.store(false, Ordering::Relaxed);
            notices.push(error);
        }
    }

    let shortcut_notices = apply_shortcuts(app, settings.global_shortcuts);
    state.shortcuts_active.store(
        settings.global_shortcuts && shortcut_notices.is_empty(),
        Ordering::Relaxed,
    );
    notices.extend(shortcut_notices);

    if let Some(error) = apply_autostart(app, settings.autostart) {
        notices.push(error);
    }
    if let Some(manager) = autostart_manager(app) {
        let enabled = manager.is_enabled().unwrap_or(settings.autostart);
        state.autostart_enabled.store(enabled, Ordering::Relaxed);
    }

    if let Err(error) = install_app_menu(app) {
        notices.push(error);
    }

    state.set_notices(notices);
}

fn ensure_tray(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let existing = app.tray_by_id(TRAY_ID);
    if !enabled {
        if let Some(tray) = existing {
            tray.set_visible(false)
                .map_err(|error| format!("No se pudo ocultar la bandeja: {error}"))?;
        }
        return Ok(false);
    }
    if let Some(tray) = existing {
        tray.set_visible(true)
            .map_err(|error| format!("No se pudo mostrar la bandeja: {error}"))?;
        return Ok(true);
    }

    let toggle = MenuItemBuilder::with_id("native-toggle", "Reproducir o pausar")
        .build(app)
        .map_err(|error| error.to_string())?;
    let previous = MenuItemBuilder::with_id("native-previous", "Anterior")
        .build(app)
        .map_err(|error| error.to_string())?;
    let next = MenuItemBuilder::with_id("native-next", "Siguiente")
        .build(app)
        .map_err(|error| error.to_string())?;
    let show = MenuItemBuilder::with_id("native-show", "Mostrar ventana")
        .build(app)
        .map_err(|error| error.to_string())?;
    let quit = MenuItemBuilder::with_id("native-quit", "Salir de Zenith")
        .build(app)
        .map_err(|error| error.to_string())?;
    let separator =
        tauri::menu::PredefinedMenuItem::separator(app).map_err(|error| error.to_string())?;
    let menu = Menu::with_items(app, &[&toggle, &previous, &next, &separator, &show, &quit])
        .map_err(|error| error.to_string())?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(MEDIA_DISPLAY_NAME)
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .build(app)
        .map_err(|error| format!("No se pudo crear la bandeja del sistema: {error}"))?;
    Ok(true)
}

fn ensure_media_controls(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let state = app
        .try_state::<NativeState>()
        .ok_or_else(|| "La integración nativa aún no está disponible.".to_string())?;
    if !enabled {
        *lock(&state.media) = None;
        *lock(&state.last_media_track) = None;
        return Ok(false);
    }
    if lock(&state.media).is_some() {
        return Ok(true);
    }

    let mut controls = MediaControls::new(platform_config(app))
        .map_err(|error| format!("No se pudieron crear los controles multimedia: {error}"))?;
    let handler_app = app.clone();
    controls
        .attach(move |event| on_media_event(&handler_app, event))
        .map_err(|error| format!("No se pudieron escuchar los controles multimedia: {error}"))?;
    *lock(&state.media) = Some(controls);
    *lock(&state.last_media_track) = None;
    Ok(true)
}

fn platform_config(_app: &AppHandle) -> PlatformConfig<'static> {
    #[cfg(target_os = "windows")]
    let hwnd = _app.get_webview_window("main").and_then(|window| {
        let handle = window.window_handle().ok()?;
        match handle.as_raw() {
            raw_window_handle::RawWindowHandle::Win32(handle) => {
                Some(handle.hwnd.get() as *mut c_void)
            }
            _ => None,
        }
    });
    #[cfg(not(target_os = "windows"))]
    let hwnd: Option<*mut c_void> = None;

    PlatformConfig {
        dbus_name: MEDIA_DBUS_NAME,
        display_name: MEDIA_DISPLAY_NAME,
        hwnd,
    }
}

fn apply_shortcuts(app: &AppHandle, enabled: bool) -> Vec<String> {
    let shortcuts = app.global_shortcut();
    let mut notices = Vec::new();
    if let Err(error) = shortcuts.unregister_all() {
        notices.push(format!(
            "No se pudieron liberar los atajos globales: {error}"
        ));
    }
    if !enabled {
        return notices;
    }
    for (shortcut, label) in [
        (
            Shortcut::new(None, Code::MediaPlayPause),
            "reproducir o pausar",
        ),
        (Shortcut::new(None, Code::MediaTrackNext), "siguiente"),
        (Shortcut::new(None, Code::MediaTrackPrevious), "anterior"),
        (Shortcut::new(None, Code::MediaStop), "detener"),
    ] {
        if let Err(error) = shortcuts.register(shortcut) {
            notices.push(format!(
                "El sistema no permitió la tecla multimedia de {label}: {error}"
            ));
        }
    }
    notices
}

fn autostart_manager(
    app: &AppHandle,
) -> Option<State<'_, tauri_plugin_autostart::AutoLaunchManager>> {
    app.try_state::<tauri_plugin_autostart::AutoLaunchManager>()
}

fn apply_autostart(app: &AppHandle, enabled: bool) -> Option<String> {
    let manager = autostart_manager(app)?;
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result
        .err()
        .map(|error| format!("No se pudo cambiar el inicio automático: {error}"))
}

#[cfg(target_os = "macos")]
fn install_app_menu(app: &AppHandle) -> Result<(), String> {
    let menu = Menu::default(app).map_err(|error| error.to_string())?;
    let playback = SubmenuBuilder::new(app, "Reproducción")
        .item(
            &MenuItemBuilder::with_id("native-toggle", "Reproducir o pausar")
                .build(app)
                .map_err(|error| error.to_string())?,
        )
        .item(
            &MenuItemBuilder::with_id("native-previous", "Anterior")
                .build(app)
                .map_err(|error| error.to_string())?,
        )
        .item(
            &MenuItemBuilder::with_id("native-next", "Siguiente")
                .build(app)
                .map_err(|error| error.to_string())?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("native-show", "Mostrar ventana")
                .build(app)
                .map_err(|error| error.to_string())?,
        )
        .build()
        .map_err(|error| error.to_string())?;
    menu.append(&playback).map_err(|error| error.to_string())?;
    menu.set_as_app_menu().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn install_app_menu(_app: &AppHandle) -> Result<(), String> {
    Ok(())
}

/// Routes application-menu and tray-menu identifiers to the audio engine.
pub fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        "native-toggle" => audio_action(app, AudioAction::Toggle),
        "native-previous" => audio_action(app, AudioAction::Previous),
        "native-next" => audio_action(app, AudioAction::Next),
        "native-show" => show_main_window(app),
        "native-quit" => quit(app),
        _ => {}
    }
}

/// Handles a registered global media key.
pub fn on_global_shortcut(app: &AppHandle, shortcut: &Shortcut) {
    let Some(state) = app.try_state::<NativeState>() else {
        return;
    };
    if !state.settings().global_shortcuts {
        return;
    }
    let pressed = |code: Code| shortcut == &Shortcut::new(None, code);
    if pressed(Code::MediaPlayPause) {
        audio_action(app, AudioAction::Toggle);
    } else if pressed(Code::MediaTrackNext) {
        audio_action(app, AudioAction::Next);
    } else if pressed(Code::MediaTrackPrevious) {
        audio_action(app, AudioAction::Previous);
    } else if pressed(Code::MediaStop) {
        audio_action(app, AudioAction::Stop);
    }
}

#[derive(Clone, Copy)]
enum AudioAction {
    Toggle,
    Next,
    Previous,
    Stop,
}

fn audio_action(app: &AppHandle, action: AudioAction) {
    let Some(engine) = app.try_state::<AudioEngine>() else {
        return;
    };
    let result = match action {
        AudioAction::Toggle => engine.toggle(),
        AudioAction::Next => engine.next(),
        AudioAction::Previous => engine.previous(),
        AudioAction::Stop => engine.stop(),
    };
    if let Err(error) = result {
        eprintln!("No se pudo ejecutar la acción nativa: {error}");
    }
}

fn on_media_event(app: &AppHandle, event: MediaControlEvent) {
    let Some(engine) = app.try_state::<AudioEngine>() else {
        return;
    };
    match event {
        MediaControlEvent::Play => drop(engine.play()),
        MediaControlEvent::Pause => drop(engine.pause()),
        MediaControlEvent::Toggle => drop(engine.toggle()),
        MediaControlEvent::Next => drop(engine.next()),
        MediaControlEvent::Previous => drop(engine.previous()),
        MediaControlEvent::Stop => drop(engine.stop()),
        MediaControlEvent::Seek(direction) => {
            let Some(state) = app.try_state::<NativeState>() else {
                return;
            };
            if let Some(controls) = lock(&state.media).as_mut() {
                drop(controls.set_playback(playback_of(&engine.snapshot())));
            }
            seek_relative(&engine, direction, SEEK_STEP);
        }
        MediaControlEvent::SeekBy(direction, amount) => {
            seek_relative(&engine, direction, amount);
        }
        MediaControlEvent::SetPosition(position) => {
            let _ = engine.seek(duration_ms(position.0));
        }
        MediaControlEvent::SetVolume(volume) => {
            let _ = engine.set_volume(volume.clamp(0.0, 1.0) as f32);
        }
        MediaControlEvent::SetShuffle(enabled) => {
            let _ = engine.set_shuffle(enabled);
        }
        MediaControlEvent::Raise => show_main_window(app),
        MediaControlEvent::Quit => quit(app),
        _ => {}
    }
}

fn seek_relative(engine: &AudioEngine, direction: SeekDirection, amount: Duration) {
    let snapshot = engine.snapshot();
    let current = Duration::from_millis(snapshot.position_ms);
    let target = match direction {
        SeekDirection::Forward => current.saturating_add(amount),
        SeekDirection::Backward => current.saturating_sub(amount),
    };
    let target = snapshot
        .duration_ms
        .map(Duration::from_millis)
        .map_or(target, |duration| target.min(duration));
    let _ = engine.seek(duration_ms(target));
}

/// Pushes a new playback snapshot to the OS media controls.
///
/// Called by the audio worker after publishing. The first snapshot of every
/// track updates the metadata; the rest only update the playback state.
pub fn sync_snapshot(app: &AppHandle, snapshot: &PlaybackSnapshot) {
    let Some(state) = app.try_state::<NativeState>() else {
        return;
    };
    let settings = state.settings();

    if settings.media_controls {
        let track_key = snapshot
            .track
            .as_ref()
            .map(|track| track.id.clone())
            .filter(|id| !id.is_empty());
        let changed = *lock(&state.last_media_track) != track_key;
        let mut media = lock(&state.media);
        if let Some(controls) = media.as_mut() {
            if changed {
                let metadata = match snapshot.track.as_ref() {
                    Some(track) => MediaMetadata {
                        title: Some(track.title.as_str()),
                        artist: track.artist.as_deref(),
                        album: track.album.as_deref(),
                        cover_url: None,
                        duration: snapshot.duration_ms.map(Duration::from_millis),
                    },
                    None => MediaMetadata::default(),
                };
                let _ = controls.set_metadata(metadata);
                *lock(&state.last_media_track) = track_key;
            }
            let _ = controls.set_playback(playback_of(snapshot));
        }
    }

    if settings.notifications {
        notify_track(app, &state, snapshot);
    }
}

/// Updates only the progress reported to the media controls once per second.
pub fn sync_progress(
    app: &AppHandle,
    status: &PlaybackStatus,
    position_ms: u64,
    duration_ms: Option<u64>,
) {
    let Some(state) = app.try_state::<NativeState>() else {
        return;
    };
    if !state.settings().media_controls {
        return;
    }
    let now = Instant::now();
    {
        let mut last = lock(&state.last_progress);
        if last.is_some_and(|value| now.duration_since(value) < Duration::from_secs(1)) {
            return;
        }
        *last = Some(now);
    }
    let playback = match status {
        PlaybackStatus::Playing => MediaPlayback::Playing {
            progress: Some(MediaPosition(Duration::from_millis(position_ms))),
        },
        PlaybackStatus::Paused => MediaPlayback::Paused {
            progress: Some(MediaPosition(Duration::from_millis(position_ms))),
        },
        _ => MediaPlayback::Stopped,
    };
    if let Some(controls) = lock(&state.media).as_mut() {
        let _ = controls.set_playback(playback);
    }
    let _ = duration_ms;
}

fn playback_of(snapshot: &PlaybackSnapshot) -> MediaPlayback {
    let progress = Some(MediaPosition(Duration::from_millis(snapshot.position_ms)));
    match snapshot.status {
        PlaybackStatus::Playing => MediaPlayback::Playing { progress },
        PlaybackStatus::Paused => MediaPlayback::Paused { progress },
        _ => MediaPlayback::Stopped,
    }
}

fn notify_track(app: &AppHandle, state: &NativeState, snapshot: &PlaybackSnapshot) {
    // Track changes are published as `Loading` first, so a notification is
    // only committed once playback actually starts.
    if snapshot.status != PlaybackStatus::Playing {
        return;
    }
    let Some(track) = snapshot.track.as_ref() else {
        return;
    };
    let key = if track.id.is_empty() {
        track.path.clone()
    } else {
        track.id.clone()
    };
    {
        let mut last = lock(&state.last_notified_track);
        if *last == Some(key.clone()) {
            return;
        }
        *last = Some(key);
    }
    if app
        .get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
    {
        return;
    }
    let body = [track.artist.as_deref(), track.album.as_deref()]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let builder = app
        .notification()
        .builder()
        .title(&track.title)
        .body(if body.is_empty() {
            "Zenith Player"
        } else {
            &body
        });
    if let Err(error) = builder.show() {
        state.add_notice(format!("No se pudo mostrar la notificación: {error}"));
    }
}

/// Returns true when the window must stay alive in the tray instead of closing.
pub fn should_hide_on_close<R: tauri::Runtime>(window: &tauri::Window<R>) -> bool {
    let Some(state) = window.try_state::<NativeState>() else {
        return false;
    };
    let settings = state.settings();
    settings.close_to_tray && state.tray_active.load(Ordering::Relaxed)
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn quit(app: &AppHandle) {
    if let Some(engine) = app.try_state::<AudioEngine>() {
        let _ = engine.flush();
    }
    app.exit(0);
}

/// True when the worker thread was suspended long enough to need a fresh
/// audio output. Sleep and hibernate do not deliver a portable resume event,
/// so the engine uses this elapsed-time check instead of guessing per OS.
pub fn resumed_after_suspend(elapsed: Duration) -> bool {
    elapsed >= RESUME_THRESHOLD
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tauri::command]
pub fn integration_get(state: State<'_, NativeState>) -> IntegrationSnapshot {
    state.snapshot()
}

#[tauri::command]
pub fn integration_save(
    app: AppHandle,
    state: State<'_, NativeState>,
    settings: IntegrationSettings,
) -> Result<IntegrationSnapshot, String> {
    let settings = settings.normalized();
    app.state::<StorageManager>()
        .save_document(SETTINGS_DOCUMENT, &settings)
        .map_err(|error| error.to_string())?;
    *lock(&state.settings) = settings;
    apply(&app);
    announce(&app, "Preferencias del sistema actualizadas.");
    Ok(state.snapshot())
}

#[tauri::command]
pub fn integration_notification_test(app: AppHandle) -> Result<(), String> {
    app.notification()
        .builder()
        .title("Zenith Player")
        .body("Las notificaciones del sistema están activas.")
        .show()
        .map_err(|error| error.to_string())
}

/// Emits a short in-app status message; used by the settings screen after a
/// successful save when the window is visible.
pub fn announce(app: &AppHandle, message: &str) {
    let _ = app.emit("native:notice", message.to_string());
}

/// Helper used by the global-shortcut plugin handler.
pub fn shortcut_pressed(state: ShortcutState) -> bool {
    state == ShortcutState::Pressed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageManager;
    use std::fs;

    #[test]
    fn default_settings_enable_system_controls_without_autostart() {
        let settings = IntegrationSettings::default();
        assert!(settings.media_controls);
        assert!(settings.global_shortcuts);
        assert!(settings.notifications);
        assert!(settings.tray);
        assert!(!settings.close_to_tray);
        assert!(!settings.autostart);
        assert!(!settings.reduced_motion);
        assert!(!settings.high_contrast);
    }

    #[test]
    fn closing_to_the_tray_requires_the_tray() {
        let settings = IntegrationSettings {
            tray: false,
            close_to_tray: true,
            ..IntegrationSettings::default()
        }
        .normalized();
        assert!(!settings.close_to_tray);
    }

    #[test]
    fn settings_survive_a_restart_and_ignore_unknown_fields() {
        let root = std::env::temp_dir().join(format!("zenith-native-{}", std::process::id()));
        let storage = StorageManager::open(root.join("data"), false).unwrap();
        let settings = IntegrationSettings {
            notifications: false,
            reduced_motion: true,
            ..IntegrationSettings::default()
        };
        storage.save_document(SETTINGS_DOCUMENT, &settings).unwrap();
        assert_eq!(load_settings(&storage), settings);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_settings_fall_back_to_defaults() {
        let root = std::env::temp_dir().join(format!("zenith-native-empty-{}", std::process::id()));
        let storage = StorageManager::open(root.join("data"), false).unwrap();
        assert_eq!(load_settings(&storage), IntegrationSettings::default());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn long_delays_are_treated_as_a_resume() {
        assert!(!resumed_after_suspend(Duration::from_millis(200)));
        assert!(resumed_after_suspend(RESUME_THRESHOLD));
        assert!(resumed_after_suspend(Duration::from_secs(60)));
    }

    #[test]
    fn duration_conversion_is_bounded() {
        assert_eq!(duration_ms(Duration::from_millis(1500)), 1500);
        assert_eq!(duration_ms(Duration::from_secs(3)), 3000);
    }
}
