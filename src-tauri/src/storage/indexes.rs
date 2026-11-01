//! In-memory indexes rebuilt at startup from `library/tracks.json`.
//!
//! The roadmap asks for indexes over title, artist, album, genre, year,
//! duration and path. They live only in memory: the JSON files remain the
//! source of truth, so a restart rebuilds them from disk.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Fields covered by the in-memory indexes.
pub const INDEX_FIELDS: &[&str] = &[
    "title", "artist", "album", "genre", "year", "duration", "path",
];

/// Minimum track record needed to build the phase 2 indexes.
///
/// Every field except the ones the phase 6 model defines as mandatory is
/// optional, so incomplete metadata does not drop a track from the library.
/// The final identity rules belong to phase 6.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub duration: Option<u64>,
    #[serde(default)]
    pub track_number: Option<u32>,
    #[serde(default)]
    pub disc_number: Option<u32>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(default)]
    pub artwork_id: Option<String>,
    #[serde(default)]
    pub lyrics_id: Option<String>,
    #[serde(default)]
    pub album_artist: Option<String>,
    #[serde(default)]
    pub comments: Option<String>,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub modified_ns: u128,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub added_at: u64,
    #[serde(default)]
    pub missing: bool,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub play_count: u64,
    #[serde(default)]
    pub last_played: Option<u64>,
    // Whole editable metadata overlay. Rescans preserve explicit internal edits.
    #[serde(default)]
    pub edited: bool,
}

/// Payload of `library/tracks.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TracksDocument {
    #[serde(default)]
    pub tracks: Vec<TrackEntry>,
    #[serde(default)]
    pub locations: Vec<String>,
}

/// Search result exposed to the interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrackHit {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub path: Option<String>,
}

/// Collection of lookups built from the stored tracks.
#[derive(Debug, Default)]
pub struct LibraryIndexes {
    tracks: Vec<TrackEntry>,
    by_title: HashMap<String, Vec<usize>>,
    by_artist: HashMap<String, Vec<usize>>,
    by_album: HashMap<String, Vec<usize>>,
    by_genre: HashMap<String, Vec<usize>>,
    by_year: HashMap<u32, Vec<usize>>,
    by_duration: HashMap<u64, Vec<usize>>,
    by_path: HashMap<String, Vec<usize>>,
}

impl LibraryIndexes {
    /// Rebuilds every index from scratch, in a single pass over the tracks.
    pub fn rebuild(tracks: Vec<TrackEntry>) -> Self {
        let mut index = Self::default();
        for (position, track) in tracks.iter().enumerate() {
            push_text(&mut index.by_title, &track.title, position);
            push_option(&mut index.by_artist, &track.artist, position);
            push_option(&mut index.by_album, &track.album, position);
            push_option(&mut index.by_genre, &track.genre, position);
            if let Some(year) = track.year {
                index.by_year.entry(year).or_default().push(position);
            }
            if let Some(duration) = track.duration {
                index
                    .by_duration
                    .entry(duration)
                    .or_default()
                    .push(position);
            }
            push_option(&mut index.by_path, &track.local_path, position);
        }
        index.tracks = tracks;
        index
    }

    /// Number of tracks currently indexed.
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// Case-insensitive substring search over every indexed field.
    ///
    /// An empty query matches all tracks, which is what the library view shows
    /// before the user types anything.
    pub fn search(&self, query: &str) -> Vec<TrackHit> {
        let needle = normalize(query);
        self.tracks
            .iter()
            .filter(|track| needle.is_empty() || track_matches(track, &needle))
            .map(|track| TrackHit {
                id: track.id.clone(),
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                path: track.local_path.clone(),
            })
            .collect()
    }
}

fn track_matches(track: &TrackEntry, needle: &str) -> bool {
    contains(&track.title, needle)
        || contains(track.artist.as_deref().unwrap_or_default(), needle)
        || contains(track.album.as_deref().unwrap_or_default(), needle)
        || contains(track.genre.as_deref().unwrap_or_default(), needle)
        || contains(track.local_path.as_deref().unwrap_or_default(), needle)
        || track
            .year
            .is_some_and(|year| year.to_string().contains(needle))
        || track
            .duration
            .is_some_and(|duration| duration.to_string().contains(needle))
}

fn contains(haystack: &str, needle: &str) -> bool {
    normalize(haystack).contains(needle)
}

/// Lowercases and collapses whitespace so small spelling differences do not
/// create separate index buckets.
fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
        .to_lowercase()
}

fn push_text(map: &mut HashMap<String, Vec<usize>>, value: &str, position: usize) {
    let key = normalize(value);
    if key.is_empty() {
        return;
    }
    map.entry(key).or_default().push(position);
}

fn push_option(map: &mut HashMap<String, Vec<usize>>, value: &Option<String>, position: usize) {
    if let Some(value) = value {
        push_text(map, value, position);
    }
}
