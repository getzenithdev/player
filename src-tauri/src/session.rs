//! One atomic history document owns checkpoints, a saved queue, visits and
//! cumulative counters. The audio worker submits ordered checkpoints to a
//! dedicated persistence thread; seek distance never counts as listening time.
use crate::{
    playlists::{self, Entry},
    storage::{StorageManager, indexes::TrackEntry},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const DOCUMENT: &str = "library/history.json";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub queue: Vec<Entry>,
    pub current_id: Option<String>,
    pub position_ms: u64,
    pub volume: f32,
    pub muted: bool,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub visit: Option<Visit>,
}
impl Default for SessionState {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            current_id: None,
            position_ms: 0,
            volume: 0.7,
            muted: false,
            repeat: RepeatMode::Off,
            shuffle: false,
            visit: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Visit {
    pub id: String,
    pub entry: Entry,
    pub started_at: u64,
    pub updated_at: u64,
    pub listened_ms: u64,
    pub position_ms: u64,
}
impl Visit {
    pub fn new(entry: Entry) -> Self {
        let now = playlists::now();
        Self {
            id: playlists::new_id(),
            entry,
            started_at: now,
            updated_at: now,
            listened_ms: 0,
            position_ms: 0,
        }
    }
}
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    pub play_count: u64,
    pub listened_ms: u64,
    pub last_played: u64,
    pub last_position_ms: u64,
}
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HistoryDocument {
    pub session: SessionState,
    pub saved_queue: Option<SessionState>,
    pub visits: Vec<Visit>,
    pub statistics: BTreeMap<String, Statistics>,
}
pub fn load(storage: &StorageManager) -> Result<HistoryDocument, String> {
    storage
        .load_document::<HistoryDocument>(DOCUMENT)
        .map(|d| d.map(|d| d.data).unwrap_or_default())
        .map_err(|e| e.to_string())
}
fn key(entry: &Entry) -> String {
    entry
        .track_id
        .as_ref()
        .map(|id| format!("id:{id}"))
        .unwrap_or_else(|| format!("path:{}", entry.path))
}
pub fn apply_checkpoint(doc: &mut HistoryDocument, state: SessionState) {
    if let Some(visit) = &state.visit {
        let old = doc.visits.iter().find(|v| v.id == visit.id);
        let old_ms = old.map(|v| v.listened_ms).unwrap_or_default();
        if visit.listened_ms >= old_ms {
            let delta = visit.listened_ms - old_ms;
            let stats = doc.statistics.entry(key(&visit.entry)).or_default();
            stats.listened_ms = stats.listened_ms.saturating_add(delta);
            if old_ms < 30_000 && visit.listened_ms >= 30_000 {
                stats.play_count = stats.play_count.saturating_add(1);
            }
            stats.last_played = stats.last_played.max(visit.updated_at);
            stats.last_position_ms = visit.position_ms;
            if let Some(existing) = doc.visits.iter_mut().find(|v| v.id == visit.id) {
                *existing = visit.clone();
            } else {
                doc.visits.push(visit.clone());
            }
            if doc.visits.len() > 1000 {
                doc.visits.drain(..doc.visits.len() - 1000);
            }
        }
    }
    doc.session = state;
}
pub fn checkpoint(
    storage: &StorageManager,
    state: SessionState,
    save_queue: bool,
) -> Result<(), String> {
    let _operation = storage.library_lock();
    let mut doc = load(storage)?;
    let before = doc.clone();
    apply_checkpoint(&mut doc, state);
    if save_queue {
        doc.saved_queue = Some(doc.session.clone());
    }
    if doc != before {
        storage
            .save_document(DOCUMENT, &doc)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn restore(storage: &StorageManager, saved: bool) -> Result<SessionState, String> {
    let _operation = storage.library_lock();
    let doc = load(storage)?;
    let mut state = if saved {
        doc.saved_queue.ok_or("Todavía no hay una cola guardada")?
    } else {
        doc.session
    };
    let library = playlists::library_tracks(storage)?;
    playlists::resolve_entries(&mut state.queue, &library);
    if !state.volume.is_finite() || !(0.0..=1.0).contains(&state.volume) {
        state.volume = 0.7;
    }
    if !state
        .queue
        .iter()
        .any(|e| Some(&e.id) == state.current_id.as_ref())
    {
        state.current_id = None;
        state.position_ms = 0;
        state.visit = None;
    }
    if saved {
        state.visit = None;
    } // Explicit replay is a new listening visit.
    Ok(state)
}
pub fn overlay(storage: &StorageManager, tracks: &mut [TrackEntry]) -> Result<(), String> {
    let doc = load(storage)?;
    for t in tracks {
        for key in [
            Some(format!("id:{}", t.id)),
            t.local_path.as_ref().map(|p| format!("path:{p}")),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(stats) = doc.statistics.get(&key) {
                t.play_count = t.play_count.saturating_add(stats.play_count);
                t.listened_ms = t.listened_ms.saturating_add(stats.listened_ms);
                if stats.last_played >= t.last_played.unwrap_or_default() {
                    t.last_played = Some(stats.last_played);
                    t.last_position_ms = stats.last_position_ms;
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

/// Skip missing files without deleting their queue entries or looping forever.
pub fn available_index(
    available: &[usize],
    len: usize,
    current: usize,
    wrap: bool,
    shuffle: bool,
    forward: bool,
    random: usize,
) -> Option<usize> {
    if current >= len || available.is_empty() {
        return None;
    }
    if shuffle {
        let alternatives: Vec<_> = available
            .iter()
            .copied()
            .filter(|i| *i != current)
            .collect();
        if !alternatives.is_empty() {
            return Some(alternatives[random % alternatives.len()]);
        }
    }
    if forward {
        available
            .iter()
            .copied()
            .find(|i| *i > current)
            .or_else(|| {
                if wrap {
                    available.first().copied()
                } else {
                    None
                }
            })
    } else {
        available
            .iter()
            .rev()
            .copied()
            .find(|i| *i < current)
            .or_else(|| {
                if wrap {
                    available.last().copied()
                } else {
                    None
                }
            })
    }
}

/// Decoder position samples at 200 ms intervals. Discontinuities (seeks,
/// suspension, device reset) larger than one second are not credited.
pub fn listening_delta(previous: u64, position: u64, playing: bool, paused: bool) -> u64 {
    let delta = position.saturating_sub(previous);
    if playing && !paused && delta <= 1000 {
        delta
    } else {
        0
    }
}
