use super::*;
use crate::{
    library::{self, LibraryAction},
    storage::StorageManager,
};
const A: &str = "12345678-1234-1234-1234-123456789abc";
const B: &str = "abcdefab-1234-1234-1234-123456789abc";
fn track(title: &str) -> TrackEntry {
    let mut t = TrackEntry {
        title: title.into(),
        artist: Some("Same Artist".into()),
        album: Some("Album".into()),
        duration: Some(245),
        ..Default::default()
    };
    prepare(&mut t).unwrap();
    t
}
struct Fixture {
    root: std::path::PathBuf,
    storage: StorageManager,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("zenith-metadata-{}", crate::playlists::new_id()));
        let storage = StorageManager::open(root.clone(), false).unwrap();
        Self { root, storage }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn normalizes_unicode_and_whitespace_without_discarding_original_values() {
    let mut t = track("  Cafe\u{301}\tLive\n ");
    t.metadata.original = None;
    t.artist = Some("  Same\u{a0} Artist ".into());
    t.comments = Some("Do not\ncollapse this".into());
    prepare(&mut t).unwrap();
    assert_eq!(t.title, "Café Live");
    assert_eq!(t.artist.as_deref(), Some("Same Artist"));
    assert_eq!(t.metadata.original.as_ref().unwrap().title, "Café Live");
    let mut raw = TrackEntry {
        title: "  Cafe\u{301}\t Live ".into(),
        ..Default::default()
    };
    prepare(&mut raw).unwrap();
    assert_eq!(
        raw.metadata.original.as_ref().unwrap().title,
        "  Cafe\u{301}\t Live "
    );
    assert_eq!(raw.metadata.normalized.title, "café live");
    assert_eq!(t.comments.as_deref(), Some("Do not\ncollapse this"));
}
#[test]
fn legacy_missing_identity_is_assigned_once_and_survives_restart() {
    let f = Fixture::new();
    f.storage
        .save_document(
            "library/tracks.json",
            &serde_json::json!({"tracks":[{"title":"  Song  ","artist":"Artist"}]}),
        )
        .unwrap();
    let first = library::execute(&f.storage, LibraryAction::Snapshot).unwrap();
    let id = first.tracks[0].id.clone();
    assert!(!id.is_empty());
    assert_eq!(first.tracks[0].title, "Song");
    assert_eq!(first.tracks[0].metadata.version, 1);
    let reopened = StorageManager::open(f.root.clone(), false).unwrap();
    let next = library::execute(&reopened, LibraryAction::Snapshot).unwrap();
    assert_eq!(next.tracks[0].id, id);
    assert_eq!(
        next.tracks[0].metadata.original.as_ref().unwrap().title,
        "  Song  "
    );
}
#[test]
fn newer_metadata_is_not_overwritten() {
    let f = Fixture::new();
    let mut t = track("Song");
    t.metadata.version = VERSION + 1;
    f.storage
        .save_document(
            "library/tracks.json",
            &TracksDocument {
                tracks: vec![t],
                ..Default::default()
            },
        )
        .unwrap();
    let before = fs::read(f.storage.document_path("library/tracks.json")).unwrap();
    assert!(library::execute(&f.storage, LibraryAction::Rescan).is_err());
    assert_eq!(
        before,
        fs::read(f.storage.document_path("library/tracks.json")).unwrap()
    );
}
#[test]
fn similar_live_remix_deluxe_and_duration_variants_stay_distinct() {
    let standard = track("Song");
    for title in ["Song (Live)", "Song (Remix)", "Song (Acoustic)"] {
        assert_eq!(compare(&standard, &track(title)).kind, MatchKind::Different);
    }
    assert_eq!(
        compare(&track("Song (Live Paris)"), &track("Song (Live London)")).kind,
        MatchKind::Different
    );
    let mut long = standard.clone();
    long.duration = Some(300);
    assert_eq!(compare(&standard, &long).kind, MatchKind::Different);
    let mut deluxe = standard.clone();
    deluxe.album = Some("Album (Deluxe)".into());
    prepare(&mut deluxe).unwrap();
    assert_ne!(deluxe.metadata.album_key, standard.metadata.album_key);
    assert_eq!(compare(&standard, &deluxe).kind, MatchKind::Different);
}
#[test]
fn duration_tolerance_proposes_review_but_never_reuses_an_internal_id() {
    let a = track("Song");
    let mut b = track("Song");
    b.duration = Some(247);
    assert_eq!(compare(&a, &b).kind, MatchKind::PossibleRecording);
    assert_ne!(a.id, b.id);
    b.duration = Some(248);
    assert_eq!(compare(&a, &b).kind, MatchKind::Different);
}
#[test]
fn same_name_artists_with_distinct_ids_have_distinct_groups() {
    let mut a = track("Song");
    let mut b = track("Song");
    a.metadata.identifiers.musicbrainz_artist_id = Some(A.into());
    b.metadata.identifiers.musicbrainz_artist_id = Some(B.into());
    prepare(&mut a).unwrap();
    prepare(&mut b).unwrap();
    assert_ne!(a.metadata.artist_key, b.metadata.artist_key);
    assert_ne!(a.metadata.album_key, b.metadata.album_key);
    assert_eq!(compare(&a, &b).kind, MatchKind::Different);
    assert_eq!(analyze(&[a, b]).artist_collisions.len(), 1);
}
#[test]
fn provider_ids_are_namespaced_and_not_assumed_equivalent() {
    let mut a = track("Song");
    let mut b = track("Song");
    a.metadata
        .identifiers
        .provider_artist_ids
        .insert("apple_music".into(), "123".into());
    b.metadata
        .identifiers
        .provider_artist_ids
        .insert("youtube".into(), "123".into());
    prepare(&mut a).unwrap();
    prepare(&mut b).unwrap();
    assert_ne!(a.metadata.artist_key, b.metadata.artist_key);
    assert_eq!(compare(&a, &b).kind, MatchKind::PossibleRecording);
    b.metadata
        .identifiers
        .provider_artist_ids
        .insert("apple_music".into(), "456".into());
    prepare(&mut b).unwrap();
    assert_eq!(compare(&a, &b).kind, MatchKind::Different);
}
#[test]
fn retains_raw_identifiers_and_only_uses_valid_values_for_matching() {
    let mut a = track("Song");
    let mut b = track("Song");
    a.metadata.identifiers.musicbrainz_recording_id = Some(A.to_uppercase());
    b.metadata.identifiers.musicbrainz_recording_id = Some(A.into());
    assert_eq!(compare(&a, &b).kind, MatchKind::SharedRecording);
    a.metadata.identifiers.musicbrainz_recording_id = Some("invalid/raw".into());
    assert_eq!(compare(&a, &b).kind, MatchKind::PossibleRecording);
    a.metadata.identifiers.isrc = Some("us-abc-12-34567".into());
    b.metadata.identifiers.isrc = Some("USABC1234567".into());
    assert_eq!(compare(&a, &b).kind, MatchKind::SharedRecording);
    assert_eq!(
        a.metadata.identifiers.musicbrainz_recording_id.as_deref(),
        Some("invalid/raw")
    );
    b.duration = Some(320);
    assert_eq!(compare(&a, &b).kind, MatchKind::Different);
}
#[test]
fn duplicate_album_candidates_are_reported_without_merging_releases() {
    let mut a = track("One");
    let mut b = track("Two");
    a.metadata.identifiers.musicbrainz_release_id = Some(A.into());
    b.metadata.identifiers.musicbrainz_release_id = Some(B.into());
    prepare(&mut a).unwrap();
    prepare(&mut b).unwrap();
    let report = analyze(&[a.clone(), b.clone()]);
    assert_eq!(report.album_candidates.len(), 1);
    assert_eq!(report.album_candidates[0].album_keys.len(), 2);
    assert_ne!(a.id, b.id);
}
#[test]
fn cloud_tracks_need_no_path_and_refresh_preserves_user_state() {
    let mut doc = TracksDocument::default();
    let input = IncomingTrack {
        source: "apple_music".into(),
        source_id: Some("123".into()),
        title: " Song ".into(),
        artist: Some(" Artist ".into()),
        identifiers: ExternalIds {
            musicbrainz_recording_id: Some(A.into()),
            ..Default::default()
        },
        provider_data: BTreeMap::from([("territory".into(), serde_json::json!("MX"))]),
        ..Default::default()
    };
    let id = upsert(&mut doc, input.clone()).unwrap();
    doc.tracks[0].favorite = true;
    doc.tracks[0].play_count = 9;
    doc.tracks[0].title = "My edit".into();
    doc.tracks[0].edited = true;
    let mut refresh = input.clone();
    refresh.title = "New source title".into();
    refresh.identifiers = ExternalIds::default();
    assert_eq!(upsert(&mut doc, refresh).unwrap(), id);
    assert_eq!(doc.tracks.len(), 1);
    assert!(doc.tracks[0].local_path.is_none());
    assert!(doc.tracks[0].favorite);
    assert_eq!(doc.tracks[0].title, "My edit");
    assert_eq!(
        doc.tracks[0].metadata.original.as_ref().unwrap().title,
        "New source title"
    );
    assert_eq!(
        doc.tracks[0]
            .metadata
            .identifiers
            .musicbrainz_recording_id
            .as_deref(),
        Some(A)
    );
    let mut other = input;
    other.source = "youtube".into();
    assert_ne!(upsert(&mut doc, other).unwrap(), id);
}
#[test]
fn incomplete_entries_are_retained_and_unknown_album_groups_do_not_claim_identity() {
    let mut doc = TracksDocument::default();
    let id = upsert(
        &mut doc,
        IncomingTrack {
            source: "custom_provider".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let next = upsert(
        &mut doc,
        IncomingTrack {
            source: "custom_provider".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(id, next);
    assert_eq!(doc.tracks.len(), 2);
    assert_ne!(
        doc.tracks[0].metadata.album_key,
        doc.tracks[1].metadata.album_key
    );
    assert_eq!(
        compare(&doc.tracks[0], &doc.tracks[1]).kind,
        MatchKind::Insufficient
    );
}
#[test]
fn import_is_atomic_and_serialization_preserves_provider_metadata() {
    let f = Fixture::new();
    let path = f.root.join("metadata.json");
    let mut doc = TracksDocument::default();
    let json = serde_json::json!({"format":"zenith-metadata","version":1,"tracks":[{"source":"youtube","source_id":"one","title":"Song","identifiers":{"provider_track_ids":{"apple_music":"123"}},"provider_data":{"nested":{"locale":"es-MX"}}}]});
    fs::write(&path, json.to_string()).unwrap();
    import_file(&mut doc, &path).unwrap();
    let decoded: TracksDocument =
        serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(decoded, doc);
    let before = doc.clone();
    fs::write(&path,serde_json::json!({"format":"zenith-metadata","version":1,"tracks":[{"source":"youtube","source_id":"two"},{"source":"INVALID"}]}).to_string()).unwrap();
    assert!(import_file(&mut doc, &path).is_err());
    assert_eq!(doc, before);
}

#[test]
fn v1_envelope_upgrades_to_v2_with_a_recoverable_original_backup() {
    let f = Fixture::new();
    let path = f.storage.document_path("library/tracks.json");
    let raw=serde_json::json!({"version":1,"updated_at":1,"data":{"tracks":[{"id":"existing-id","title":" Song ","favorite":true}]}}).to_string();
    fs::write(&path, &raw).unwrap();
    let result = library::execute(&f.storage, LibraryAction::Snapshot).unwrap();
    assert_eq!(result.tracks[0].id, "existing-id");
    assert!(result.tracks[0].favorite);
    let stored: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(stored["version"], 2);
    assert!(
        f.storage
            .status()
            .documents
            .iter()
            .find(|d| d.path == "library/tracks.json")
            .is_some_and(|d| d.backups > 0)
    );
}

#[test]
fn reads_embedded_musicbrainz_and_isrc_from_real_audio_tags() {
    use lofty::{
        file::{AudioFile, TaggedFileExt},
        tag::{Accessor, ItemKey, Tag, TagType},
    };
    let f = Fixture::new();
    let path = f.root.join("tagged.wav");
    let mut bytes = Vec::new();
    let size = 16000u32;
    bytes.extend(b"RIFF");
    bytes.extend((size + 36).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(8000u32.to_le_bytes());
    bytes.extend(16000u32.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(size.to_le_bytes());
    bytes.resize(44 + size as usize, 0);
    fs::write(&path, bytes).unwrap();
    let mut audio = lofty::read_from_path(&path).unwrap();
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_title("  Embedded  ".into());
    tag.insert_text(ItemKey::MusicBrainzRecordingId, A.into());
    tag.insert_text(ItemKey::MusicBrainzReleaseId, B.into());
    tag.insert_text(ItemKey::Isrc, "USABC1234567".into());
    audio.insert_tag(tag);
    audio
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    let result = library::execute(
        &f.storage,
        LibraryAction::Import {
            paths: vec![path.to_string_lossy().into_owned()],
        },
    )
    .unwrap();
    assert_eq!(result.tracks[0].title, "Embedded");
    assert_eq!(
        result.tracks[0]
            .metadata
            .identifiers
            .musicbrainz_recording_id
            .as_deref(),
        Some(A)
    );
    assert_eq!(f.storage.search(A).len(), 1);
    assert_eq!(
        result.tracks[0]
            .metadata
            .identifiers
            .musicbrainz_release_id
            .as_deref(),
        Some(B)
    );
    assert_eq!(
        result.tracks[0].metadata.identifiers.isrc.as_deref(),
        Some("USABC1234567")
    );
}
