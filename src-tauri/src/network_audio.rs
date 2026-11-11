//! Bounded progressive playback. HTTP and decoding run on a dedicated thread;
//! the device callback only takes already decoded samples (never network I/O).
use rodio::{ChannelCount, Decoder, SampleRate, Source};
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

const BLOCK: u64 = 64 * 1024;
const CACHE_BLOCKS: usize = 128;
const PCM_CHUNK: usize = 2048;
const AHEAD_SECONDS: u64 = 15;
const BEHIND_SECONDS: u64 = 5;
const MAX_AHEAD_SAMPLES: u64 = 8 * 1024 * 1024;
const MAX_BEHIND_SAMPLES: u64 = 4 * 1024 * 1024;

/// Never serialized: URLs, authorization headers and signed links stay in Rust.
pub struct RemoteAudio {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub hint: Option<String>,
}
pub type Resolver = Arc<dyn Fn(&AtomicBool) -> Result<RemoteAudio, String> + Send + Sync>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BufferedRange {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone)]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration: Option<Duration>,
    pub format: String,
}
struct PcmBlock {
    start: u64,
    samples: Arc<[f32]>,
}
struct State {
    info: Option<StreamInfo>,
    blocks: VecDeque<PcmBlock>,
    seek: Option<Duration>,
    eof: bool,
    error: Option<String>,
}
struct Shared {
    state: Mutex<State>,
    cancel: AtomicBool,
    position: AtomicU64,
    generation: AtomicU64,
    seeking: AtomicBool,
    waiting: AtomicBool,
}

pub struct StreamSession {
    shared: Arc<Shared>,
    attached: bool,
}
pub struct StreamSnapshot {
    pub info: Option<StreamInfo>,
    pub position: Duration,
    pub ranges: Vec<BufferedRange>,
    pub buffering: bool,
    pub error: Option<String>,
}
impl StreamSession {
    pub fn start(resolve: Resolver, at: Duration) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                info: None,
                blocks: VecDeque::new(),
                seek: None,
                eof: false,
                error: None,
            }),
            cancel: AtomicBool::new(false),
            position: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            seeking: AtomicBool::new(true),
            waiting: AtomicBool::new(true),
        });
        let worker = shared.clone();
        thread::Builder::new()
            .name("zenith-network-audio".into())
            .spawn(move || {
                if let Err(error) = decode(resolve, &worker, at) {
                    let mut state = worker.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.error.get_or_insert(error);
                    state.eof = true;
                }
            })
            .map_err(|_| "No se pudo iniciar el streaming".to_string())?;
        Ok(Self {
            shared,
            attached: false,
        })
    }
    pub fn snapshot(&self) -> StreamSnapshot {
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let rate = state
            .info
            .as_ref()
            .map(|i| u64::from(i.sample_rate) * u64::from(i.channels))
            .unwrap_or(1);
        let position = self.shared.position.load(Ordering::Relaxed);
        let ranges = match (state.blocks.front(), state.blocks.back()) {
            (Some(first), Some(last)) => vec![BufferedRange {
                start_ms: first.start * 1000 / rate,
                end_ms: (last.start + last.samples.len() as u64) * 1000 / rate,
            }],
            _ => Vec::new(),
        };
        StreamSnapshot {
            info: state.info.clone(),
            position: Duration::from_secs_f64(position as f64 / rate as f64),
            ranges,
            buffering: self.shared.seeking.load(Ordering::Acquire)
                || self.shared.waiting.load(Ordering::Relaxed),
            error: state.error.clone(),
        }
    }
    pub fn take_source(&mut self) -> Option<NetworkSource> {
        if self.attached {
            return None;
        }
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let info = state.info.clone()?;
        // Wait for a small initial decoded buffer, not the whole resource.
        let count: usize = state.blocks.iter().map(|b| b.samples.len()).sum();
        if self.shared.seeking.load(Ordering::Acquire)
            || (count < info.sample_rate as usize * info.channels as usize / 2 && !state.eof)
        {
            return None;
        }
        self.attached = true;
        Some(NetworkSource {
            shared: self.shared.clone(),
            info,
            samples: None,
            generation: self.shared.generation.load(Ordering::Acquire),
            output_channel: 0,
        })
    }
    pub fn seek(&self, at: Duration) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(info) = state.info.as_ref() else {
            return;
        };
        let target = (at.as_secs_f64() * f64::from(info.sample_rate)).round() as u64
            * u64::from(info.channels);
        // Seeking within decoded audio is immediate; no HTTP request or decoder reset.
        let in_buffer = !self.shared.seeking.load(Ordering::Acquire)
            && state
                .blocks
                .iter()
                .any(|b| target >= b.start && target < b.start + b.samples.len() as u64);
        if in_buffer {
            state.seek = None;
            self.shared.position.store(target, Ordering::Release);
            self.shared.seeking.store(false, Ordering::Release);
        } else {
            self.shared.seeking.store(true, Ordering::Release);
            state.seek = Some(at);
        }
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
    }
}
impl Drop for StreamSession {
    fn drop(&mut self) {
        self.shared.cancel.store(true, Ordering::Release);
    }
}

pub struct NetworkSource {
    shared: Arc<Shared>,
    info: StreamInfo,
    samples: Option<(Arc<[f32]>, usize)>,
    generation: u64,
    output_channel: u16,
}
impl Iterator for NetworkSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.next_sample();
        if sample.is_some() {
            self.output_channel = (self.output_channel + 1) % self.info.channels;
        }
        sample
    }
}
impl NetworkSource {
    fn next_sample(&mut self) -> Option<f32> {
        if self.shared.cancel.load(Ordering::Acquire) {
            return None;
        }
        let generation = self.shared.generation.load(Ordering::Acquire);
        if self.generation != generation {
            if self.output_channel != 0 {
                return Some(0.0);
            }
            self.samples = None;
            self.generation = generation;
        }
        if self.shared.seeking.load(Ordering::Acquire) {
            return Some(0.0);
        }
        if let Some((samples, index)) = &mut self.samples {
            if let Some(sample) = samples.get(*index) {
                *index += 1;
                self.shared.position.fetch_add(1, Ordering::Relaxed);
                return Some(*sample);
            }
        }
        let position = self.shared.position.load(Ordering::Relaxed);
        // Resume on a frame boundary so an underrun never swaps stereo channels.
        if self.output_channel != 0 {
            return Some(0.0);
        }
        let Ok(state) = self.shared.state.try_lock() else {
            return Some(0.0);
        };
        if self.shared.waiting.load(Ordering::Relaxed) && !state.eof {
            let ahead = state
                .blocks
                .back()
                .map(|b| b.start + b.samples.len() as u64)
                .unwrap_or(0)
                .saturating_sub(position);
            if ahead < u64::from(self.info.sample_rate) * u64::from(self.info.channels) / 2 {
                return Some(0.0);
            }
        }
        if let Some(block) = state
            .blocks
            .iter()
            .find(|b| position >= b.start && position < b.start + b.samples.len() as u64)
        {
            let offset = (position - block.start) as usize;
            self.samples = Some((block.samples.clone(), offset));
            self.shared.waiting.store(false, Ordering::Relaxed);
            drop(state);
            return self.next_sample();
        }
        if state.eof {
            return None;
        }
        self.shared.waiting.store(true, Ordering::Relaxed);
        Some(0.0)
    }
}
impl Source for NetworkSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.info.channels).expect("validated channels")
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.info.sample_rate).expect("validated rate")
    }
    fn total_duration(&self) -> Option<Duration> {
        self.info.duration
    }
}

fn decode(resolve: Resolver, shared: &Arc<Shared>, at: Duration) -> Result<(), String> {
    let reader = HttpReader::open(resolve, shared.clone())?;
    let hint = reader.remote.hint.clone();
    let length = reader.length;
    let mut builder = Decoder::builder()
        .with_data(reader)
        .with_seekable(length.is_some())
        .with_gapless(true);
    if let Some(length) = length {
        builder = builder.with_byte_len(length);
    }
    if let Some(hint) = &hint {
        builder = builder.with_hint(hint);
    }
    let mut decoder = builder
        .build()
        .map_err(|_| "El servicio no entregó un formato reproducible por streaming".to_string())?;
    let info = StreamInfo {
        sample_rate: decoder.sample_rate().get(),
        channels: decoder.channels().get(),
        duration: decoder.total_duration(),
        format: hint.unwrap_or_else(|| "STREAM".into()).to_uppercase(),
    };
    if info.channels > 8 || info.sample_rate > 192_000 {
        return Err("Formato de streaming fuera de los límites de audio".into());
    }
    let rate = u64::from(info.sample_rate) * u64::from(info.channels);
    let mut cursor = 0;
    if !at.is_zero() {
        decoder
            .try_seek(at)
            .map_err(|_| "El servicio no permite restaurar esta posición".to_string())?;
        cursor = (at.as_secs_f64() * f64::from(info.sample_rate)).round() as u64
            * u64::from(info.channels);
    }
    shared.position.store(cursor, Ordering::Release);
    shared.state.lock().unwrap_or_else(|e| e.into_inner()).info = Some(info);
    shared.seeking.store(false, Ordering::Release);
    loop {
        if shared.cancel.load(Ordering::Acquire) {
            return Ok(());
        }
        let (seek, full) = {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            let position = shared.position.load(Ordering::Relaxed);
            while state.blocks.front().is_some_and(|b| {
                b.start + b.samples.len() as u64 + (rate * BEHIND_SECONDS).min(MAX_BEHIND_SAMPLES)
                    < position
            }) {
                state.blocks.pop_front();
            }
            (
                state
                    .seek
                    .take()
                    .map(|seek| (seek, shared.generation.load(Ordering::Acquire))),
                cursor >= position.saturating_add((rate * AHEAD_SECONDS).min(MAX_AHEAD_SAMPLES))
                    || state.eof,
            )
        };
        if let Some((target, generation)) = seek {
            decoder.try_seek(target).map_err(|_| {
                "Este servidor o formato no permite avanzar a esa posición".to_string()
            })?;
            cursor = (target.as_secs_f64() * rate as f64).round() as u64;
            cursor -= cursor % u64::from(decoder.channels().get());
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if generation == shared.generation.load(Ordering::Acquire) {
                state.blocks.clear();
                state.eof = false;
                shared.position.store(cursor, Ordering::Release);
                shared.seeking.store(false, Ordering::Release);
            }
            continue;
        }
        if full {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let generation = shared.generation.load(Ordering::Acquire);
        let chunk_len = PCM_CHUNK * usize::from(decoder.channels().get());
        let samples: Vec<f32> = decoder.by_ref().take(chunk_len).collect();
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if generation != shared.generation.load(Ordering::Acquire)
            && shared.seeking.load(Ordering::Acquire)
        {
            continue;
        }
        if samples.is_empty() {
            state.eof = true;
            continue;
        }
        let size = samples.len() as u64;
        state.blocks.push_back(PcmBlock {
            start: cursor,
            samples: samples.into(),
        });
        cursor += size;
    }
}

/// A seekable, bounded byte cache. Random reads use Range, including MP4's
/// metadata at the end of a resource. Never pretends an unfinished file is EOF.
struct HttpReader {
    resolve: Resolver,
    remote: RemoteAudio,
    client: reqwest::blocking::Client,
    shared: Arc<Shared>,
    position: u64,
    length: Option<u64>,
    ranges: bool,
    cache: BTreeMap<u64, Vec<u8>>,
    sequential: Option<reqwest::blocking::Response>,
    sequential_position: u64,
}
impl HttpReader {
    fn open(resolve: Resolver, shared: Arc<Shared>) -> Result<Self, String> {
        let remote = resolve(&shared.cancel)?;
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.url().scheme() != "https" || attempt.previous().len() >= 8 {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| "No se pudo abrir la conexión de audio")?;
        let mut reader = Self {
            resolve,
            remote,
            client,
            shared,
            position: 0,
            length: None,
            ranges: true,
            cache: BTreeMap::new(),
            sequential: None,
            sequential_position: 0,
        };
        reader.fetch(0).map_err(|e| e.to_string())?;
        Ok(reader)
    }
    fn request(&self, start: u64) -> io::Result<reqwest::blocking::Response> {
        if self.shared.cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Streaming cancelado",
            ));
        }
        let url = reqwest::Url::parse(&self.remote.url)
            .map_err(|_| io::Error::other("URL de audio inválida"))?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return Err(io::Error::other(
                "El audio remoto requiere HTTPS sin credenciales en la URL",
            ));
        }
        let mut request = self
            .client
            .get(url)
            .header(
                reqwest::header::RANGE,
                format!("bytes={start}-{}", start + BLOCK - 1),
            )
            .header(reqwest::header::ACCEPT_ENCODING, "identity");
        for (key, value) in &self.remote.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        request
            .send()
            .map_err(|_| io::Error::other("Conexión de audio interrumpida"))
    }
    fn fetch(&mut self, start: u64) -> io::Result<()> {
        for attempt in 0..3 {
            match self.fetch_once(start) {
                Err(error)
                    if attempt < 2
                        && !self.shared.cancel.load(Ordering::Acquire)
                        && matches!(
                            error.kind(),
                            io::ErrorKind::ConnectionReset | io::ErrorKind::UnexpectedEof
                        ) =>
                {
                    thread::sleep(Duration::from_millis(250));
                }
                outcome => return outcome,
            }
        }
        Err(io::Error::other("No se pudo recibir el tramo de audio"))
    }
    fn fetch_once(&mut self, start: u64) -> io::Result<()> {
        let mut response = None;
        for attempt in 0..3 {
            match self.request(start) {
                Ok(value) if value.status().as_u16() == 401 || value.status().as_u16() == 403 => {
                    if attempt == 2 {
                        return Err(io::Error::other(
                            "El servicio rechazó la autorización de audio",
                        ));
                    }
                    self.remote = (self.resolve)(&self.shared.cancel).map_err(io::Error::other)?;
                }
                Ok(value) if value.status().is_success() || value.status().as_u16() == 416 => {
                    response = Some(value);
                    break;
                }
                Ok(value) if value.status().is_server_error() && attempt < 2 => {
                    thread::sleep(Duration::from_millis(250));
                }
                Ok(value) => {
                    return Err(io::Error::other(format!(
                        "El servicio de audio respondió {}",
                        value.status().as_u16()
                    )));
                }
                Err(error) if attempt == 2 => return Err(error),
                Err(_) => thread::sleep(Duration::from_millis(250)),
            }
        }
        let mut response =
            response.ok_or_else(|| io::Error::other("No se pudo recibir el audio"))?;
        if response.status().as_u16() == 416 {
            if self.length.is_some_and(|len| start >= len) {
                self.cache.insert(start, Vec::new());
                return Ok(());
            }
            return Err(io::Error::other("El servicio rechazó el tramo de audio"));
        }
        if response.status().as_u16() == 206 {
            let range = response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| io::Error::other("El servicio no identificó el tramo de audio"))?;
            let (span, total) = range
                .strip_prefix("bytes ")
                .and_then(|v| v.split_once('/'))
                .ok_or_else(|| io::Error::other("Tramo de audio inválido"))?;
            let (first, last) = span
                .split_once('-')
                .ok_or_else(|| io::Error::other("Tramo de audio inválido"))?;
            let actual = first.parse::<u64>().ok();
            if actual != Some(start) {
                return Err(io::Error::other(
                    "El servicio entregó una posición de audio incorrecta",
                ));
            }
            let end = last
                .parse::<u64>()
                .map_err(|_| io::Error::other("Tramo de audio inválido"))?;
            if end < start {
                return Err(io::Error::other("Tramo de audio inválido"));
            }
            self.length = total.parse().ok().or(self.length);
            let mut bytes = Vec::new();
            response
                .by_ref()
                .take(BLOCK)
                .read_to_end(&mut bytes)
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "Recepción del audio interrumpida",
                    )
                })?;
            if bytes.len() as u64 != (end - start + 1).min(BLOCK) {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "El servicio interrumpió el tramo de audio",
                ));
            }
            self.insert(start, bytes);
        } else {
            // Servers without Range can play sequentially, but cannot promise random seeks.
            if start != 0 {
                return Err(io::Error::other(
                    "El servidor no admite reproducción por tramos",
                ));
            }
            self.ranges = false;
            self.length = response.content_length();
            self.sequential = Some(response);
            self.sequential_position = 0;
            self.fetch_sequential()?;
        }
        Ok(())
    }
    fn insert(&mut self, start: u64, bytes: Vec<u8>) {
        if self.cache.len() >= CACHE_BLOCKS {
            // Keep the initial container header for decoder probes and backward seeks.
            if let Some(key) = self
                .cache
                .keys()
                .copied()
                .find(|key| *key != 0 && *key != start)
            {
                self.cache.remove(&key);
            }
        }
        self.cache.insert(start, bytes);
    }
    fn fetch_sequential(&mut self) -> io::Result<()> {
        let start = self.sequential_position;
        let response = self
            .sequential
            .as_mut()
            .ok_or_else(|| io::Error::other("Audio remoto no disponible"))?;
        let mut bytes = Vec::new();
        response
            .take(BLOCK)
            .read_to_end(&mut bytes)
            .map_err(|_| io::Error::other("Recepción del audio interrumpida"))?;
        self.sequential_position += bytes.len() as u64;
        if bytes.is_empty() {
            self.length = Some(self.sequential_position);
        }
        self.insert(start, bytes);
        Ok(())
    }
}
impl HttpReader {
    fn read_bytes(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.shared.cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Streaming cancelado",
            ));
        }
        if output.is_empty() || self.length.is_some_and(|len| self.position >= len) {
            return Ok(0);
        }
        let cached_start = self
            .cache
            .range(..=self.position)
            .next_back()
            .filter(|(start, bytes)| self.position < **start + bytes.len() as u64)
            .map(|(start, _)| *start);
        let start = if let Some(start) = cached_start {
            start
        } else {
            if self.ranges {
                self.fetch(self.position)?;
            } else if self.position == self.sequential_position {
                self.fetch_sequential()?;
            } else {
                return Err(io::Error::other(
                    "El servidor no permite saltar a una parte no recibida",
                ));
            }
            self.position
        };
        let bytes = self
            .cache
            .get(&start)
            .ok_or_else(|| io::Error::other("Búfer de audio no disponible"))?;
        let offset = (self.position - start) as usize;
        if offset >= bytes.len() {
            return Ok(0);
        }
        let count = output.len().min(bytes.len() - offset);
        output[..count].copy_from_slice(&bytes[offset..offset + count]);
        self.position += count as u64;
        Ok(count)
    }
}
impl Read for HttpReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let result = self.read_bytes(output);
        if let Err(error) = &result {
            if !self.shared.cancel.load(Ordering::Acquire) {
                self.shared
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .error = Some(error.to_string());
            }
        }
        result
    }
}
impl Seek for HttpReader {
    fn seek(&mut self, at: SeekFrom) -> io::Result<u64> {
        let position = match at {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            SeekFrom::End(value) => {
                i128::from(self.length.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Unsupported, "Duración remota desconocida")
                })?) + i128::from(value)
            }
        };
        self.position = u64::try_from(position).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Posición de audio inválida")
        })?;
        Ok(self.position)
    }
}
