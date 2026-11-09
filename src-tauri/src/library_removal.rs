//! Explicit library removal, related records, and persistent scan exclusions.
use crate::{
    library::{self, CollectionKind, LibrarySnapshot},
    metadata,
    session::SessionState,
    storage::{
        StorageManager,
        indexes::{TrackEntry, TracksDocument},
    },
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

struct Removed {
    root: PathBuf,
    ids: BTreeSet<String>,
    paths: BTreeSet<String>,
}
static REMOVED: OnceLock<Mutex<Option<Removed>>> = OnceLock::new();

pub(crate) fn remember(storage: &StorageManager, doc: &TracksDocument) {
    let mut cached = REMOVED
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let root = storage.document_path("library/tracks.json");
    if cached.as_ref().is_none_or(|old| {
        old.root != root || old.ids != doc.removed_track_ids || old.paths != doc.excluded_paths
    }) {
        *cached = Some(Removed {
            root,
            ids: doc.removed_track_ids.clone(),
            paths: doc.excluded_paths.clone(),
        });
    }
}

fn prune_state(state: &mut SessionState, ids: &BTreeSet<String>, paths: &BTreeSet<String>) {
    let removed = |entry: &crate::playlists::Entry| {
        entry.track_id.as_ref().is_some_and(|id| ids.contains(id)) || paths.contains(&entry.path)
    };
    state.queue.retain(|entry| !removed(entry));
    if state
        .visit
        .as_ref()
        .is_some_and(|visit| removed(&visit.entry))
    {
        state.visit = None;
    }
    if state
        .current_id
        .as_ref()
        .is_some_and(|id| !state.queue.iter().any(|entry| &entry.id == id))
    {
        state.current_id = None;
        state.position_ms = 0;
    }
}

/// The playing file may finish, but later checkpoints cannot recreate deleted library history.
pub(crate) fn filter_checkpoint(
    storage: &StorageManager,
    state: &mut SessionState,
) -> Result<(), String> {
    let root = storage.document_path("library/tracks.json");
    let missing = REMOVED
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .is_none_or(|old| old.root != root);
    if missing {
        let doc = storage
            .load_document::<TracksDocument>("library/tracks.json")
            .map_err(|error| error.to_string())?
            .map(|doc| doc.data)
            .unwrap_or_default();
        remember(storage, &doc);
    }
    let cached = REMOVED
        .get()
        .unwrap()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if let Some(removed) = cached.as_ref().filter(|removed| removed.root == root) {
        prune_state(state, &removed.ids, &removed.paths);
    }
    Ok(())
}

fn artist_key(track: &TrackEntry) -> String {
    if !track.metadata.artist_key.is_empty() {
        return track.metadata.artist_key.clone();
    }
    let mut name = track
        .artist
        .clone()
        .unwrap_or_else(|| "Artista desconocido".into());
    if let Some(disambiguation) = &track.metadata.artist_disambiguation {
        name.push_str(&format!(" · {disambiguation}"));
    }
    metadata::key(&name)
}
fn album_key(track: &TrackEntry) -> String {
    if !track.metadata.album_key.is_empty() {
        return track.metadata.album_key.clone();
    }
    metadata::key(&format!(
        "{} · {}",
        track.album.as_deref().unwrap_or("Sin álbum"),
        track
            .album_artist
            .as_deref()
            .or(track.artist.as_deref())
            .unwrap_or("Artista desconocido")
    ))
}
fn related(entry: &Value, ids: &BTreeSet<String>, paths: &BTreeSet<String>) -> bool {
    entry
        .get("trackId")
        .or_else(|| entry.get("track_id"))
        .and_then(Value::as_str)
        .is_some_and(|id| ids.contains(id))
        || entry
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| paths.contains(path))
}

pub(crate) fn remove(
    storage: &StorageManager,
    mut doc: TracksDocument,
    kind: CollectionKind,
    key: &str,
    expected_ids: Vec<String>,
) -> Result<LibrarySnapshot, String> {
    if key.trim().is_empty() || key.len() > 16_384 || expected_ids.is_empty() {
        return Err("Selecciona un álbum o artista para eliminar".into());
    }
    let artist = match kind {
        CollectionKind::Album => None,
        CollectionKind::Artist => Some(
            doc.tracks
                .iter()
                .find(|track| artist_key(track) == key)
                .map(|track| {
                    track
                        .artist
                        .clone()
                        .unwrap_or_else(|| "Artista desconocido".into())
                })
                .or_else(|| {
                    doc.tracks
                        .iter()
                        .find(|track| {
                            track
                                .album_artist
                                .as_ref()
                                .is_some_and(|name| metadata::key(name) == key)
                        })
                        .and_then(|track| track.album_artist.clone())
                })
                .ok_or("El artista ya no existe")?,
        ),
    };
    let tracks: Vec<_> = doc
        .tracks
        .iter()
        .filter(|track| match kind {
            CollectionKind::Album => album_key(track) == key,
            CollectionKind::Artist => {
                artist_key(track) == key
                    || artist.as_ref().is_some_and(|name| {
                        metadata::key(track.album_artist.as_deref().unwrap_or_default())
                            == metadata::key(name)
                            && metadata::key(track.artist.as_deref().unwrap_or_default())
                                != metadata::key(name)
                    })
            }
        })
        .cloned()
        .collect();
    let ids: BTreeSet<_> = tracks.iter().map(|track| track.id.clone()).collect();
    if ids.is_empty() || ids != expected_ids.into_iter().collect() {
        return Err("El contenido de esta colección cambió. Cierra el aviso y revisa las canciones antes de eliminarla.".into());
    }
    let paths: BTreeSet<_> = tracks
        .iter()
        .filter_map(|track| track.local_path.clone())
        .collect();
    let before = serde_json::to_value(&doc).map_err(|error| error.to_string())?;
    doc.tracks.retain(|track| !ids.contains(&track.id));
    doc.excluded_paths.extend(paths.iter().cloned());
    doc.removed_track_ids.extend(ids.iter().cloned());
    let remaining_artists: BTreeSet<_> = doc.tracks.iter().map(artist_key).collect();
    let remaining_album_artists: BTreeSet<_> = doc
        .tracks
        .iter()
        .filter_map(|track| track.album_artist.as_deref())
        .map(metadata::key)
        .collect();
    let mut orphaned_artists: BTreeSet<_> = tracks
        .iter()
        .filter(|removed| {
            !remaining_artists.contains(&artist_key(removed))
                && !remaining_album_artists.contains(&metadata::key(
                    removed.artist.as_deref().unwrap_or_default(),
                ))
        })
        .map(artist_key)
        .collect();
    if let Some(name) = &artist
        && !doc.tracks.iter().any(|track| {
            artist_key(track) == key
                || track
                    .album_artist
                    .as_ref()
                    .is_some_and(|value| metadata::key(value) == metadata::key(name))
        })
    {
        orphaned_artists.insert(key.to_string());
    }
    let mut changes: Vec<(&str, Value, Value)> = Vec::new();
    for path in [
        "library/playlists.json",
        "library/lyrics.json",
        "library/artwork.json",
        "library/history.json",
    ] {
        let Some(stored) = storage
            .load_document::<Value>(path)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        let original = stored.data;
        let mut next = original.clone();
        match path {
            "library/playlists.json" => {
                if let Some(playlists) = next.get_mut("playlists").and_then(Value::as_array_mut) {
                    for playlist in playlists {
                        if let Some(entries) =
                            playlist.get_mut("entries").and_then(Value::as_array_mut)
                        {
                            entries.retain(|entry| !related(entry, &ids, &paths));
                        }
                    }
                }
            }
            "library/lyrics.json" | "library/artwork.json" => {
                for field in ["tracks", "selected", "apple_checked", "apple_warnings"] {
                    if let Some(map) = next.get_mut(field).and_then(Value::as_object_mut) {
                        map.retain(|id, _| !ids.contains(id));
                    }
                }
                if path == "library/artwork.json" {
                    for field in [
                        "artist_images",
                        "artist_apple_images",
                        "artist_apple_checked",
                    ] {
                        if let Some(map) = next.get_mut(field).and_then(Value::as_object_mut) {
                            map.retain(|key, _| !orphaned_artists.contains(key));
                        }
                    }
                }
            }
            "library/history.json" => {
                let mut history: crate::session::HistoryDocument =
                    serde_json::from_value(next).map_err(|error| error.to_string())?;
                history.visits.retain(|visit| {
                    !visit
                        .entry
                        .track_id
                        .as_ref()
                        .is_some_and(|id| ids.contains(id))
                        && !paths.contains(&visit.entry.path)
                });
                history.statistics.retain(|key, _| {
                    !key.strip_prefix("id:").is_some_and(|id| ids.contains(id))
                        && !key
                            .strip_prefix("path:")
                            .is_some_and(|path| paths.contains(path))
                });
                prune_state(&mut history.session, &ids, &paths);
                if let Some(saved) = &mut history.saved_queue {
                    prune_state(saved, &ids, &paths);
                }
                next = serde_json::to_value(history).map_err(|error| error.to_string())?;
            }
            _ => {}
        }
        if original != next {
            changes.push((path, original, next));
        }
    }
    changes.push((
        "library/tracks.json",
        before,
        serde_json::to_value(&doc).map_err(|error| error.to_string())?,
    ));
    // All callers hold the library operation lock. Restore prior documents on a failed write.
    for (index, (path, _, next)) in changes.iter().enumerate() {
        if let Err(error) = storage.save_document(path, next) {
            let mut failures = Vec::new();
            for (path, previous, _) in changes[..index].iter().rev() {
                if let Err(rollback) = storage.save_document(path, previous) {
                    failures.push(rollback.to_string());
                }
            }
            let suffix = if failures.is_empty() {
                String::new()
            } else {
                format!(
                    " No se pudieron restaurar algunos documentos: {}",
                    failures.join("; ")
                )
            };
            return Err(format!(
                "No se pudo completar la eliminación: {error}.{suffix}"
            ));
        }
    }
    remember(storage, &doc);
    storage.rebuild_indexes();
    Ok(library::snapshot(storage, doc, Vec::new()))
}
