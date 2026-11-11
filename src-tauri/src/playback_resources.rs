//! Resolve resource metadata for a stream without importing it into the library.
use crate::{audio::AudioEngine, metadata, storage::indexes::TrackEntry};
use tauri::Manager;

pub fn stream_track(app: &tauri::AppHandle, reference: &str) -> Result<TrackEntry, String> {
    let (provider, id) = crate::sources::parse_stream_reference(reference)
        .ok_or("Referencia de reproducción inválida")?;
    let snapshot = app.state::<AudioEngine>().snapshot();
    let entry = snapshot
        .queue
        .iter()
        .find(|entry| entry.path == reference)
        .ok_or("La canción ya no está en la cola de reproducción")?;
    let mut track = TrackEntry {
        id: reference.into(),
        title: entry.title.clone(),
        artist: entry.artist.clone(),
        album: entry.album.clone(),
        source: Some(provider.key().into()),
        source_id: Some(id.clone()),
        duration: entry.duration_ms.map(|value| value / 1000),
        format: entry.format.clone(),
        ..Default::default()
    };
    track
        .metadata
        .identifiers
        .provider_track_ids
        .insert(provider.key().into(), id);
    metadata::prepare(&mut track)?;
    Ok(track)
}
