//! Ten-band equalizer and playback processing (roadmap phase 12).
//!
//! Processing happens inside a [`rodio::Source`] wrapper, so it runs on the
//! audio worker thread without blocking the interface. Parameters live in a
//! shared [`ProcessingShared`] value; the source reads a revision counter and
//! updates filter coefficients between samples, which changes the sound
//! without stopping or reloading the track.
//!
//! The engine keeps one JSON document (`config/audio.json`) with the settings
//! and named custom presets. ReplayGain values are read from the file tags;
//! the decoded data is never modified.

use std::{
    f32::consts::PI,
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

use rodio::{ChannelCount, SampleRate, Source, source::SeekError};
use serde::{Deserialize, Serialize};

use crate::storage::StorageManager;

/// Document shared with the audio engine and phase 13.
pub const AUDIO_DOCUMENT: &str = "config/audio.json";
/// Center frequencies of the ten bands, in hertz.
pub const BAND_FREQUENCIES: [f32; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0,
];
/// Minimum gain for a band, in decibels.
pub const MIN_GAIN_DB: f32 = -12.0;
/// Maximum gain for a band, in decibels.
pub const MAX_GAIN_DB: f32 = 12.0;
/// Minimum preamplification, in decibels.
pub const MIN_PREAMP_DB: f32 = -12.0;
/// Maximum preamplification, in decibels.
pub const MAX_PREAMP_DB: f32 = 12.0;
/// Longest transition fade, in milliseconds.
pub const MAX_CROSSFADE_MS: u32 = 12_000;
/// Maximum number of custom presets kept in the document.
pub const MAX_CUSTOM_PRESETS: usize = 40;
/// Ideal Q for octave-spaced peaking filters.
const BAND_Q: f32 = 1.414;
/// Bands above this fraction of the sample rate are bypassed.
const NYQUIST_MARGIN: f32 = 0.45;
/// Peak target used by normalization (about -1 dBFS).
const NORMALIZATION_TARGET_PEAK: f32 = 0.891;
/// Threshold where the clip guard starts compressing.
const CLIP_THRESHOLD: f32 = 0.95;

/// How ReplayGain tags are applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplayGainMode {
    #[default]
    Off,
    Track,
    Album,
}

/// A named user preset. Gains are in decibels and follow [`BAND_FREQUENCIES`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPreset {
    pub name: String,
    pub gains: [f32; 10],
    pub preamp_db: f32,
}

/// Persisted processing preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EqualizerSettings {
    pub enabled: bool,
    /// Built-in preset id, `custom:<name>` or `custom`.
    pub preset: String,
    pub gains: [f32; 10],
    pub preamp_db: f32,
    /// -1.0 is fully left, 0.0 centered and 1.0 fully right.
    pub balance: f32,
    pub replay_gain: ReplayGainMode,
    pub normalization: bool,
    pub crossfade_ms: u32,
    /// Keeps automatic headroom and a soft limiter enabled.
    pub clip_guard: bool,
    pub custom_presets: Vec<CustomPreset>,
}

impl Default for EqualizerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: "flat".to_string(),
            gains: [0.0; 10],
            preamp_db: 0.0,
            balance: 0.0,
            replay_gain: ReplayGainMode::Off,
            normalization: false,
            crossfade_ms: 0,
            clip_guard: true,
            custom_presets: Vec::new(),
        }
    }
}

impl EqualizerSettings {
    /// Clamps every value into the supported range and removes duplicates.
    pub fn normalized(mut self) -> Self {
        for gain in &mut self.gains {
            *gain = clamp_finite(*gain, MIN_GAIN_DB, MAX_GAIN_DB, 0.0);
        }
        self.preamp_db = clamp_finite(self.preamp_db, MIN_PREAMP_DB, MAX_PREAMP_DB, 0.0);
        self.balance = clamp_finite(self.balance, -1.0, 1.0, 0.0);
        self.crossfade_ms = self.crossfade_ms.min(MAX_CROSSFADE_MS);
        self.preset = self.preset.trim().to_string();
        if self.preset.is_empty() {
            self.preset = "flat".to_string();
        }
        if self.preset.len() > 80 {
            self.preset.truncate(80);
        }

        let mut names = Vec::new();
        let mut clean = Vec::new();
        for mut preset in self.custom_presets {
            preset.name = preset.name.trim().to_string();
            if preset.name.is_empty() || preset.name.len() > 60 || names.contains(&preset.name) {
                continue;
            }
            for gain in &mut preset.gains {
                *gain = clamp_finite(*gain, MIN_GAIN_DB, MAX_GAIN_DB, 0.0);
            }
            preset.preamp_db = clamp_finite(preset.preamp_db, MIN_PREAMP_DB, MAX_PREAMP_DB, 0.0);
            names.push(preset.name.clone());
            clean.push(preset);
            if clean.len() == MAX_CUSTOM_PRESETS {
                break;
            }
        }
        self.custom_presets = clean;
        self
    }

    /// Applies a built-in id, `custom:<name>` or `custom`.
    ///
    /// Unknown ids leave the settings untouched and return `false`.
    #[cfg(test)]
    pub fn apply_preset(&mut self, id: &str) -> bool {
        if let Some(preset) = builtin_preset(id) {
            self.gains = preset.gains;
            self.preamp_db = preset.preamp_db;
            self.preset = preset.id.to_string();
            return true;
        }
        if let Some(name) = id.strip_prefix("custom:")
            && let Some(preset) = self
                .custom_presets
                .iter()
                .find(|preset| preset.name == name)
                .cloned()
        {
            self.gains = preset.gains;
            self.preamp_db = preset.preamp_db;
            self.preset = format!("custom:{}", preset.name);
            return true;
        }
        false
    }

    /// Saves the current gains as a named preset, replacing a same-name one.
    #[cfg(test)]
    pub fn save_custom_preset(&mut self, name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Escribe un nombre para el preset.".to_string());
        }
        if name.len() > 60 {
            return Err("El nombre del preset no puede superar 60 caracteres.".to_string());
        }

        let preset = CustomPreset {
            name: name.to_string(),
            gains: self.gains,
            preamp_db: self.preamp_db,
        };
        if let Some(existing) = self
            .custom_presets
            .iter_mut()
            .find(|existing| existing.name == preset.name)
        {
            *existing = preset;
        } else {
            if self.custom_presets.len() >= MAX_CUSTOM_PRESETS {
                return Err(format!(
                    "Solo se pueden guardar {MAX_CUSTOM_PRESETS} presets personalizados."
                ));
            }
            self.custom_presets.push(preset);
        }
        let id = format!("custom:{name}");
        self.preset = id.clone();
        Ok(id)
    }

    /// Removes a custom preset and marks the current preset as unsaved when it
    /// was the selected one.
    #[cfg(test)]
    pub fn remove_custom_preset(&mut self, name: &str) -> bool {
        let before = self.custom_presets.len();
        self.custom_presets.retain(|preset| preset.name != name);
        let removed = self.custom_presets.len() != before;
        if removed && self.preset == format!("custom:{name}") {
            self.preset = "custom".to_string();
        }
        removed
    }
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

/// One of the documented built-in presets.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuiltinPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub gains: [f32; 10],
    pub preamp_db: f32,
}

/// Built-in preset table used by tests and mirrored in the interface table.
#[cfg(test)]
pub const BUILTIN_PRESETS: [BuiltinPreset; 10] = [
    BuiltinPreset {
        id: "flat",
        label: "Plano",
        gains: [0.0; 10],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "rock",
        label: "Rock",
        gains: [5.0, 4.0, 3.0, 1.0, -1.0, -2.0, -1.0, 1.0, 3.0, 4.0],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "pop",
        label: "Pop",
        gains: [-1.0, 0.0, 1.0, 2.0, 3.0, 2.0, 0.0, -1.0, -1.0, -1.0],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "classical",
        label: "Clásica",
        gains: [4.0, 3.0, 2.0, 1.0, 0.0, 0.0, -1.0, -2.0, -3.0, -4.0],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "jazz",
        label: "Jazz",
        gains: [3.0, 2.0, 1.0, 2.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "electronic",
        label: "Electrónica",
        gains: [5.0, 4.0, 1.0, 0.0, -2.0, 2.0, 1.0, 3.0, 4.0, 5.0],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "hip-hop",
        label: "Hip-Hop",
        gains: [6.0, 5.0, 3.0, 1.0, -1.0, -1.0, 1.0, 2.0, 3.0, 4.0],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "bass-boost",
        label: "Graves reforzados",
        gains: [8.0, 7.0, 5.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "vocal",
        label: "Voz",
        gains: [-2.0, -1.0, 1.0, 3.0, 5.0, 5.0, 3.0, 1.0, 0.0, -1.0],
        preamp_db: 0.0,
    },
    BuiltinPreset {
        id: "acoustic",
        label: "Acústica",
        gains: [3.0, 2.0, 1.0, 1.0, 0.0, 1.0, 2.0, 2.0, 1.0, 0.0],
        preamp_db: 0.0,
    },
];

#[cfg(test)]
fn builtin_preset(id: &str) -> Option<BuiltinPreset> {
    BUILTIN_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.id == id)
}

/// ReplayGain values found in the current file.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrackGain {
    pub track_db: Option<f32>,
    pub album_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_peak: Option<f32>,
}

impl TrackGain {
    fn gain_db(self, mode: ReplayGainMode) -> Option<f32> {
        match mode {
            ReplayGainMode::Off => None,
            ReplayGainMode::Track => self.track_db.or(self.album_db),
            ReplayGainMode::Album => self.album_db.or(self.track_db),
        }
    }

    fn peak(self, mode: ReplayGainMode) -> Option<f32> {
        match mode {
            ReplayGainMode::Off => None,
            ReplayGainMode::Track => self.track_peak.or(self.album_peak),
            ReplayGainMode::Album => self.album_peak.or(self.track_peak),
        }
    }
}

/// Reads ReplayGain tags with Lofty; unreadable or untagged files are silent.
pub fn read_track_gain(path: &Path) -> TrackGain {
    let Ok(file) = lofty::read_from_path(path) else {
        return TrackGain::default();
    };

    let mut gain = TrackGain::default();
    for tag in lofty::file::TaggedFileExt::tags(&file) {
        let track_gain = tag_string(tag, lofty::tag::ItemKey::ReplayGainTrackGain);
        let track_peak = tag_string(tag, lofty::tag::ItemKey::ReplayGainTrackPeak);
        let album_gain = tag_string(tag, lofty::tag::ItemKey::ReplayGainAlbumGain);
        let album_peak = tag_string(tag, lofty::tag::ItemKey::ReplayGainAlbumPeak);
        gain.track_db = gain
            .track_db
            .or_else(|| track_gain.as_deref().and_then(parse_gain));
        gain.track_peak = gain
            .track_peak
            .or_else(|| track_peak.as_deref().and_then(parse_peak));
        gain.album_db = gain
            .album_db
            .or_else(|| album_gain.as_deref().and_then(parse_gain));
        gain.album_peak = gain
            .album_peak
            .or_else(|| album_peak.as_deref().and_then(parse_peak));
    }
    gain
}

fn tag_string(tag: &lofty::tag::Tag, key: lofty::tag::ItemKey) -> Option<String> {
    tag.get_string(key).map(ToOwned::to_owned)
}

fn parse_gain(value: &str) -> Option<f32> {
    parse_first_number(value).filter(|gain| gain.is_finite() && (-60.0..=60.0).contains(gain))
}

fn parse_peak(value: &str) -> Option<f32> {
    parse_first_number(value).filter(|peak| peak.is_finite() && *peak > 0.0)
}

fn parse_first_number(value: &str) -> Option<f32> {
    value
        .split_whitespace()
        .next()?
        .replace(',', ".")
        .parse::<f32>()
        .ok()
}

/// Immutable view of the applied settings and their measured context.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingDiagnostics {
    pub sample_rate: Option<u32>,
    /// Which bands fit below Nyquist for the current sample rate.
    pub supported_bands: [bool; 10],
    pub active_bands: usize,
    pub replay_gain_db: f32,
    pub preamp_db: f32,
    pub headroom_db: f32,
    /// Preamplification plus ReplayGain and automatic headroom.
    pub effective_gain_db: f32,
    pub limiter_engaged: bool,
}

impl Default for ProcessingDiagnostics {
    fn default() -> Self {
        Self {
            sample_rate: None,
            supported_bands: supported_bands(None),
            active_bands: 0,
            replay_gain_db: 0.0,
            preamp_db: 0.0,
            headroom_db: 0.0,
            effective_gain_db: 0.0,
            limiter_engaged: false,
        }
    }
}

/// Settings plus their applied diagnostics.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingSnapshot {
    pub settings: EqualizerSettings,
    pub diagnostics: ProcessingDiagnostics,
}

/// State shared by the worker, the source wrapper and the Tauri commands.
pub struct ProcessingShared {
    settings: Mutex<EqualizerSettings>,
    track: Mutex<TrackGain>,
    revision: AtomicU64,
    sample_rate: AtomicU32,
    limiter: AtomicBool,
}

impl ProcessingShared {
    pub fn new(settings: EqualizerSettings) -> Self {
        Self {
            settings: Mutex::new(settings.normalized()),
            track: Mutex::new(TrackGain::default()),
            revision: AtomicU64::new(1),
            sample_rate: AtomicU32::new(0),
            limiter: AtomicBool::new(false),
        }
    }

    pub fn settings(&self) -> EqualizerSettings {
        lock(&self.settings).clone()
    }

    pub fn update(&self, settings: EqualizerSettings) {
        *lock(&self.settings) = settings.normalized();
        self.revision.fetch_add(1, Ordering::AcqRel);
    }

    pub fn set_track_gain(&self, gain: TrackGain) {
        *lock(&self.track) = gain;
        self.revision.fetch_add(1, Ordering::AcqRel);
    }

    pub fn set_sample_rate(&self, sample_rate: Option<u32>) {
        self.sample_rate
            .store(sample_rate.unwrap_or_default(), Ordering::Release);
    }

    pub fn reset_limiter(&self) {
        self.limiter.store(false, Ordering::Release);
    }

    fn mark_limiter(&self) {
        self.limiter.store(true, Ordering::Release);
    }

    pub fn snapshot(&self) -> ProcessingSnapshot {
        let settings = self.settings();
        let track = *lock(&self.track);
        let sample_rate = match self.sample_rate.load(Ordering::Acquire) {
            0 => None,
            value => Some(value),
        };
        let diagnostics = diagnostics(&settings, track, sample_rate);
        ProcessingSnapshot {
            settings,
            diagnostics: ProcessingDiagnostics {
                limiter_engaged: self.limiter.load(Ordering::Acquire),
                ..diagnostics
            },
        }
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
}

/// Computes supported bands, gains and headroom for a sample rate.
pub fn diagnostics(
    settings: &EqualizerSettings,
    track: TrackGain,
    sample_rate: Option<u32>,
) -> ProcessingDiagnostics {
    let supported = supported_bands(sample_rate);
    let replay_gain_db = if settings.enabled {
        track.gain_db(settings.replay_gain).unwrap_or_default()
    } else {
        0.0
    };
    let mut static_gain = settings.preamp_db + replay_gain_db;
    if settings.enabled
        && settings.normalization
        && let Some(peak) = track.peak(settings.replay_gain)
    {
        let allowed = linear_to_db(NORMALIZATION_TARGET_PEAK / peak);
        static_gain = static_gain.min(allowed);
    }
    let highest_boost = if settings.enabled {
        settings
            .gains
            .iter()
            .zip(supported.iter())
            .filter(|(_, supported)| **supported)
            .map(|(gain, _)| *gain)
            .fold(0.0f32, f32::max)
    } else {
        0.0
    };
    let headroom_db = if settings.enabled && settings.clip_guard {
        -(static_gain + highest_boost).max(0.0)
    } else {
        0.0
    };
    let active_bands = if settings.enabled {
        settings
            .gains
            .iter()
            .zip(supported.iter())
            .filter(|(gain, supported)| **supported && gain.abs() > 0.01)
            .count()
    } else {
        0
    };

    ProcessingDiagnostics {
        sample_rate,
        supported_bands: supported,
        active_bands,
        replay_gain_db,
        preamp_db: if settings.enabled {
            settings.preamp_db
        } else {
            0.0
        },
        headroom_db,
        effective_gain_db: if settings.enabled {
            static_gain + headroom_db
        } else {
            0.0
        },
        limiter_engaged: false,
    }
}

fn supported_bands(sample_rate: Option<u32>) -> [bool; 10] {
    let Some(sample_rate) = sample_rate else {
        return [true; 10];
    };
    let limit = sample_rate as f32 * NYQUIST_MARGIN;
    BAND_FREQUENCIES.map(|frequency| frequency < limit)
}

/// Loads settings from `config/audio.json`, falling back to defaults.
pub fn load_settings(storage: &StorageManager) -> EqualizerSettings {
    storage
        .load_document::<EqualizerSettings>(AUDIO_DOCUMENT)
        .ok()
        .flatten()
        .map(|document| document.data.normalized())
        .unwrap_or_default()
}

/// Persists the normalized settings.
pub fn save_settings(storage: &StorageManager, settings: &EqualizerSettings) -> Result<(), String> {
    storage
        .save_document(AUDIO_DOCUMENT, &settings.clone().normalized())
        .map_err(|error| error.to_string())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * linear.log10()
    }
}

/// One peaking biquad with transposed direct form II state.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl Biquad {
    fn set_peaking(&mut self, sample_rate: f32, frequency: f32, gain_db: f32, q: f32) {
        if frequency >= sample_rate * 0.5 || gain_db.abs() < 0.001 || sample_rate <= 0.0 {
            *self = Self {
                z1: self.z1,
                z2: self.z2,
                ..Self::default()
            };
            return;
        }

        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * frequency / sample_rate;
        let cos = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha / a;

        let b0 = (1.0 + alpha * a) / a0;
        let b1 = (-2.0 * cos) / a0;
        let b2 = (1.0 - alpha * a) / a0;
        let a1 = (-2.0 * cos) / a0;
        let a2 = (1.0 - alpha / a) / a0;

        *self = Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: self.z1,
            z2: self.z2,
        };
    }

    fn process(&mut self, sample: f32) -> f32 {
        let output = self.b0 * sample + self.z1;
        self.z1 = self.b1 * sample - self.a1 * output + self.z2;
        self.z2 = self.b2 * sample - self.a2 * output;
        output
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[cfg(test)]
    fn magnitude_at(&self, sample_rate: f32, frequency: f32) -> f32 {
        let w = 2.0 * PI * frequency / sample_rate;
        let (sin1, cos1) = w.sin_cos();
        let (sin2, cos2) = (2.0 * w).sin_cos();
        let numerator = (
            self.b0 + self.b1 * cos1 + self.b2 * cos2,
            -(self.b1 * sin1 + self.b2 * sin2),
        );
        let denominator = (
            1.0 + self.a1 * cos1 + self.a2 * cos2,
            -(self.a1 * sin1 + self.a2 * sin2),
        );
        let num = (numerator.0 * numerator.0 + numerator.1 * numerator.1).sqrt();
        let den = (denominator.0 * denominator.0 + denominator.1 * denominator.1).sqrt();
        if den == 0.0 { 0.0 } else { num / den }
    }
}

#[derive(Clone, Debug)]
struct CachedParams {
    revision: u64,
    enabled: bool,
    supported: [bool; 10],
    gain_linear: f32,
    balance: f32,
    clip_guard: bool,
    crossfade_ms: u32,
    filters: Vec<Biquad>,
}

impl CachedParams {
    fn new(channels: u16) -> Self {
        Self {
            revision: u64::MAX,
            enabled: false,
            supported: [false; 10],
            gain_linear: 1.0,
            balance: 0.0,
            clip_guard: false,
            crossfade_ms: 0,
            filters: vec![Biquad::default(); usize::from(channels) * BAND_FREQUENCIES.len()],
        }
    }
}

/// Wraps a decoded source and applies the shared processing parameters per
/// frame. Changing settings only updates coefficients; playback is not
/// stopped or reloaded.
pub struct ProcessingSource<S> {
    inner: S,
    shared: Arc<ProcessingShared>,
    channels: u16,
    sample_rate: u32,
    total_frames: Option<u64>,
    frame: u64,
    channel: u16,
    cached: CachedParams,
}

impl<S> ProcessingSource<S>
where
    S: Source<Item = f32>,
{
    pub fn new(inner: S, shared: Arc<ProcessingShared>, total: Option<Duration>) -> Self {
        let channels = inner.channels().get();
        let sample_rate = inner.sample_rate().get();
        let total_frames =
            total.map(|duration| (duration.as_secs_f64() * f64::from(sample_rate)).round() as u64);
        let mut source = Self {
            inner,
            shared,
            channels,
            sample_rate,
            total_frames,
            frame: 0,
            channel: 0,
            cached: CachedParams::new(channels),
        };
        let settings = source.shared.settings();
        let track = *lock(&source.shared.track);
        let revision = source.shared.revision();
        source.refresh(&settings, track, revision);
        source
    }

    fn refresh(&mut self, settings: &EqualizerSettings, track: TrackGain, revision: u64) {
        let was_enabled = self.cached.enabled;
        let supported = supported_bands(Some(self.sample_rate));
        let diagnostics = diagnostics(settings, track, Some(self.sample_rate));

        let mut filters = std::mem::take(&mut self.cached.filters);
        filters.resize(
            self.usize_channels() * BAND_FREQUENCIES.len(),
            Biquad::default(),
        );
        if settings.enabled && !was_enabled {
            for filter in &mut filters {
                filter.reset();
            }
        }
        for (index, filter) in filters.iter_mut().enumerate() {
            let band = index % BAND_FREQUENCIES.len();
            let gain = if settings.enabled && supported[band] {
                settings.gains[band]
            } else {
                0.0
            };
            filter.set_peaking(
                self.sample_rate as f32,
                BAND_FREQUENCIES[band],
                gain,
                BAND_Q,
            );
        }

        self.cached = CachedParams {
            revision,
            enabled: settings.enabled,
            supported,
            gain_linear: db_to_linear(diagnostics.effective_gain_db),
            balance: settings.balance,
            clip_guard: settings.clip_guard,
            crossfade_ms: settings.crossfade_ms,
            filters,
        };
    }

    fn usize_channels(&self) -> usize {
        usize::from(self.channels)
    }

    fn maybe_refresh(&mut self) {
        let revision = self.shared.revision();
        if revision != self.cached.revision {
            let settings = self.shared.settings();
            let track = *lock(&self.shared.track);
            self.refresh(&settings, track, revision);
        }
    }

    fn fade_gain(&self, frame: u64) -> f32 {
        if self.cached.crossfade_ms == 0 {
            return 1.0;
        }
        let requested = u64::from(self.cached.crossfade_ms) * u64::from(self.sample_rate) / 1000;
        let fade = match self.total_frames {
            Some(total) => requested.min(total / 2).max(1),
            None => requested,
        };
        if fade == 0 {
            return 1.0;
        }

        let mut gain = 1.0;
        if frame < fade {
            let position = (frame as f32 + 0.5) / fade as f32;
            gain *= (position * PI * 0.5).sin();
        }
        if let Some(total) = self.total_frames {
            let fade_start = total.saturating_sub(fade);
            if frame >= fade_start {
                let position = (frame - fade_start) as f32 / fade as f32;
                gain *= ((1.0 - position) * PI * 0.5).sin();
            }
        }
        gain.clamp(0.0, 1.0)
    }

    fn balance_gain(&self, channel: u16) -> f32 {
        if self.channels < 2 {
            return 1.0;
        }
        let balance = self.cached.balance;
        match channel {
            0 if balance > 0.0 => 1.0 - balance,
            1 if balance < 0.0 => 1.0 + balance,
            _ => 1.0,
        }
    }

    fn process(&mut self, sample: f32) -> f32 {
        self.maybe_refresh();
        let channel = self.channel;
        let frame = self.frame;
        self.channel = if self.channels <= 1 {
            0
        } else {
            (self.channel + 1) % self.channels
        };
        if self.channel == 0 {
            self.frame = self.frame.saturating_add(1);
        }

        if !self.cached.enabled {
            return sample;
        }

        let mut output = sample;
        let offset = usize::from(channel) * BAND_FREQUENCIES.len();
        for band in 0..BAND_FREQUENCIES.len() {
            if self.cached.supported[band] {
                output = self.cached.filters[offset + band].process(output);
            }
        }
        output *= self.cached.gain_linear;
        output *= self.balance_gain(channel);
        output *= self.fade_gain(frame);

        if self.cached.clip_guard {
            if output.abs() > CLIP_THRESHOLD {
                self.shared.mark_limiter();
                output = soft_limit(output);
            }
            output = output.clamp(-1.0, 1.0);
        }
        output
    }
}

impl<S> Iterator for ProcessingSource<S>
where
    S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        Some(self.process(sample))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S> Source for ProcessingSource<S>
where
    S: Source<Item = f32>,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)?;
        self.frame = (pos.as_secs_f64() * f64::from(self.sample_rate)).round() as u64;
        self.channel = 0;
        for filter in &mut self.cached.filters {
            filter.reset();
        }
        Ok(())
    }
}

fn soft_limit(sample: f32) -> f32 {
    let sign = sample.signum();
    let magnitude = sample.abs();
    if magnitude <= CLIP_THRESHOLD {
        return sample;
    }
    let headroom = 1.0 - CLIP_THRESHOLD;
    let compressed = CLIP_THRESHOLD + headroom * ((magnitude - CLIP_THRESHOLD) / headroom).tanh();
    sign * compressed.min(1.0 - f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn builtin_presets_cover_every_documented_preset() {
        assert_eq!(BUILTIN_PRESETS.len(), 10);
        let mut ids = Vec::new();
        for preset in BUILTIN_PRESETS {
            assert_eq!(preset.gains.len(), BAND_FREQUENCIES.len());
            assert!(
                preset
                    .gains
                    .iter()
                    .all(|gain| (MIN_GAIN_DB..=MAX_GAIN_DB).contains(gain))
            );
            assert!(!ids.contains(&preset.id));
            ids.push(preset.id);
        }
    }

    #[test]
    fn settings_are_clamped_and_duplicate_presets_are_removed() {
        let settings = EqualizerSettings {
            gains: [
                f32::NAN,
                20.0,
                -20.0,
                0.0,
                f32::INFINITY,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
            ],
            preamp_db: 100.0,
            balance: -4.0,
            crossfade_ms: 99_000,
            custom_presets: vec![
                CustomPreset {
                    name: "  Voz  ".into(),
                    gains: [0.0; 10],
                    preamp_db: 0.0,
                },
                CustomPreset {
                    name: "Voz".into(),
                    gains: [1.0; 10],
                    preamp_db: 0.0,
                },
            ],
            ..EqualizerSettings::default()
        }
        .normalized();

        assert_eq!(settings.gains[0], 0.0);
        assert_eq!(settings.gains[1], MAX_GAIN_DB);
        assert_eq!(settings.gains[2], MIN_GAIN_DB);
        assert_eq!(settings.gains[4], 0.0);
        assert_eq!(settings.preamp_db, MAX_PREAMP_DB);
        assert_eq!(settings.balance, -1.0);
        assert_eq!(settings.crossfade_ms, MAX_CROSSFADE_MS);
        assert_eq!(settings.custom_presets.len(), 1);
        assert_eq!(settings.custom_presets[0].name, "Voz");
    }

    #[test]
    fn custom_presets_can_be_saved_and_removed() {
        let mut settings = EqualizerSettings::default();
        settings.gains[0] = 4.0;
        let id = settings.save_custom_preset("Coche").unwrap();
        assert_eq!(id, "custom:Coche");
        assert_eq!(settings.preset, id);
        assert_eq!(settings.gains, settings.custom_presets[0].gains);

        settings.gains[0] = -3.0;
        assert!(settings.apply_preset("flat"));
        assert_eq!(settings.gains, [0.0; 10]);
        assert!(settings.apply_preset("custom:Coche"));
        assert_eq!(settings.gains[0], 4.0);

        assert!(settings.remove_custom_preset("Coche"));
        assert!(settings.custom_presets.is_empty());
        assert_eq!(settings.preset, "custom");
    }

    #[test]
    fn unknown_presets_and_blank_names_are_rejected() {
        let mut settings = EqualizerSettings::default();
        assert!(!settings.apply_preset("missing"));
        assert!(settings.save_custom_preset("   ").is_err());
        assert!(settings.save_custom_preset(&"x".repeat(61)).is_err());
    }

    #[test]
    fn band_support_follows_the_sample_rate() {
        assert_eq!(supported_bands(None), [true; 10]);
        assert_eq!(supported_bands(Some(44_100)), [true; 10]);
        let at_32k = supported_bands(Some(32_000));
        assert!(at_32k[8]);
        assert!(!at_32k[9]);
        let at_16k = supported_bands(Some(16_000));
        assert!(at_16k[7]);
        assert!(!at_16k[8]);
        assert!(!at_16k[9]);
    }

    #[test]
    fn headroom_keeps_boosted_output_below_full_scale() {
        let settings = EqualizerSettings {
            enabled: true,
            gains: [12.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            preamp_db: 6.0,
            ..EqualizerSettings::default()
        }
        .normalized();
        let diagnostics = diagnostics(&settings, TrackGain::default(), Some(44_100));
        assert_eq!(diagnostics.headroom_db, -18.0);
        assert_eq!(diagnostics.effective_gain_db, -12.0);
        assert!(diagnostics.effective_gain_db + 12.0 <= 0.0);
    }

    #[test]
    fn replay_gain_and_normalization_limit_the_static_gain() {
        let settings = EqualizerSettings {
            enabled: true,
            replay_gain: ReplayGainMode::Track,
            normalization: true,
            ..EqualizerSettings::default()
        };
        let track = TrackGain {
            track_db: Some(9.0),
            track_peak: Some(0.99),
            ..TrackGain::default()
        };
        let diagnostics = diagnostics(&settings, track, Some(44_100));
        assert_eq!(diagnostics.replay_gain_db, 9.0);
        assert!(diagnostics.effective_gain_db < 0.0);
        assert!(
            db_to_linear(diagnostics.effective_gain_db) * track.track_peak.unwrap()
                <= NORMALIZATION_TARGET_PEAK
        );
    }

    #[test]
    fn peaking_filter_boosts_its_center_frequency() {
        let mut filter = Biquad::default();
        filter.set_peaking(44_100.0, 1_000.0, 6.0, BAND_Q);
        let boost = filter.magnitude_at(44_100.0, 1_000.0);
        assert!((boost - 1.995).abs() < 0.03, "unexpected boost: {boost}");
        let untouched = filter.magnitude_at(44_100.0, 31.0);
        assert!(
            (untouched - 1.0).abs() < 0.05,
            "unexpected low band: {untouched}"
        );
    }

    #[test]
    fn soft_limiter_never_exceeds_full_scale() {
        for sample in [-8.0, -2.0, -0.9, 0.0, 0.9, 2.0, 8.0] {
            let limited = soft_limit(sample);
            assert!(limited.abs() < 1.0);
            assert!(limited.abs() >= sample.abs().min(CLIP_THRESHOLD));
        }
        assert_eq!(soft_limit(0.4), 0.4);
    }

    #[test]
    fn duration_fades_are_bounded_for_short_tracks() {
        let settings = EqualizerSettings {
            crossfade_ms: 8_000,
            ..EqualizerSettings::default()
        };
        let source = ProcessingSource::new(
            rodio::source::SineWave::new(440.0),
            Arc::new(ProcessingShared::new(settings)),
            Some(Duration::from_millis(500)),
        );
        let fade = u64::from(source.cached.crossfade_ms) * u64::from(source.sample_rate) / 1000;
        let bounded = fade.min(source.total_frames.unwrap() / 2);
        assert!(bounded > 0);
        assert!(bounded <= source.total_frames.unwrap() / 2);
    }

    #[test]
    fn stereo_fades_use_frames_and_keep_channels_aligned() {
        let settings = EqualizerSettings {
            enabled: true,
            crossfade_ms: 10,
            ..EqualizerSettings::default()
        };
        let buffer = rodio::buffer::SamplesBuffer::new(
            std::num::NonZeroU16::new(2).unwrap(),
            std::num::NonZeroU32::new(48_000).unwrap(),
            vec![0.25; 1920],
        );
        let mut source = ProcessingSource::new(
            buffer,
            Arc::new(ProcessingShared::new(settings)),
            Some(Duration::from_millis(20)),
        );
        for _ in 0..960 {
            let left = source.next().unwrap();
            let right = source.next().unwrap();
            assert_eq!(left, right);
            assert!(left >= 0.0);
        }
        assert_eq!(source.frame, 960);
    }

    #[test]
    fn settings_survive_storage_and_preserve_custom_presets() {
        let root = std::env::temp_dir().join(format!(
            "zenith-eq-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = StorageManager::open(root.join("data"), false).unwrap();
        let mut settings = EqualizerSettings {
            enabled: true,
            gains: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            ..EqualizerSettings::default()
        };
        settings.save_custom_preset("Estudio").unwrap();
        save_settings(&storage, &settings).unwrap();

        let loaded = load_settings(&storage);
        assert_eq!(loaded, settings);
        assert_eq!(loaded.custom_presets[0].name, "Estudio");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gain_and_peak_parsers_accept_common_replaygain_values() {
        assert_eq!(parse_gain("-7.25 dB"), Some(-7.25));
        assert_eq!(parse_gain("+2,5 dB"), Some(2.5));
        assert_eq!(parse_gain("not-a-number"), None);
        assert_eq!(parse_peak("0.987654"), Some(0.987654));
        assert_eq!(parse_peak("0"), None);
    }

    #[test]
    #[ignore = "manual processing-load measurement"]
    fn processing_throughput_reports_real_time_factor() {
        let settings = EqualizerSettings {
            enabled: true,
            gains: [6.0; 10],
            ..EqualizerSettings::default()
        };
        let shared = Arc::new(ProcessingShared::new(settings));
        let total = Duration::from_secs(5);
        let mut source = ProcessingSource::new(
            rodio::source::SineWave::new(440.0).take_duration(total),
            shared,
            Some(total),
        );

        let started = std::time::Instant::now();
        let mut samples = 0_u64;
        while source.next().is_some() {
            samples += 1;
        }
        let elapsed = started.elapsed().as_secs_f64();
        let samples_per_second = samples as f64 / elapsed;
        let real_time_factor = samples_per_second / f64::from(source.sample_rate);
        println!(
            "processed {samples} samples in {elapsed:.3} s ({samples_per_second:.0} samples/s, {real_time_factor:.1}x real time)"
        );
        assert!(
            real_time_factor > 5.0,
            "processing is too slow: {real_time_factor:.1}x real time"
        );
    }
}
