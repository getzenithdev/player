//! Offline-first lyrics. Provider results are proposals, never silent replacements.
use crate::{
    library, metadata,
    storage::{StorageManager, indexes::TrackEntry},
};
use lofty::{file::TaggedFileExt, tag::ItemKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

const DOCUMENT: &str = "library/lyrics.json";
pub const MAX_TEXT: usize = 1024 * 1024;
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Word {
    pub at: u64,
    pub text: String,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Line {
    pub at: Option<u64>,
    pub text: String,
    pub words: Vec<Word>,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Parsed {
    pub lines: Vec<Line>,
    pub offset: i64,
    pub warnings: Vec<String>,
}

fn timestamp(value: &str) -> Option<u64> {
    let (minutes, seconds) = value.split_once(':')?;
    if !minutes.bytes().all(|b| b.is_ascii_digit()) || minutes.is_empty() {
        return None;
    }
    let minutes: u64 = minutes.parse().ok()?;
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let seconds: u64 = seconds.parse().ok()?;
    if seconds >= 60 || fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let millis = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().ok()? * 10u64.pow(3 - fraction.len() as u32)
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds * 1000 + millis)
        .filter(|n| *n <= 86_400_000)
}
fn words(text: &str) -> (String, Vec<Word>) {
    let mut output = String::new();
    let mut result = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>').map(|v| v + start) else {
            output.push_str(&rest[start..]);
            return (output, result);
        };
        let Some(at) = timestamp(&rest[start + 1..end]) else {
            output.push_str(&rest[start..=end]);
            rest = &rest[end + 1..];
            continue;
        };
        rest = &rest[end + 1..];
        let next = rest.find('<').unwrap_or(rest.len());
        let text = rest[..next].to_string();
        output.push_str(&text);
        result.push(Word { at, text });
        rest = &rest[next..];
    }
    output.push_str(rest);
    (output.trim().to_string(), result)
}
pub fn parse(raw: &str) -> Result<Parsed, String> {
    if raw.len() > MAX_TEXT || raw.contains('\0') {
        return Err("Letra inválida o mayor a 1 MiB".into());
    }
    let mut parsed = Parsed::default();
    for (number, raw_line) in raw.trim_start_matches('\u{feff}').lines().enumerate() {
        if number >= 10000 {
            return Err("Demasiadas líneas".into());
        }
        let mut rest = raw_line.trim();
        if let Some(value) = rest
            .strip_prefix("[offset:")
            .and_then(|v| v.strip_suffix(']'))
        {
            parsed.offset = value
                .trim()
                .parse::<i64>()
                .map_err(|_| "Desfase LRC inválido")?
                .clamp(-600_000, 600_000);
            continue;
        }
        if ["[ar:", "[al:", "[ti:", "[by:", "[length:", "[re:", "[ve:"]
            .iter()
            .any(|tag| rest.starts_with(tag))
        {
            continue;
        }
        let mut times = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else {
                break;
            };
            let Some(at) = timestamp(&rest[1..end]) else {
                break;
            };
            times.push(at);
            rest = &rest[end + 1..];
        }
        if times.is_empty() {
            if rest.starts_with('[') {
                parsed
                    .warnings
                    .push(format!("Marca de tiempo inválida en línea {}", number + 1));
            }
            if !rest.is_empty() {
                parsed.lines.push(Line {
                    text: rest.into(),
                    ..Line::default()
                });
            }
        } else {
            let (text, mut syllables) = words(rest);
            if syllables.windows(2).any(|w| w[0].at > w[1].at)
                || syllables.first().is_some_and(|w| w.at < times[0])
            {
                parsed.warnings.push(format!(
                    "Tiempos de palabras inválidos en línea {}",
                    number + 1
                ));
                syllables.clear();
            }
            let first = times[0];
            for at in times {
                parsed.lines.push(Line {
                    at: Some(at),
                    text: text.clone(),
                    words: syllables
                        .iter()
                        .map(|w| Word {
                            at: w.at.saturating_add(at.saturating_sub(first)),
                            text: w.text.clone(),
                        })
                        .collect(),
                });
            }
        }
    }
    // Untimed text is retained and displayed, but cannot become an active lyric.
    parsed.lines.sort_by_key(|l| l.at.unwrap_or(u64::MAX));
    if parsed.lines.is_empty() {
        return Err("No hay texto de letra".into());
    }
    Ok(parsed)
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Candidate {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<f64>,
    pub raw: String,
    pub source_url: Option<String>,
    pub score: u32,
    pub parsed: Parsed,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Preferences {
    pub mode: String,
    pub offset: i64,
    pub size: u32,
    pub font: String,
    pub color: String,
    pub active_color: String,
    pub external: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            mode: "normal".into(),
            offset: 0,
            size: 30,
            font: "system".into(),
            color: "#777777".into(),
            active_color: "#8956dd".into(),
            external: false,
        }
    }
}
impl Preferences {
    fn validate(&self) -> Result<(), String> {
        if !["normal", "karaoke", "minimal", "fullscreen"].contains(&self.mode.as_str())
            || !["system", "serif", "sans-serif"].contains(&self.font.as_str())
            || !(16..=80).contains(&self.size)
            || !(-600_000..=600_000).contains(&self.offset)
            || ![&self.color, &self.active_color].iter().all(|v| {
                v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit())
            })
        {
            return Err("Preferencias de letras inválidas".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct TrackLyrics {
    pub selected: Option<Candidate>,
    pub rejected: Vec<String>,
    pub offset: i64,
    pub suppress_local: bool,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct Document {
    tracks: BTreeMap<String, TrackLyrics>,
    preferences: Preferences,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub saved: TrackLyrics,
    pub preferences: Preferences,
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<String>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Snapshot,
    Search {
        query: Option<String>,
    },
    Import {
        path: String,
    },
    ImportPlus {
        path: String,
    },
    Save {
        raw: String,
    },
    Select {
        candidate: Box<Candidate>,
    },
    Reject {
        id: String,
    },
    Delete,
    Export {
        path: String,
    },
    Preferences {
        preferences: Preferences,
        offset: i64,
    },
}
fn load(storage: &StorageManager) -> Result<Document, String> {
    storage
        .load_document::<Document>(DOCUMENT)
        .map(|d| d.map(|d| d.data).unwrap_or_default())
        .map_err(|e| e.to_string())
}
fn candidate(provider: &str, raw: String, track: &TrackEntry) -> Result<Candidate, String> {
    let parsed = parse(&raw)?;
    Ok(Candidate {
        id: format!("{provider}:{:x}", Sha256::digest(raw.as_bytes())),
        provider: provider.into(),
        title: track.title.clone(),
        artist: track.artist.clone().unwrap_or_default(),
        album: track.album.clone().unwrap_or_default(),
        duration: track.duration.map(|v| v as f64),
        raw,
        source_url: None,
        score: 100,
        parsed,
    })
}
fn local_candidates(track: &TrackEntry) -> Vec<Candidate> {
    let mut result = Vec::new();
    if let Some(path) = track.local_path.as_deref() {
        let path = Path::new(path);
        for ext in ["lrc", "txt"] {
            let side = path.with_extension(ext);
            if let Ok(raw) = crate::resources::read_text(&side, MAX_TEXT)
                && let Ok(c) = candidate("local", raw, track)
            {
                result.push(c);
            }
        }
        if let Ok(file) = lofty::read_from_path(path) {
            for tag in file.tags() {
                if let Some(raw) = tag
                    .get_string(ItemKey::Lyrics)
                    .or_else(|| tag.get_string(ItemKey::UnsyncLyrics))
                    && let Ok(c) = candidate("embedded", raw.into(), track)
                {
                    result.push(c);
                }
            }
        }
    }
    result
}
fn score(track: &TrackEntry, title: &str, artist: &str, album: &str, duration: Option<f64>) -> u32 {
    let mut score = 0;
    if metadata::key(&track.title) == metadata::key(title) {
        score += 45;
    }
    if track
        .artist
        .as_deref()
        .is_some_and(|v| !v.is_empty() && metadata::key(v) == metadata::key(artist))
    {
        score += 30;
    }
    if track
        .album
        .as_deref()
        .is_some_and(|v| !v.is_empty() && metadata::key(v) == metadata::key(album))
    {
        score += 10;
    }
    if let (Some(a), Some(b)) = (track.duration, duration)
        && (a as f64 - b).abs() <= 2.0
    {
        score += 15;
    }
    score
}
fn lrclib(track: &TrackEntry, query: Option<String>) -> Result<Vec<Candidate>, String> {
    let mut url = reqwest::Url::parse("https://lrclib.net/api/search").unwrap();
    if let Some(query) = query.filter(|s| !s.trim().is_empty()) {
        url.query_pairs_mut().append_pair("q", &query);
    } else {
        url.query_pairs_mut()
            .append_pair("track_name", &track.title)
            .append_pair("artist_name", track.artist.as_deref().unwrap_or_default());
    }
    let records: serde_json::Value =
        serde_json::from_slice(&crate::resources::get(url, 2 * MAX_TEXT)?)
            .map_err(|_| "Respuesta LRCLIB inválida")?;
    let rows = records.as_array().ok_or("Respuesta LRCLIB inválida")?;
    Ok(rows
        .iter()
        .take(20)
        .filter_map(|r| {
            let title = r["trackName"].as_str()?;
            let artist = r["artistName"].as_str().unwrap_or_default();
            let album = r["albumName"].as_str().unwrap_or_default();
            let raw = r["syncedLyrics"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| r["plainLyrics"].as_str())?;
            let id = r["id"].as_u64()?;
            let duration = r["duration"].as_f64();
            Some(Candidate {
                id: format!("lrclib:{id}"),
                provider: "lrclib".into(),
                title: title.into(),
                artist: artist.into(),
                album: album.into(),
                duration,
                raw: raw.into(),
                source_url: Some(format!("https://lrclib.net/api/get/{id}")),
                score: score(track, title, artist, album, duration),
                parsed: parse(raw).ok()?,
            })
        })
        .collect())
}
/// LyricsPlus v2 adapter consumes exported responses. The documented public host
/// is unavailable; no undocumented mirrors or account-cookie scraping is used.
fn lyrics_plus(raw: &str, track: &TrackEntry) -> Result<Candidate, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| "JSON LyricsPlus inválido")?;
    let rows = value["lyrics"]
        .as_array()
        .ok_or("Falta lyrics[] de LyricsPlus v2")?;
    let mut text = String::new();
    for row in rows.iter().take(10000) {
        let start = row["time"].as_u64().ok_or("Falta time en milisegundos")?;
        text.push_str(&stamp(start));
        if let Some(words) = row["syllables"].as_array() {
            for word in words {
                let at = word["time"].as_u64().ok_or("Falta tiempo de sílaba")?;
                text.push_str(&stamp(at).replace('[', "<").replace(']', ">"));
                text.push_str(word["text"].as_str().unwrap_or_default());
            }
        } else {
            text.push_str(row["text"].as_str().unwrap_or_default());
        }
        text.push('\n');
    }
    let mut c = candidate("lyrics-plus", text, track)?;
    // v2 timing envelopes don't reliably contain matching metadata. Require review.
    c.score = 0;
    Ok(c)
}
fn stamp(ms: u64) -> String {
    format!(
        "[{:02}:{:02}.{:03}]",
        ms / 60000,
        (ms / 1000) % 60,
        ms % 1000
    )
}
pub fn execute(
    storage: &StorageManager,
    track_id: String,
    action: Action,
) -> Result<Snapshot, String> {
    // Do network and file inspection outside the shared mutation lock.
    let track = {
        let _lock = storage.library_lock();
        library::load(storage)?
            .tracks
            .into_iter()
            .find(|t| t.id == track_id)
            .ok_or("Selecciona una canción de la biblioteca")?
    };
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    match &action {
        Action::Snapshot => candidates = local_candidates(&track),
        Action::Search { query } => {
            if !load(storage)?.preferences.external {
                return Err("Activa la búsqueda externa de letras primero".into());
            }
            candidates = local_candidates(&track);
            match lrclib(&track, query.clone()) {
                Ok(c) => candidates.extend(c),
                Err(e) => warnings.push(e),
            }
        }
        Action::Import { path } => candidates.push(candidate(
            "local",
            crate::resources::read_text(Path::new(path), MAX_TEXT)?,
            &track,
        )?),
        Action::ImportPlus { path } => candidates.push(lyrics_plus(
            &crate::resources::read_text(Path::new(path), MAX_TEXT)?,
            &track,
        )?),
        _ => {}
    }
    let _lock = storage.library_lock();
    let mut doc = load(storage)?;
    let saved = doc.tracks.entry(track_id.clone()).or_default();
    let mut changed = true;
    match action {
        Action::Save { raw } => {
            saved.selected = Some(candidate("edited", raw, &track)?);
            saved.suppress_local = false;
        }
        Action::Select { mut candidate } => {
            candidate.parsed = parse(&candidate.raw)?;
            if candidate.id.len() > 256
                || candidate.provider.len() > 64
                || candidate.title.len() > 4096
                || candidate.artist.len() > 4096
                || candidate.album.len() > 4096
                || candidate
                    .source_url
                    .as_ref()
                    .is_some_and(|v| v.len() > 4096)
            {
                return Err("Candidato inválido".into());
            }
            saved.rejected.retain(|id| id != &candidate.id);
            saved.selected = Some(*candidate);
            saved.suppress_local = false;
        }
        Action::Reject { id } => {
            if id.len() > 256 || saved.rejected.len() >= 1000 {
                return Err("Demasiados rechazos".into());
            }
            if !saved.rejected.contains(&id) {
                saved.rejected.push(id.clone());
            }
            if saved.selected.as_ref().is_some_and(|c| c.id == id) {
                saved.selected = None;
            }
            saved.suppress_local = true;
        }
        Action::Delete => {
            saved.selected = None;
            saved.suppress_local = true;
        }
        Action::Export { path } => {
            let selected = saved
                .selected
                .as_ref()
                .ok_or("Guarda o selecciona una letra primero")?;
            if !matches!(
                Path::new(&path).extension().and_then(|s| s.to_str()),
                Some("lrc" | "txt")
            ) {
                return Err("Usa la extensión .lrc o .txt".into());
            }
            let raw = if path.ends_with(".txt") {
                selected
                    .parsed
                    .lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                selected.raw.clone()
            };
            crate::storage::write_atomic(Path::new(&path), raw.as_bytes())
                .map_err(|e| e.to_string())?;
            changed = false;
        }
        Action::Preferences {
            preferences,
            offset,
        } => {
            preferences.validate()?;
            if !(-600000..=600000).contains(&offset) {
                return Err("Desfase inválido".into());
            }
            saved.offset = offset;
            doc.preferences = preferences;
        }
        _ => changed = false,
    }
    if changed {
        storage
            .save_document(DOCUMENT, &doc)
            .map_err(|e| e.to_string())?;
    }
    let saved = doc.tracks.remove(&track_id).unwrap_or_default();
    candidates.retain(|c| !saved.rejected.contains(&c.id));
    candidates.sort_by_key(|c| std::cmp::Reverse(c.score));
    candidates.dedup_by(|a, b| a.raw == b.raw);
    Ok(Snapshot {
        saved,
        preferences: doc.preferences,
        candidates,
        warnings,
    })
}
#[tauri::command]
pub async fn lyrics_action(
    app: tauri::AppHandle,
    track_id: String,
    action: Action,
) -> Result<Snapshot, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        execute(&app.state::<StorageManager>(), track_id, action)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractions_repeats_and_sort() {
        let p = parse("[00:03.5][00:05.050]Again\n[00:01.01]First").unwrap();
        assert_eq!(
            p.lines.iter().map(|l| l.at).collect::<Vec<_>>(),
            vec![Some(1010), Some(3500), Some(5050)]
        );
    }
    #[test]
    fn words_offset_and_unicode() {
        let p = parse("\u{feff}[offset:-150]\n[00:01.000]<00:01.000>Ol<00:01.500>á").unwrap();
        assert_eq!(p.offset, -150);
        assert_eq!(p.lines[0].text, "Olá");
        assert_eq!(p.lines[0].words[1].at, 1500);
    }
    #[test]
    fn malformed_and_plain_are_safe() {
        let p = parse("[00:99]broken\nPlain\n[00:01]Valid").unwrap();
        assert_eq!(p.warnings.len(), 1);
        assert_eq!(p.lines.len(), 3);
        assert!(parse("").is_err());
        assert!(parse("\0").is_err());
    }
    #[test]
    fn backwards_words_fall_back_to_line() {
        assert!(
            parse("[00:01]<00:02>one<00:01>two").unwrap().lines[0]
                .words
                .is_empty()
        );
    }
    #[test]
    fn edition_and_duration_reduce_score() {
        let t = TrackEntry {
            title: "Song (Live)".into(),
            artist: Some("A".into()),
            duration: Some(100),
            ..Default::default()
        };
        assert_eq!(score(&t, "Song", "A", "", Some(150.0)), 30);
        assert_eq!(score(&t, "Song (Live)", "A", "", Some(101.0)), 90);
    }
    #[test]
    fn plus_word_adapter() {
        let c=lyrics_plus(r#"{"lyrics":[{"time":1000,"syllables":[{"time":1000,"text":"Hi "},{"time":1500,"text":"there"}]}]}"#,&TrackEntry::default()).unwrap();
        assert_eq!(c.parsed.lines[0].words.len(), 2);
    }
    #[test]
    fn invalid_preferences() {
        let p = Preferences {
            color: "url(evil)".into(),
            ..Preferences::default()
        };
        assert!(p.validate().is_err());
    }
    #[test]
    fn save_restart_reject_delete() {
        let root =
            std::env::temp_dir().join(format!("zenith-lyrics-test-{}", rand::random::<u64>()));
        let s = StorageManager::open(root.clone(), true).unwrap();
        library::save(
            &s,
            &crate::storage::indexes::TracksDocument {
                tracks: vec![TrackEntry {
                    id: "track".into(),
                    title: "Song".into(),
                    ..Default::default()
                }],
                locations: vec![],
            },
        )
        .unwrap();
        let a = execute(
            &s,
            "track".into(),
            Action::Save {
                raw: "[00:01]Saved".into(),
            },
        )
        .unwrap();
        let id = a.saved.selected.unwrap().id;
        drop(s);
        let s = StorageManager::open(root.clone(), true).unwrap();
        assert!(
            execute(&s, "track".into(), Action::Snapshot)
                .unwrap()
                .saved
                .selected
                .is_some()
        );
        assert!(execute(&s, "track".into(), Action::Search { query: None }).is_err());
        let a = execute(&s, "track".into(), Action::Reject { id }).unwrap();
        assert!(a.saved.selected.is_none());
        assert_eq!(a.saved.rejected.len(), 1);
        execute(&s, "track".into(), Action::Delete).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
