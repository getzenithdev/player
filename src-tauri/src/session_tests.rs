use super::*;
use std::fs;
fn entry(name: &str) -> Entry {
    let mut e = Entry::from_path(std::path::Path::new(name));
    e.track_id = Some(name.into());
    e
}
fn state(ms: u64, position: u64) -> SessionState {
    let mut v = Visit::new(entry("a.wav"));
    v.id = "visit-one".into();
    v.listened_ms = ms;
    v.position_ms = position;
    SessionState {
        queue: vec![v.entry.clone()],
        current_id: Some(v.entry.id.clone()),
        position_ms: position,
        visit: Some(v),
        ..Default::default()
    }
}
#[test]
fn checkpoint_deltas_are_idempotent_and_seek_is_not_listening() {
    let mut doc = HistoryDocument::default();
    apply_checkpoint(&mut doc, state(5000, 150000));
    apply_checkpoint(&mut doc, state(5000, 200000));
    assert_eq!(doc.statistics["id:a.wav"].listened_ms, 5000);
    assert_eq!(doc.statistics["id:a.wav"].play_count, 0);
    apply_checkpoint(&mut doc, state(30000, 225000));
    apply_checkpoint(&mut doc, state(35000, 230000));
    assert_eq!(doc.statistics["id:a.wav"].play_count, 1);
    assert_eq!(doc.statistics["id:a.wav"].listened_ms, 35000);
    assert_eq!(doc.visits.len(), 1);
}
#[test]
fn another_visit_counts_once_and_short_listens_are_retained() {
    let mut doc = HistoryDocument::default();
    apply_checkpoint(&mut doc, state(30000, 30000));
    let mut next = state(30000, 30000);
    next.visit.as_mut().unwrap().id = "visit-two".into();
    apply_checkpoint(&mut doc, next);
    assert_eq!(doc.statistics["id:a.wav"].play_count, 2);
    assert_eq!(doc.visits.len(), 2);
    let mut short = state(1000, 1000);
    short.visit.as_mut().unwrap().id = "short".into();
    apply_checkpoint(&mut doc, short);
    assert_eq!(doc.statistics["id:a.wav"].play_count, 2);
    assert_eq!(doc.visits.len(), 3);
}
#[test]
fn saved_queue_and_auto_session_are_independent_and_survive_restart() {
    let root = std::env::temp_dir().join(format!("zenith-session-{}", playlists::new_id()));
    let storage = StorageManager::open(root.clone(), false).unwrap();
    let original = state(15000, 15000);
    checkpoint(&storage, original.clone(), true).unwrap();
    checkpoint(&storage, SessionState::default(), false).unwrap();
    let reopened = StorageManager::open(root.clone(), false).unwrap();
    assert!(restore(&reopened, false).unwrap().queue.is_empty());
    let saved = restore(&reopened, true).unwrap();
    assert_eq!(saved.queue, original.queue);
    assert_eq!(saved.position_ms, 15000);
    assert!(saved.visit.is_none());
    assert_eq!(
        load(&reopened).unwrap().statistics["id:a.wav"].listened_ms,
        15000
    );
    let _ = fs::remove_dir_all(root);
}
#[test]
fn queue_reorder_uses_stable_entry_identity_even_with_duplicate_paths() {
    let mut entries = vec![entry("a.wav"), entry("b.wav"), entry("a.wav")];
    let current = entries[0].id.clone();
    let last = entries[2].id.clone();
    playlists::move_before(&mut entries, &last, Some(&current)).unwrap();
    assert_eq!(entries[0].id, last);
    assert_eq!(entries[1].id, current);
    assert_ne!(entries[0].id, entries[1].id);
}

#[test]
fn missing_entries_are_skipped_without_reordering_or_infinite_repeat() {
    assert_eq!(
        available_index(&[0, 2], 4, 0, false, false, true, 0),
        Some(2)
    );
    assert_eq!(available_index(&[0, 2], 4, 2, false, false, true, 0), None);
    assert_eq!(
        available_index(&[0, 2], 4, 2, true, false, true, 0),
        Some(0)
    );
    assert_eq!(available_index(&[], 4, 2, true, true, true, 0), None);
}

#[test]
fn changing_and_restoring_a_queue_never_changes_its_source_playlist() {
    let root = std::env::temp_dir().join(format!("zenith-independent-{}", playlists::new_id()));
    let storage = StorageManager::open(root.clone(), false).unwrap();
    let original = vec![entry("a.wav"), entry("b.wav")];
    playlists::execute(
        &storage,
        playlists::PlaylistAction::Create {
            name: "Source".into(),
            entries: original.clone(),
        },
    )
    .unwrap();
    let before = fs::read(storage.document_path("library/playlists.json")).unwrap();
    checkpoint(
        &storage,
        SessionState {
            queue: original.clone(),
            current_id: Some(original[0].id.clone()),
            position_ms: 12345,
            ..Default::default()
        },
        true,
    )
    .unwrap();
    let mut restored = restore(&storage, true).unwrap();
    let id = restored.queue[0].id.clone();
    playlists::move_before(&mut restored.queue, &id, None).unwrap();
    restored.queue.remove(0);
    checkpoint(&storage, restored, false).unwrap();
    assert_eq!(
        fs::read(storage.document_path("library/playlists.json")).unwrap(),
        before
    );
    assert_eq!(restore(&storage, true).unwrap().queue, original);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pauses_seeks_resets_and_suspension_do_not_inflate_listening_time() {
    assert_eq!(listening_delta(0, 200, true, false), 200);
    assert_eq!(listening_delta(200, 200, true, false), 0);
    assert_eq!(listening_delta(200, 400, false, true), 0);
    assert_eq!(listening_delta(200, 100000, true, false), 0);
    assert_eq!(listening_delta(100000, 0, true, false), 0);
    assert_eq!(listening_delta(29800, 30000, true, false), 200);
}
