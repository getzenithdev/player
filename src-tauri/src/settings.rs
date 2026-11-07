//! Centralized application preferences (roadmap phase 13).
//!
//! One versioned document (`config/settings.json`) owns the preferences that
//! cross module boundaries: general behaviour, the extended appearance
//! options, playback defaults and the global lyrics/artwork presentation.
//!
//! Module documents remain authoritative for operational state (native
//! integration in `config/shortcuts.json`, audio processing in
//! `config/audio.json`, sources in `config/providers.json`). Saving this
//! document mirrors the theme, lyric and artwork preferences into those
//! existing stores so every screen keeps reading the same values it already
//! understands. Credentials never enter this document.
//!
//! Values are validated strictly before writing and normalized after loading,
//! so an old or hand-edited file can never bypass a range check.

use serde::{Deserialize, Serialize};

use crate::{
    artwork, lyrics,
    storage::{StorageManager, schema},
};

/// Document shared by the settings center and every module reader.
pub const SETTINGS_DOCUMENT: &str = "config/settings.json";

const LANGUAGES: [&str; 2] = ["es", "system"];
const ACCENTS: [&str; 5] = ["violet", "plum", "rose", "teal", "amber"];
const ANIMATIONS: [&str; 2] = ["full", "reduced"];
const BACKGROUNDS: [&str; 2] = ["dynamic", "solid"];
const PLAYER_LAYOUTS: [&str; 2] = ["full", "compact"];
const LYRICS_STYLES: [&str; 3] = ["balanced", "focus", "lyrics"];
const REPEAT_MODES: [&str; 3] = ["off", "all", "one"];
const LYRIC_PROVIDERS: [&str; 3] = ["local", "embedded", "lrclib"];
const LYRIC_FONTS: [&str; 3] = ["system", "serif", "sans-serif"];
const ARTWORK_PROVIDERS: [&str; 4] = ["local", "apple-music", "archive-org", "youtube"];
const ARTWORK_QUALITIES: [&str; 2] = ["thumbnail", "preview"];

const MIN_SCALE: u16 = 75;
const MAX_SCALE: u16 = 200;
const MIN_LYRIC_SIZE: u16 = 16;
const MAX_LYRIC_SIZE: u16 = 80;
const MIN_LYRIC_OFFSET: i64 = -600_000;
const MAX_LYRIC_OFFSET: i64 = 600_000;

/// General behaviour that is not owned by a single module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralSettings {
    /// Interface language. `system` follows the operating system.
    pub language: String,
    /// Asks before destructive actions such as clearing the cache.
    pub confirm_destructive: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: "es".into(),
            confirm_destructive: true,
        }
    }
}

/// Extended appearance preferences. The theme is mirrored to
/// `config/appearance.json`; the rest is applied by the interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppearanceSettings {
    pub theme: String,
    pub accent: String,
    /// Interface scale in percent.
    pub scale: u16,
    pub animations: String,
    pub background: String,
    pub player_layout: String,
    pub lyrics_style: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            accent: "violet".into(),
            scale: 100,
            animations: "full".into(),
            background: "dynamic".into(),
            player_layout: "full".into(),
            lyrics_style: "balanced".into(),
        }
    }
}

/// Defaults used when playback starts without a restored session. The audio
/// engine keeps its own session volume, shuffle and repeat once a queue exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlaybackSettings {
    /// Initial volume in percent.
    pub initial_volume: u8,
    pub shuffle: bool,
    pub repeat: String,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            initial_volume: 70,
            shuffle: false,
            repeat: "off".into(),
        }
    }
}

/// Global lyric presentation, mirrored to `library/lyrics.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LyricsSettings {
    /// Providers queried in order. `local` and `embedded` never hit the network.
    pub providers: Vec<String>,
    pub size: u16,
    pub font: String,
    pub color: String,
    pub active_color: String,
    /// Positive values show a line earlier, following the LRC convention.
    pub offset_ms: i64,
}

impl Default for LyricsSettings {
    fn default() -> Self {
        Self {
            providers: LYRIC_PROVIDERS
                .iter()
                .map(|value| (*value).into())
                .collect(),
            size: 30,
            font: "system".into(),
            color: "#777777".into(),
            active_color: "#8956dd".into(),
            offset_ms: 0,
        }
    }
}

/// Global artwork presentation, mirrored to `library/artwork.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArtworkSettings {
    /// Provider priority, highest first. The first entry wins ties.
    pub provider_priority: Vec<String>,
    /// Plays animated covers when the track is visible.
    pub animate: bool,
    pub loop_animation: bool,
    pub battery_saver: bool,
    pub quality: String,
    /// Allows downloading provider images into the local cache.
    pub allow_downloads: bool,
}

impl Default for ArtworkSettings {
    fn default() -> Self {
        Self {
            provider_priority: ARTWORK_PROVIDERS
                .iter()
                .map(|value| (*value).into())
                .collect(),
            animate: true,
            loop_animation: true,
            battery_saver: false,
            quality: "preview".into(),
            allow_downloads: false,
        }
    }
}

/// Everything stored in `config/settings.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub playback: PlaybackSettings,
    pub lyrics: LyricsSettings,
    pub artwork: ArtworkSettings,
}

/// Settings plus the document metadata shown by the settings center.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub settings: AppSettings,
    pub document: String,
    /// False the first time the document has not been written yet.
    pub stored: bool,
}

impl AppSettings {
    /// Rejects a value the interface or a hand-edited file could send.
    ///
    /// Numeric range violations and unknown enumerations are errors instead of
    /// silent fallbacks; normalization only repairs loaded documents.
    pub fn validate(&self) -> Result<(), String> {
        if !LANGUAGES.contains(&self.general.language.as_str()) {
            return Err("Idioma no compatible".into());
        }
        if !schema::is_valid_theme(&self.appearance.theme) {
            return Err("Tema no compatible".into());
        }
        if !ACCENTS.contains(&self.appearance.accent.as_str()) {
            return Err("Color de acento no compatible".into());
        }
        if !(MIN_SCALE..=MAX_SCALE).contains(&self.appearance.scale) {
            return Err(format!(
                "La escala debe estar entre {MIN_SCALE} y {MAX_SCALE} %"
            ));
        }
        for (value, allowed, message) in [
            (
                &self.appearance.animations,
                &ANIMATIONS[..],
                "Animaciones no válidas",
            ),
            (
                &self.appearance.background,
                &BACKGROUNDS[..],
                "Fondo no válido",
            ),
            (
                &self.appearance.player_layout,
                &PLAYER_LAYOUTS[..],
                "Disposición del reproductor no válida",
            ),
            (
                &self.appearance.lyrics_style,
                &LYRICS_STYLES[..],
                "Estilo de letras no válido",
            ),
        ] {
            if !allowed.contains(&value.as_str()) {
                return Err(message.into());
            }
        }
        if !REPEAT_MODES.contains(&self.playback.repeat.as_str()) {
            return Err("Modo de repetición no válido".into());
        }
        if self.playback.initial_volume > 100 {
            return Err("El volumen inicial debe estar entre 0 y 100".into());
        }
        if self.lyrics.providers.is_empty()
            || self
                .lyrics
                .providers
                .iter()
                .any(|value| !LYRIC_PROVIDERS.contains(&value.as_str()))
        {
            return Err("Proveedores de letras no válidos".into());
        }
        if !(MIN_LYRIC_SIZE..=MAX_LYRIC_SIZE).contains(&self.lyrics.size) {
            return Err(format!(
                "El tamaño de letra debe estar entre {MIN_LYRIC_SIZE} y {MAX_LYRIC_SIZE}"
            ));
        }
        if !LYRIC_FONTS.contains(&self.lyrics.font.as_str()) {
            return Err("Tipografía de letras no válida".into());
        }
        if !is_hex_color(&self.lyrics.color) || !is_hex_color(&self.lyrics.active_color) {
            return Err("Los colores de letras deben usar el formato #RRGGBB".into());
        }
        if !(MIN_LYRIC_OFFSET..=MAX_LYRIC_OFFSET).contains(&self.lyrics.offset_ms) {
            return Err("El desfase de letras está fuera de rango".into());
        }
        if self.artwork.provider_priority.is_empty()
            || self
                .artwork
                .provider_priority
                .iter()
                .any(|value| !ARTWORK_PROVIDERS.contains(&value.as_str()))
        {
            return Err("Prioridad de portadas no válida".into());
        }
        if !ARTWORK_QUALITIES.contains(&self.artwork.quality.as_str()) {
            return Err("Calidad de portadas no válida".into());
        }
        Ok(())
    }

    /// Repairs a loaded document: clamps ranges, drops unknown values and
    /// completes provider lists without losing the user's order.
    pub fn normalized(mut self) -> Self {
        if !LANGUAGES.contains(&self.general.language.as_str()) {
            self.general.language = "es".into();
        }
        if !schema::is_valid_theme(&self.appearance.theme) {
            self.appearance.theme = "light".into();
        }
        if !ACCENTS.contains(&self.appearance.accent.as_str()) {
            self.appearance.accent = "violet".into();
        }
        self.appearance.scale = self.appearance.scale.clamp(MIN_SCALE, MAX_SCALE);
        if !ANIMATIONS.contains(&self.appearance.animations.as_str()) {
            self.appearance.animations = "full".into();
        }
        if !BACKGROUNDS.contains(&self.appearance.background.as_str()) {
            self.appearance.background = "dynamic".into();
        }
        if !PLAYER_LAYOUTS.contains(&self.appearance.player_layout.as_str()) {
            self.appearance.player_layout = "full".into();
        }
        if !LYRICS_STYLES.contains(&self.appearance.lyrics_style.as_str()) {
            self.appearance.lyrics_style = "balanced".into();
        }
        self.playback.initial_volume = self.playback.initial_volume.min(100);
        if !REPEAT_MODES.contains(&self.playback.repeat.as_str()) {
            self.playback.repeat = "off".into();
        }
        self.lyrics.providers = unique_allowed(&self.lyrics.providers, &LYRIC_PROVIDERS);
        if self.lyrics.providers.is_empty() {
            self.lyrics.providers = LyricsSettings::default().providers;
        }
        self.lyrics.size = self.lyrics.size.clamp(MIN_LYRIC_SIZE, MAX_LYRIC_SIZE);
        if !LYRIC_FONTS.contains(&self.lyrics.font.as_str()) {
            self.lyrics.font = "system".into();
        }
        if !is_hex_color(&self.lyrics.color) {
            self.lyrics.color = "#777777".into();
        }
        if !is_hex_color(&self.lyrics.active_color) {
            self.lyrics.active_color = "#8956dd".into();
        }
        self.lyrics.offset_ms = self
            .lyrics
            .offset_ms
            .clamp(MIN_LYRIC_OFFSET, MAX_LYRIC_OFFSET);
        self.artwork.provider_priority =
            unique_allowed(&self.artwork.provider_priority, &ARTWORK_PROVIDERS);
        for provider in ARTWORK_PROVIDERS {
            if !self
                .artwork
                .provider_priority
                .iter()
                .any(|value| value == provider)
            {
                self.artwork.provider_priority.push(provider.into());
            }
        }
        if !ARTWORK_QUALITIES.contains(&self.artwork.quality.as_str()) {
            self.artwork.quality = "preview".into();
        }
        self
    }
}

fn unique_allowed(values: &[String], allowed: &[&str]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for value in values {
        if allowed.contains(&value.as_str()) && !result.iter().any(|item| item == value) {
            result.push(value.clone());
        }
    }
    result
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Loads the document, seeding missing sections from the module stores.
///
/// A missing file is not an error: the settings center shows the effective
/// defaults, which include the theme and lyric/artwork preferences saved by
/// earlier phases. The document is only created on the first save.
pub fn load(storage: &StorageManager) -> Result<SettingsSnapshot, String> {
    match storage
        .load_document::<AppSettings>(SETTINGS_DOCUMENT)
        .map_err(|error| error.to_string())?
    {
        Some(document) => {
            let mut settings = document.data.normalized();
            let current = seed_defaults(storage);
            let newer = |path: &str| {
                storage
                    .load_document::<serde_json::Value>(path)
                    .ok()
                    .flatten()
                    .is_some_and(|module| module.updated_at >= document.updated_at)
            };
            if newer("config/appearance.json") {
                settings.appearance.theme = current.appearance.theme;
            }
            if newer("library/lyrics.json") {
                settings.lyrics.size = current.lyrics.size;
                settings.lyrics.font = current.lyrics.font;
                settings.lyrics.color = current.lyrics.color;
                settings.lyrics.active_color = current.lyrics.active_color;
                settings.lyrics.offset_ms = current.lyrics.offset_ms;
                if !current
                    .lyrics
                    .providers
                    .iter()
                    .any(|provider| provider == "lrclib")
                {
                    settings
                        .lyrics
                        .providers
                        .retain(|provider| provider != "lrclib");
                } else if !settings
                    .lyrics
                    .providers
                    .iter()
                    .any(|provider| provider == "lrclib")
                {
                    settings.lyrics.providers.push("lrclib".into());
                }
            }
            if newer("library/artwork.json") {
                settings.artwork.animate = current.artwork.animate;
                settings.artwork.loop_animation = current.artwork.loop_animation;
                settings.artwork.battery_saver = current.artwork.battery_saver;
                settings.artwork.quality = current.artwork.quality;
                settings.artwork.allow_downloads = current.artwork.allow_downloads;
            }
            Ok(SettingsSnapshot {
                settings: settings.normalized(),
                document: SETTINGS_DOCUMENT.into(),
                stored: true,
            })
        }
        None => Ok(SettingsSnapshot {
            settings: seed_defaults(storage),
            document: SETTINGS_DOCUMENT.into(),
            stored: false,
        }),
    }
}

/// Saves validated settings and mirrors the values owned by other modules.
pub fn save(storage: &StorageManager, settings: AppSettings) -> Result<SettingsSnapshot, String> {
    settings.validate()?;
    let settings = settings.normalized();
    storage
        .save_document(SETTINGS_DOCUMENT, &settings)
        .map_err(|error| error.to_string())?;
    storage
        .save_appearance(&settings.appearance.theme)
        .map_err(|error| error.to_string())?;
    sync_lyrics(storage, &settings.lyrics)?;
    sync_artwork(storage, &settings.artwork)?;
    Ok(SettingsSnapshot {
        settings,
        document: SETTINGS_DOCUMENT.into(),
        stored: true,
    })
}

/// Restores the documented defaults and mirrors them like a normal save.
pub fn reset(storage: &StorageManager) -> Result<SettingsSnapshot, String> {
    save(storage, AppSettings::default())
}

fn seed_defaults(storage: &StorageManager) -> AppSettings {
    let mut settings = AppSettings::default();
    if let Ok(appearance) = storage.load_appearance() {
        settings.appearance.theme = appearance.theme;
    }
    if let Ok(preferences) = lyrics::load_preferences(storage) {
        settings.lyrics.size = u16::try_from(preferences.size).unwrap_or(settings.lyrics.size);
        settings.lyrics.font = preferences.font;
        settings.lyrics.color = preferences.color;
        settings.lyrics.active_color = preferences.active_color;
        settings.lyrics.offset_ms = preferences.offset;
        if !preferences.external {
            settings
                .lyrics
                .providers
                .retain(|provider| provider != "lrclib");
        }
    }
    if let Ok(preferences) = artwork::load_preferences(storage) {
        settings.artwork.animate = preferences.autoplay;
        settings.artwork.loop_animation = preferences.looping;
        settings.artwork.battery_saver = preferences.battery_saver;
        settings.artwork.quality = preferences.quality;
        settings.artwork.allow_downloads = preferences.external;
    }
    settings.normalized()
}

fn sync_lyrics(storage: &StorageManager, settings: &LyricsSettings) -> Result<(), String> {
    let mut preferences = lyrics::load_preferences(storage)?;
    preferences.size = u32::from(settings.size);
    preferences.font = settings.font.clone();
    preferences.color = settings.color.clone();
    preferences.active_color = settings.active_color.clone();
    preferences.offset = settings.offset_ms;
    preferences.external = settings
        .providers
        .iter()
        .any(|provider| provider == "lrclib");
    lyrics::save_preferences(storage, preferences)
}

fn sync_artwork(storage: &StorageManager, settings: &ArtworkSettings) -> Result<(), String> {
    let mut preferences = artwork::load_preferences(storage)?;
    preferences.autoplay = settings.animate;
    preferences.looping = settings.loop_animation;
    preferences.battery_saver = settings.battery_saver;
    preferences.quality = settings.quality.clone();
    preferences.external = settings.allow_downloads;
    artwork::save_preferences(storage, preferences)
}

#[tauri::command]
pub fn settings_get(manager: tauri::State<'_, StorageManager>) -> Result<SettingsSnapshot, String> {
    load(&manager)
}

#[tauri::command]
pub fn settings_save(
    manager: tauri::State<'_, StorageManager>,
    settings: AppSettings,
) -> Result<SettingsSnapshot, String> {
    save(&manager, settings)
}

#[tauri::command]
pub fn settings_reset(
    manager: tauri::State<'_, StorageManager>,
) -> Result<SettingsSnapshot, String> {
    reset(&manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageManager;
    use std::fs;

    fn storage(name: &str) -> (std::path::PathBuf, StorageManager) {
        let root =
            std::env::temp_dir().join(format!("zenith-settings-{name}-{}", std::process::id()));
        let manager = StorageManager::open(root.join("data"), false).unwrap();
        (root, manager)
    }

    #[test]
    fn defaults_are_valid_and_follow_the_documented_values() {
        let settings = AppSettings::default();
        settings.validate().unwrap();
        assert_eq!(settings.general.language, "es");
        assert!(settings.general.confirm_destructive);
        assert_eq!(settings.appearance.scale, 100);
        assert_eq!(settings.playback.initial_volume, 70);
        assert_eq!(settings.lyrics.size, 30);
        assert_eq!(settings.lyrics.providers, ["local", "embedded", "lrclib"]);
        assert_eq!(settings.artwork.provider_priority, ARTWORK_PROVIDERS);
        assert!(!settings.artwork.allow_downloads);
    }

    #[test]
    fn invalid_values_are_rejected_instead_of_silently_saved() {
        let mut settings = AppSettings::default();
        settings.appearance.theme = "neon".into();
        assert!(settings.validate().is_err());
        settings.appearance.theme = "dark".into();
        settings.appearance.scale = 10;
        assert!(settings.validate().is_err());
        settings.appearance.scale = 100;
        settings.lyrics.size = 8;
        assert!(settings.validate().is_err());
        settings.lyrics.size = 30;
        settings.lyrics.color = "red".into();
        assert!(settings.validate().is_err());
        settings.lyrics.color = "#777777".into();
        settings.general.language = "fr".into();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn normalization_repairs_ranges_and_orders() {
        let mut settings = AppSettings::default();
        settings.appearance.scale = 500;
        settings.playback.initial_volume = 250;
        settings.lyrics.size = 2;
        settings.lyrics.offset_ms = 900_000;
        settings.lyrics.providers = vec![
            "lrclib".into(),
            "lrclib".into(),
            "desconocido".into(),
            "local".into(),
        ];
        settings.artwork.provider_priority = vec!["youtube".into(), "youtube".into()];
        settings.artwork.quality = "ultra".into();
        let settings = settings.normalized();
        settings.validate().unwrap();
        assert_eq!(settings.appearance.scale, MAX_SCALE);
        assert_eq!(settings.playback.initial_volume, 100);
        assert_eq!(settings.lyrics.size, MIN_LYRIC_SIZE);
        assert_eq!(settings.lyrics.offset_ms, MAX_LYRIC_OFFSET);
        assert_eq!(settings.lyrics.providers, ["lrclib", "local"]);
        assert_eq!(settings.artwork.provider_priority[0], "youtube");
        assert_eq!(
            settings.artwork.provider_priority.len(),
            ARTWORK_PROVIDERS.len()
        );
        assert_eq!(settings.artwork.quality, "preview");
    }

    #[test]
    fn settings_survive_a_restart_and_update_module_documents() {
        let (root, storage) = storage("roundtrip");
        let mut settings = AppSettings::default();
        settings.appearance.theme = "dark".into();
        settings.appearance.accent = "rose".into();
        settings.lyrics.size = 44;
        settings.lyrics.offset_ms = -250;
        settings
            .lyrics
            .providers
            .retain(|provider| provider != "lrclib");
        settings.artwork.animate = true;
        settings.artwork.quality = "thumbnail".into();
        settings.artwork.allow_downloads = true;

        let saved = save(&storage, settings.clone()).unwrap();
        assert!(saved.stored);
        assert_eq!(saved.settings, settings);
        assert_eq!(load(&storage).unwrap().settings, settings);
        assert_eq!(storage.load_appearance().unwrap().theme, "dark");

        let lyrics = lyrics::load_preferences(&storage).unwrap();
        assert_eq!(lyrics.size, 44);
        assert_eq!(lyrics.offset, -250);
        assert!(!lyrics.external);

        let artwork = artwork::load_preferences(&storage).unwrap();
        assert!(artwork.autoplay);
        assert_eq!(artwork.quality, "thumbnail");
        assert!(artwork.external);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_document_seeds_from_existing_module_preferences() {
        let (root, storage) = storage("seed");
        storage.save_appearance("dark").unwrap();
        let mut lyrics = lyrics::load_preferences(&storage).unwrap();
        lyrics.size = 52;
        lyrics.font = "serif".into();
        lyrics.offset = 1200;
        lyrics.external = true;
        lyrics::save_preferences(&storage, lyrics).unwrap();
        let mut artwork = artwork::load_preferences(&storage).unwrap();
        artwork.quality = "thumbnail".into();
        artwork.autoplay = true;
        artwork::save_preferences(&storage, artwork).unwrap();

        let snapshot = load(&storage).unwrap();
        assert!(!snapshot.stored);
        assert_eq!(snapshot.settings.appearance.theme, "dark");
        assert_eq!(snapshot.settings.lyrics.size, 52);
        assert_eq!(snapshot.settings.lyrics.font, "serif");
        assert_eq!(snapshot.settings.lyrics.offset_ms, 1200);
        assert!(
            snapshot
                .settings
                .lyrics
                .providers
                .iter()
                .any(|p| p == "lrclib")
        );
        assert_eq!(snapshot.settings.artwork.quality, "thumbnail");
        assert!(snapshot.settings.artwork.animate);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn module_preferences_edited_after_a_save_are_not_overwritten_by_stale_values() {
        let (root, storage) = storage("module-edits");
        save(&storage, AppSettings::default()).unwrap();
        let mut preferences = lyrics::load_preferences(&storage).unwrap();
        preferences.size = 48;
        lyrics::save_preferences(&storage, preferences).unwrap();
        storage.save_appearance("dark").unwrap();
        let settings = load(&storage).unwrap().settings;
        assert_eq!(settings.lyrics.size, 48);
        assert_eq!(settings.appearance.theme, "dark");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_fields_are_ignored_when_loading() {
        let (root, storage) = storage("unknown");
        let payload = serde_json::json!({
            "general": { "language": "es", "confirmDestructive": false, "future": 1 },
            "appearance": { "theme": "dark", "accent": "teal", "scale": 110 },
            "playback": { "initialVolume": 55 },
            "lyrics": { "size": 36 },
            "artwork": { "quality": "thumbnail" },
            "unknownSection": { "enabled": true }
        });
        storage.save_document(SETTINGS_DOCUMENT, &payload).unwrap();
        let settings = load(&storage).unwrap().settings;
        assert!(!settings.general.confirm_destructive);
        assert_eq!(settings.appearance.theme, "dark");
        assert_eq!(settings.appearance.accent, "teal");
        assert_eq!(settings.appearance.scale, 110);
        assert_eq!(settings.playback.initial_volume, 55);
        assert_eq!(settings.lyrics.size, 36);
        assert_eq!(settings.artwork.quality, "thumbnail");
        let _ = fs::remove_dir_all(root);
    }
}
