//! Offline audio playback for local files.
//!
//! Rodio owns decoding (through Symphonia) and output (through CPAL). The
//! output stream is not `Send` on every platform, so it lives on a dedicated
//! worker thread. Tauri commands only send messages to that thread and read an
//! immutable snapshot of the playback state.

use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

const STATE_EVENT: &str = "audio:state";
const POSITION_EVENT: &str = "audio:position";
const TICK: Duration = Duration::from_millis(200);
const STALL_TICKS: u32 = 20;
const MAX_RECOVERIES: u8 = 2;
const PREVIOUS_RESTART_AFTER: Duration = Duration::from_secs(3);
const DEFAULT_VOLUME: f32 = 0.7;

/// Extensions handed to the decoder. Opus is intentionally absent: the selected
/// Symphonia version has no Opus decoder.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "wave", "ogg", "oga", "aiff", "aif", "aifc", "m4a", "mp4", "aac", "alac",
    "caf",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackStatus {
    Idle,
    Loading,
    Playing,
    Paused,
    Stopped,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackSummary {
    pub path: String,
    pub title: String,
    pub format: String,
    pub duration_ms: Option<u64>,
}

impl TrackSummary {
    fn from_path(path: &Path) -> Self {
        let title = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty())
            .unwrap_or("Pista sin título")
            .to_string();

        Self {
            path: path.to_string_lossy().into_owned(),
            title,
            format: format_label(path),
            duration_ms: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSnapshot {
    pub status: PlaybackStatus,
    pub track: Option<TrackSummary>,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub format: Option<String>,
    pub volume: f32,
    pub muted: bool,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub queue: Vec<TrackSummary>,
    pub current_index: Option<usize>,
    pub error: Option<String>,
    pub output_sample_rate: Option<u32>,
    pub output_channels: Option<u16>,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Idle,
            track: None,
            position_ms: 0,
            duration_ms: None,
            sample_rate: None,
            channels: None,
            format: None,
            volume: DEFAULT_VOLUME,
            muted: false,
            repeat: RepeatMode::Off,
            shuffle: false,
            queue: Vec::new(),
            current_index: None,
            error: None,
            output_sample_rate: None,
            output_channels: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionEvent {
    position_ms: u64,
    duration_ms: Option<u64>,
    status: PlaybackStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioCapabilities {
    decoder: &'static str,
    output: &'static str,
    formats: &'static [&'static str],
    unsupported: &'static [&'static str],
}

enum AudioCommand {
    SetQueue { paths: Vec<String>, play: bool },
    PlayIndex(usize),
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    Seek(u64),
    SetVolume(f32),
    SetMuted(bool),
    SetRepeat(RepeatMode),
    SetShuffle(bool),
    RestartOutput,
    ClearQueue,
}

/// Handle stored in Tauri state. All audio work happens on the worker thread.
pub struct AudioEngine {
    commands: Mutex<Sender<AudioCommand>>,
    snapshot: Arc<Mutex<PlaybackSnapshot>>,
}

impl AudioEngine {
    pub fn new(app: AppHandle) -> Self {
        let (sender, receiver) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(PlaybackSnapshot::default()));
        let worker = AudioWorker::new(app, Arc::clone(&snapshot), receiver);

        if let Err(error) = thread::Builder::new()
            .name("zenith-audio".into())
            .spawn(move || worker.run())
        {
            eprintln!("no se pudo iniciar el hilo de audio: {error}");
        }

        Self {
            commands: Mutex::new(sender),
            snapshot,
        }
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default()
    }

    pub fn open_files(&self, paths: Vec<String>, play: bool) -> Result<(), String> {
        let paths: Vec<String> = paths
            .into_iter()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .collect();

        if paths.is_empty() {
            return Err("Selecciona al menos un archivo de audio.".to_string());
        }

        self.send(AudioCommand::SetQueue { paths, play })
    }

    pub fn play_index(&self, index: usize) -> Result<(), String> {
        self.send(AudioCommand::PlayIndex(index))
    }

    pub fn play(&self) -> Result<(), String> {
        self.send(AudioCommand::Play)
    }

    pub fn pause(&self) -> Result<(), String> {
        self.send(AudioCommand::Pause)
    }

    pub fn toggle(&self) -> Result<(), String> {
        self.send(AudioCommand::Toggle)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.send(AudioCommand::Stop)
    }

    pub fn next(&self) -> Result<(), String> {
        self.send(AudioCommand::Next)
    }

    pub fn previous(&self) -> Result<(), String> {
        self.send(AudioCommand::Previous)
    }

    pub fn seek(&self, position_ms: u64) -> Result<(), String> {
        self.send(AudioCommand::Seek(position_ms))
    }

    pub fn set_volume(&self, volume: f32) -> Result<(), String> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err("El volumen debe estar entre 0 y 1.".to_string());
        }

        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.volume = volume;
        }

        self.send(AudioCommand::SetVolume(volume))
    }

    pub fn set_muted(&self, muted: bool) -> Result<(), String> {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.muted = muted;
        }

        self.send(AudioCommand::SetMuted(muted))
    }

    pub fn set_repeat(&self, mode: RepeatMode) -> Result<(), String> {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.repeat = mode;
        }

        self.send(AudioCommand::SetRepeat(mode))
    }

    pub fn set_shuffle(&self, shuffle: bool) -> Result<(), String> {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.shuffle = shuffle;
        }

        self.send(AudioCommand::SetShuffle(shuffle))
    }

    pub fn restart_output(&self) -> Result<(), String> {
        self.send(AudioCommand::RestartOutput)
    }

    pub fn clear_queue(&self) -> Result<(), String> {
        self.send(AudioCommand::ClearQueue)
    }

    fn send(&self, command: AudioCommand) -> Result<(), String> {
        self.commands
            .lock()
            .map_err(|_| "El motor de audio no está disponible.".to_string())?
            .send(command)
            .map_err(|_| "El motor de audio se detuvo inesperadamente.".to_string())
    }
}

struct AudioWorker {
    app: AppHandle,
    shared: Arc<Mutex<PlaybackSnapshot>>,
    commands: Receiver<AudioCommand>,
    sink: Option<MixerDeviceSink>,
    player: Option<Player>,
    queue: Vec<TrackSummary>,
    current: Option<usize>,
    status: PlaybackStatus,
    error: Option<String>,
    volume: f32,
    muted: bool,
    repeat: RepeatMode,
    shuffle: bool,
    position: Duration,
    duration: Option<Duration>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    format: Option<String>,
    output_sample_rate: Option<u32>,
    output_channels: Option<u16>,
    last_position: Duration,
    stalled_ticks: u32,
    recoveries: u8,
    listened: Duration,
    listen_position: Duration,
    counted: bool,
}

impl AudioWorker {
    fn new(
        app: AppHandle,
        shared: Arc<Mutex<PlaybackSnapshot>>,
        commands: Receiver<AudioCommand>,
    ) -> Self {
        Self {
            app,
            shared,
            commands,
            sink: None,
            player: None,
            queue: Vec::new(),
            current: None,
            status: PlaybackStatus::Idle,
            error: None,
            volume: DEFAULT_VOLUME,
            muted: false,
            repeat: RepeatMode::Off,
            shuffle: false,
            position: Duration::ZERO,
            duration: None,
            sample_rate: None,
            channels: None,
            format: None,
            output_sample_rate: None,
            output_channels: None,
            last_position: Duration::ZERO,
            stalled_ticks: 0,
            recoveries: 0,
            listened: Duration::ZERO,
            listen_position: Duration::ZERO,
            counted: false,
        }
    }

    fn run(mut self) {
        if let Err(error) = self.open_output() {
            self.error = Some(error);
            self.status = PlaybackStatus::Error;
        }
        self.publish();

        loop {
            match self.commands.recv_timeout(TICK) {
                Err(RecvTimeoutError::Disconnected) => break,
                Ok(command) => self.handle(command),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.tick();
        }
    }

    fn handle(&mut self, command: AudioCommand) {
        match command {
            AudioCommand::SetQueue { paths, play } => self.set_queue(paths, play),
            AudioCommand::PlayIndex(index) => self.start_index(index, true, None),
            AudioCommand::Play => self.resume(),
            AudioCommand::Pause => self.pause(),
            AudioCommand::Toggle => match self.status {
                PlaybackStatus::Playing => self.pause(),
                _ => self.resume(),
            },
            AudioCommand::Stop => self.stop(),
            AudioCommand::Next => self.next_manual(),
            AudioCommand::Previous => self.previous_manual(),
            AudioCommand::Seek(position_ms) => self.seek(Duration::from_millis(position_ms)),
            AudioCommand::SetVolume(volume) => {
                self.volume = volume;
                self.apply_volume();
                self.publish();
            }
            AudioCommand::SetMuted(muted) => {
                self.muted = muted;
                self.apply_volume();
                self.publish();
            }
            AudioCommand::SetRepeat(mode) => {
                self.repeat = mode;
                self.publish();
            }
            AudioCommand::SetShuffle(shuffle) => {
                self.shuffle = shuffle;
                self.publish();
            }
            AudioCommand::RestartOutput => self.restart_output(),
            AudioCommand::ClearQueue => self.clear_queue(),
        }
    }

    fn open_output(&mut self) -> Result<(), String> {
        let mut sink = DeviceSinkBuilder::open_default_sink()
            .map_err(|error| format!("No se pudo abrir la salida de audio: {error}"))?;
        sink.log_on_drop(false);

        let player = Player::connect_new(sink.mixer());
        player.set_volume(self.effective_volume());

        let config = sink.config();
        self.output_sample_rate = Some(config.sample_rate().get());
        self.output_channels = Some(config.channel_count().get());
        self.sink = Some(sink);
        self.player = Some(player);

        Ok(())
    }

    fn set_queue(&mut self, paths: Vec<String>, play: bool) {
        self.queue = paths
            .iter()
            .map(|path| TrackSummary::from_path(Path::new(path)))
            .collect();
        if let Ok(library) = crate::library::execute(
            &self.app.state::<crate::storage::StorageManager>(),
            crate::library::LibraryAction::Snapshot,
        ) {
            for track in &mut self.queue {
                if let Some(entry) = library
                    .tracks
                    .iter()
                    .find(|t| t.local_path.as_deref() == Some(&track.path))
                {
                    track.title = entry.title.clone();
                }
            }
        }
        self.current = None;
        self.clear_player();
        self.position = Duration::ZERO;
        self.duration = None;
        self.sample_rate = None;
        self.channels = None;
        self.format = None;
        self.error = None;
        self.recoveries = 0;

        if play && !self.queue.is_empty() {
            self.start_index(0, true, None);
        } else {
            self.status = PlaybackStatus::Idle;
            self.publish();
        }
    }

    fn clear_queue(&mut self) {
        self.queue.clear();
        self.current = None;
        self.clear_player();
        self.position = Duration::ZERO;
        self.duration = None;
        self.sample_rate = None;
        self.channels = None;
        self.format = None;
        self.error = None;
        self.status = PlaybackStatus::Idle;
        self.publish();
    }

    fn clear_player(&mut self) {
        if let Some(player) = &self.player {
            player.stop();
        }
        self.last_position = Duration::ZERO;
        self.stalled_ticks = 0;
    }

    fn start_index(&mut self, index: usize, autoplay: bool, start_at: Option<Duration>) {
        if index >= self.queue.len() {
            self.set_error("La pista seleccionada no existe en la cola.");
            return;
        }

        if start_at.is_none() {
            self.listened = Duration::ZERO;
            self.listen_position = Duration::ZERO;
            self.counted = false;
            self.recoveries = 0;
        }

        self.current = Some(index);
        self.status = PlaybackStatus::Loading;
        self.error = None;
        self.position = Duration::ZERO;
        self.duration = None;
        self.sample_rate = None;
        self.channels = None;
        self.format = Some(format_label(Path::new(&self.queue[index].path)));
        self.publish();

        if let Err(error) = self.load(index, autoplay, start_at) {
            self.set_error(error);
        }
    }

    fn load(
        &mut self,
        index: usize,
        autoplay: bool,
        start_at: Option<Duration>,
    ) -> Result<(), String> {
        let path = self
            .queue
            .get(index)
            .ok_or_else(|| "La pista seleccionada no existe en la cola.".to_string())?
            .path
            .clone();

        let player = self
            .player
            .as_ref()
            .ok_or_else(|| "No hay una salida de audio disponible.".to_string())?;

        player.stop();
        let (decoder, info) = decode_file(Path::new(&path))?;
        player.set_volume(self.effective_volume());
        player.append(decoder);

        let position = start_at.unwrap_or(Duration::ZERO);
        if position > Duration::ZERO {
            let _ = player.try_seek(position);
        }

        if autoplay {
            player.play();
        } else {
            player.pause();
        }

        self.current = Some(index);
        self.position = position;
        self.duration = info.duration;
        self.sample_rate = Some(info.sample_rate);
        self.channels = Some(info.channels);
        self.format = Some(format_label(Path::new(&path)));
        self.last_position = position;
        self.listen_position = position;
        self.stalled_ticks = 0;
        self.status = if autoplay {
            PlaybackStatus::Playing
        } else {
            PlaybackStatus::Paused
        };

        if let Some(track) = self.queue.get_mut(index) {
            track.duration_ms = info.duration.map(duration_ms);
        }

        self.publish();
        Ok(())
    }

    fn resume(&mut self) {
        match self.status {
            PlaybackStatus::Playing => {}
            PlaybackStatus::Paused => {
                if let Some(player) = &self.player {
                    player.play();
                }
                self.status = PlaybackStatus::Playing;
                self.publish();
            }
            PlaybackStatus::Loading => {}
            PlaybackStatus::Error if self.player.is_none() => self.restart_output(),
            PlaybackStatus::Stopped | PlaybackStatus::Idle | PlaybackStatus::Error => {
                if let Some(index) = self.current {
                    self.start_index(index, true, None);
                } else if !self.queue.is_empty() {
                    self.start_index(0, true, None);
                } else {
                    self.status = PlaybackStatus::Idle;
                    self.publish();
                }
            }
        }
    }

    fn pause(&mut self) {
        if self.status == PlaybackStatus::Playing {
            if let Some(player) = &self.player {
                player.pause();
            }
            self.status = PlaybackStatus::Paused;
            self.publish();
        }
    }

    fn stop(&mut self) {
        if let Some(player) = &self.player {
            player.stop();
        }
        self.status = PlaybackStatus::Stopped;
        self.position = Duration::ZERO;
        self.last_position = Duration::ZERO;
        self.stalled_ticks = 0;
        self.publish();
    }

    fn seek(&mut self, position: Duration) {
        if !matches!(
            self.status,
            PlaybackStatus::Playing | PlaybackStatus::Paused
        ) {
            return;
        }

        let Some(player) = &self.player else {
            return;
        };

        let target = self
            .duration
            .map_or(position, |duration| position.min(duration));

        match player.try_seek(target) {
            Ok(()) => {
                self.position = target;
                self.last_position = target;
                self.listen_position = target;
                self.stalled_ticks = 0;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("No se pudo buscar la posición: {error}"));
            }
        }

        self.publish();
    }

    fn next_manual(&mut self) {
        let Some(current) = self.current else {
            if !self.queue.is_empty() {
                self.start_index(0, true, None);
            }
            return;
        };

        match step_index(
            self.queue.len(),
            current,
            self.repeat,
            self.shuffle,
            true,
            random_seed(),
        ) {
            Some(index) => self.start_index(index, true, None),
            None => self.stop(),
        }
    }

    fn previous_manual(&mut self) {
        if self.position > PREVIOUS_RESTART_AFTER {
            self.seek(Duration::ZERO);
            return;
        }

        let Some(current) = self.current else {
            if !self.queue.is_empty() {
                self.start_index(0, true, None);
            }
            return;
        };

        match step_index(
            self.queue.len(),
            current,
            self.repeat,
            self.shuffle,
            false,
            random_seed(),
        ) {
            Some(index) => self.start_index(index, true, None),
            None => self.seek(Duration::ZERO),
        }
    }

    fn advance_after_end(&mut self) {
        let Some(current) = self.current else {
            self.stop();
            return;
        };

        if self.repeat == RepeatMode::One {
            self.start_index(current, true, None);
            return;
        }

        match step_index(
            self.queue.len(),
            current,
            self.repeat,
            self.shuffle,
            true,
            random_seed(),
        ) {
            Some(index) => self.start_index(index, true, None),
            None => {
                self.status = PlaybackStatus::Stopped;
                self.position = self.duration.unwrap_or_default();
                self.last_position = self.position;
                self.stalled_ticks = 0;
                self.publish();
            }
        }
    }

    fn tick(&mut self) {
        if !matches!(
            self.status,
            PlaybackStatus::Playing | PlaybackStatus::Paused
        ) {
            return;
        }

        let (position, empty, paused) = match &self.player {
            Some(player) => (player.get_pos(), player.empty(), player.is_paused()),
            None => return,
        };

        self.position = position;
        let delta = position.saturating_sub(self.listen_position);
        self.listen_position = position;
        if self.status == PlaybackStatus::Playing && !paused && delta <= Duration::from_secs(1) {
            self.listened += delta;
            if !self.counted && self.listened >= Duration::from_secs(30) {
                self.counted = true;
                if let Some(track) = self.current.and_then(|i| self.queue.get(i)) {
                    let path = track.path.clone();
                    let app = self.app.clone();
                    // Never make JSON persistence or a scan block audio output.
                    tauri::async_runtime::spawn_blocking(move || {
                        if crate::library::record_play(
                            &app.state::<crate::storage::StorageManager>(),
                            &path,
                        )
                        .is_ok()
                        {
                            let _ = app.emit("library:changed", ());
                        }
                    });
                }
            }
        }

        if self.status != PlaybackStatus::Playing {
            return;
        }

        if empty {
            self.advance_after_end();
            return;
        }

        self.detect_stall(position, paused);

        let payload = PositionEvent {
            position_ms: duration_ms(position),
            duration_ms: self.duration.map(duration_ms),
            status: self.status.clone(),
        };
        let _ = self.app.emit(POSITION_EVENT, payload);
    }

    fn detect_stall(&mut self, position: Duration, paused: bool) {
        if paused {
            return;
        }

        if position != self.last_position {
            self.last_position = position;
            self.stalled_ticks = 0;
            self.recoveries = 0;
            return;
        }

        self.stalled_ticks += 1;
        if self.stalled_ticks < STALL_TICKS {
            return;
        }

        self.stalled_ticks = 0;
        if self.recoveries >= MAX_RECOVERIES {
            self.set_error(
                "La salida de audio dejó de responder. Reinicia la salida para continuar.",
            );
            return;
        }

        self.recoveries += 1;
        self.restart_output();
    }

    fn restart_output(&mut self) {
        let resume_at = self.position;
        let index = self.current;
        let was_playing = self.status == PlaybackStatus::Playing;

        self.player = None;
        self.sink = None;
        self.output_sample_rate = None;
        self.output_channels = None;

        if let Err(error) = self.open_output() {
            self.set_error(error);
            return;
        }

        match index {
            Some(index) if index < self.queue.len() => {
                if let Err(error) = self.load(index, was_playing, Some(resume_at)) {
                    self.set_error(error);
                }
            }
            _ => {
                self.status = PlaybackStatus::Idle;
                self.publish();
            }
        }
    }

    fn apply_volume(&self) {
        if let Some(player) = &self.player {
            player.set_volume(self.effective_volume());
        }
    }

    fn effective_volume(&self) -> f32 {
        effective_volume(self.volume, self.muted)
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.status = PlaybackStatus::Error;
        self.publish();
    }

    fn publish(&self) {
        let snapshot = PlaybackSnapshot {
            status: self.status.clone(),
            track: self
                .current
                .and_then(|index| self.queue.get(index).cloned()),
            position_ms: duration_ms(self.position),
            duration_ms: self.duration.map(duration_ms),
            sample_rate: self.sample_rate,
            channels: self.channels,
            format: self.format.clone(),
            volume: self.volume,
            muted: self.muted,
            repeat: self.repeat,
            shuffle: self.shuffle,
            queue: self.queue.clone(),
            current_index: self.current,
            error: self.error.clone(),
            output_sample_rate: self.output_sample_rate,
            output_channels: self.output_channels,
        };

        if let Ok(mut shared) = self.shared.lock() {
            *shared = snapshot.clone();
        }

        let _ = self.app.emit(STATE_EVENT, snapshot);
    }
}

struct DecodedInfo {
    duration: Option<Duration>,
    sample_rate: u32,
    channels: u16,
}

fn decode_file(path: &Path) -> Result<(Decoder<BufReader<File>>, DecodedInfo), String> {
    let file = File::open(path)
        .map_err(|error| format!("No se pudo abrir «{}»: {error}", path.display()))?;
    let byte_len = file
        .metadata()
        .map_err(|error| format!("No se pudo leer «{}»: {error}", path.display()))?
        .len();

    if byte_len == 0 {
        return Err(format!("El archivo «{}» está vacío.", path.display()));
    }

    let mut builder = Decoder::builder()
        .with_data(BufReader::new(file))
        .with_byte_len(byte_len)
        .with_seekable(true)
        .with_gapless(true);

    if let Some(hint) = format_hint(path) {
        builder = builder.with_hint(&hint);
    }

    let decoder = builder
        .build()
        .map_err(|error| format!("No se pudo decodificar «{}»: {error}", path.display()))?;

    let info = DecodedInfo {
        duration: decoder.total_duration(),
        sample_rate: decoder.sample_rate().get(),
        channels: decoder.channels().get(),
    };

    Ok((decoder, info))
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn format_hint(path: &Path) -> Option<String> {
    extension(path).filter(|extension| SUPPORTED_EXTENSIONS.contains(&extension.as_str()))
}

fn format_label(path: &Path) -> String {
    match extension(path).as_deref() {
        Some("mp3") => "MP3",
        Some("flac") => "FLAC",
        Some("wav" | "wave") => "WAV",
        Some("ogg" | "oga") => "OGG Vorbis",
        Some("opus") => "Opus",
        Some("aiff" | "aif" | "aifc") => "AIFF",
        Some("m4a" | "mp4" | "aac") => "M4A/AAC",
        Some("alac") => "ALAC",
        Some("caf") => "CAF",
        _ => "Desconocido",
    }
    .to_string()
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn effective_volume(volume: f32, muted: bool) -> f32 {
    if muted { 0.0 } else { volume }
}

fn random_seed() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| usize::try_from(elapsed.as_nanos()).ok())
        .unwrap_or_else(|| usize::try_from(rand::random::<u64>()).unwrap_or(usize::MAX))
}

fn step_index(
    len: usize,
    current: usize,
    repeat: RepeatMode,
    shuffle: bool,
    forward: bool,
    random: usize,
) -> Option<usize> {
    if len == 0 || current >= len {
        return None;
    }

    if shuffle && len > 1 {
        let offset = random % (len - 1);
        return Some(if offset >= current {
            offset + 1
        } else {
            offset
        });
    }

    if forward {
        if current + 1 < len {
            Some(current + 1)
        } else if repeat == RepeatMode::All {
            Some(0)
        } else {
            None
        }
    } else if current > 0 {
        Some(current - 1)
    } else if repeat == RepeatMode::All {
        Some(len - 1)
    } else {
        None
    }
}

#[tauri::command]
pub fn audio_get_state(engine: State<'_, AudioEngine>) -> PlaybackSnapshot {
    engine.snapshot()
}

#[tauri::command]
pub fn audio_open_files(
    engine: State<'_, AudioEngine>,
    paths: Vec<String>,
    play: bool,
) -> Result<(), String> {
    engine.open_files(paths, play)
}

#[tauri::command]
pub fn audio_play_index(engine: State<'_, AudioEngine>, index: usize) -> Result<(), String> {
    engine.play_index(index)
}

#[tauri::command]
pub fn audio_play(engine: State<'_, AudioEngine>) -> Result<(), String> {
    engine.play()
}

#[tauri::command]
pub fn audio_pause(engine: State<'_, AudioEngine>) -> Result<(), String> {
    engine.pause()
}

#[tauri::command]
pub fn audio_toggle(engine: State<'_, AudioEngine>) -> Result<(), String> {
    engine.toggle()
}

#[tauri::command]
pub fn audio_stop(engine: State<'_, AudioEngine>) -> Result<(), String> {
    engine.stop()
}

#[tauri::command]
pub fn audio_next(engine: State<'_, AudioEngine>) -> Result<(), String> {
    engine.next()
}

#[tauri::command]
pub fn audio_previous(engine: State<'_, AudioEngine>) -> Result<(), String> {
    engine.previous()
}

#[tauri::command]
pub fn audio_seek(engine: State<'_, AudioEngine>, position_ms: u64) -> Result<(), String> {
    engine.seek(position_ms)
}

#[tauri::command]
pub fn audio_set_volume(engine: State<'_, AudioEngine>, volume: f32) -> Result<(), String> {
    engine.set_volume(volume)
}

#[tauri::command]
pub fn audio_set_muted(engine: State<'_, AudioEngine>, muted: bool) -> Result<(), String> {
    engine.set_muted(muted)
}

#[tauri::command]
pub fn audio_set_repeat(engine: State<'_, AudioEngine>, mode: RepeatMode) -> Result<(), String> {
    engine.set_repeat(mode)
}

#[tauri::command]
pub fn audio_set_shuffle(engine: State<'_, AudioEngine>, shuffle: bool) -> Result<(), String> {
    engine.set_shuffle(shuffle)
}

#[tauri::command]
pub fn audio_restart_output(engine: State<'_, AudioEngine>) -> Result<(), String> {
    engine.restart_output()
}

#[tauri::command]
pub fn audio_clear_queue(engine: State<'_, AudioEngine>) -> Result<(), String> {
    engine.clear_queue()
}

#[tauri::command]
pub fn audio_capabilities() -> AudioCapabilities {
    AudioCapabilities {
        decoder: "Symphonia (a través de Rodio 0.23)",
        output: "CPAL",
        formats: SUPPORTED_EXTENSIONS,
        unsupported: &["opus"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_known_formats() {
        assert_eq!(format_label(Path::new("song.mp3")), "MP3");
        assert_eq!(format_label(Path::new("song.FLAC")), "FLAC");
        assert_eq!(format_label(Path::new("song.wave")), "WAV");
        assert_eq!(format_label(Path::new("song.ogg")), "OGG Vorbis");
        assert_eq!(format_label(Path::new("song.m4a")), "M4A/AAC");
        assert_eq!(format_label(Path::new("song.unknown")), "Desconocido");
    }

    #[test]
    fn track_summary_uses_the_file_stem() {
        let track = TrackSummary::from_path(Path::new("/music/01 - Song.flac"));
        assert_eq!(track.title, "01 - Song");
        assert_eq!(track.format, "FLAC");
        assert_eq!(track.duration_ms, None);
    }

    #[test]
    fn step_index_respects_repeat_mode() {
        assert_eq!(step_index(3, 1, RepeatMode::Off, false, true, 0), Some(2));
        assert_eq!(step_index(3, 2, RepeatMode::Off, false, true, 0), None);
        assert_eq!(step_index(3, 2, RepeatMode::All, false, true, 0), Some(0));
        assert_eq!(step_index(3, 0, RepeatMode::Off, false, false, 0), None);
        assert_eq!(step_index(3, 0, RepeatMode::All, false, false, 0), Some(2));
        assert_eq!(step_index(0, 0, RepeatMode::All, false, true, 0), None);
    }

    #[test]
    fn shuffle_never_repeats_the_current_track() {
        for current in 0..4 {
            for random in 0..12 {
                let next = step_index(4, current, RepeatMode::All, true, true, random)
                    .expect("shuffle index");
                assert_ne!(next, current);
                assert!(next < 4);
            }
        }
    }

    #[test]
    fn muted_volume_is_silent() {
        assert_eq!(effective_volume(0.7, true), 0.0);
        assert_eq!(effective_volume(0.7, false), 0.7);
    }

    #[test]
    fn durations_are_reported_in_milliseconds() {
        assert_eq!(duration_ms(Duration::from_millis(1234)), 1234);
        assert_eq!(duration_ms(Duration::ZERO), 0);
    }

    #[test]
    fn decodes_a_generated_wav_file() {
        let path = std::env::temp_dir().join(format!(
            "zenith-audio-test-{}-{}.wav",
            std::process::id(),
            random_seed()
        ));
        write_test_wav(&path);

        let (_decoder, info) = decode_file(&path).expect("decode generated wav");
        assert_eq!(info.sample_rate, 8_000);
        assert_eq!(info.channels, 1);
        let duration = info
            .duration
            .expect("duration from the generated wav")
            .as_millis();
        assert!(
            (90..=110).contains(&duration),
            "unexpected duration: {duration}"
        );

        let _ = std::fs::remove_file(path);
    }

    fn write_test_wav(path: &Path) {
        let sample_rate = 8_000u32;
        let samples = 800u32;
        let data_len = samples * 2;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);

        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.resize(bytes.len() + data_len as usize, 0);

        std::fs::write(path, bytes).expect("write the generated wav");
    }
}
