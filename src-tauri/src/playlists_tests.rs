use super::*;
struct Fixture {
    root: PathBuf,
    storage: StorageManager,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("zenith-playlists-{}", new_id()));
        fs::create_dir_all(&root).unwrap();
        Self {
            storage: StorageManager::open(root.join("data"), false).unwrap(),
            root,
        }
    }
    fn entry(&self, name: &str) -> Entry {
        Entry::from_path(&self.root.join(name))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn crud_duplicates_and_order_survive_restart() {
    let f = Fixture::new();
    let entries = vec![f.entry("a.wav"), f.entry("b.wav"), f.entry("a.wav")];
    let p = execute(
        &f.storage,
        PlaylistAction::Create {
            name: "List".into(),
            entries,
        },
    )
    .unwrap()
    .playlists
    .remove(0);
    assert_ne!(p.entries[0].id, p.entries[2].id);
    let id = p.id.clone();
    execute(
        &f.storage,
        PlaylistAction::Rename {
            id: id.clone(),
            name: "Renamed".into(),
        },
    )
    .unwrap();
    execute(
        &f.storage,
        PlaylistAction::Move {
            id: id.clone(),
            entry_id: p.entries[2].id.clone(),
            before_id: Some(p.entries[0].id.clone()),
        },
    )
    .unwrap();
    let snapshot = execute(&f.storage, PlaylistAction::Duplicate { id: id.clone() }).unwrap();
    assert_eq!(snapshot.playlists.len(), 2);
    assert_ne!(
        snapshot.playlists[0].entries[0].id,
        snapshot.playlists[1].entries[0].id
    );
    execute(
        &f.storage,
        PlaylistAction::Remove {
            id: id.clone(),
            entry_id: p.entries[1].id.clone(),
        },
    )
    .unwrap();
    let reopened = StorageManager::open(f.root.join("data"), false).unwrap();
    let stored = execute(&reopened, PlaylistAction::Snapshot).unwrap();
    assert_eq!(stored.playlists[0].name, "Renamed");
    assert_eq!(stored.playlists[0].entries.len(), 2);
    assert_eq!(stored.playlists[0].entries[0].id, p.entries[2].id);
    execute(&reopened, PlaylistAction::Delete { id }).unwrap();
    assert_eq!(load(&reopened).unwrap().playlists.len(), 1);
}
#[test]
fn exchange_preserves_order_duplicates_unicode_and_missing_files() {
    let f = Fixture::new();
    let p = Playlist {
        id: new_id(),
        name: "Español".into(),
        entries: vec![
            f.entry("música.wav"),
            f.entry("missing.wav"),
            f.entry("música.wav"),
        ],
        updated_at: 0,
    };
    for format in ["m3u", "m3u8", "json"] {
        let path = f.root.join(format!("list.{format}"));
        export_file(&p, &path, format).unwrap();
        let restored = import_file(&path).unwrap();
        assert_eq!(
            restored.entries.iter().map(|e| &e.path).collect::<Vec<_>>(),
            p.entries.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
        assert_ne!(restored.entries[0].id, restored.entries[2].id);
    }
}
#[test]
fn bom_relative_paths_crlf_and_extinf_are_supported() {
    let f = Fixture::new();
    let path = f.root.join("list.m3u8");
    fs::write(
        &path,
        "\u{feff}#EXTM3U\r\n#EXTINF:123,Canción\r\n./folder/../song.wav\r\n",
    )
    .unwrap();
    let p = import_file(&path).unwrap();
    assert_eq!(p.entries[0].path, f.root.join("song.wav").to_string_lossy());
    assert_eq!(p.entries[0].title, "Canción");
    assert_eq!(p.entries[0].duration_ms, Some(123000));
}
#[test]
fn invalid_imports_and_stale_moves_do_not_mutate_a_saved_list() {
    let f = Fixture::new();
    execute(
        &f.storage,
        PlaylistAction::Create {
            name: "Safe".into(),
            entries: vec![f.entry("a.wav")],
        },
    )
    .unwrap();
    let original = load(&f.storage).unwrap();
    let p = &original.playlists[0];
    assert!(
        execute(
            &f.storage,
            PlaylistAction::Move {
                id: p.id.clone(),
                entry_id: p.entries[0].id.clone(),
                before_id: Some("gone".into())
            }
        )
        .is_err()
    );
    let path = f.root.join("invalid.m3u");
    fs::write(&path, "https://example.com/audio.mp3").unwrap();
    assert!(
        execute(
            &f.storage,
            PlaylistAction::Import {
                path: path.to_string_lossy().into_owned()
            }
        )
        .is_err()
    );
    assert!(load(&f.storage).unwrap() == original);
}
#[test]
fn moved_library_entries_resolve_to_the_new_path_without_rewriting_source_lists() {
    let f = Fixture::new();
    let original = f.entry("old.wav");
    let p = execute(
        &f.storage,
        PlaylistAction::Create {
            name: "List".into(),
            entries: vec![original],
        },
    )
    .unwrap()
    .playlists
    .remove(0);
    let mut doc = load(&f.storage).unwrap();
    doc.playlists[0].entries[0].track_id = Some("stable".into());
    f.storage.save_document(DOCUMENT, &doc).unwrap();
    let tracks = TracksDocument {
        tracks: vec![crate::storage::indexes::TrackEntry {
            id: "stable".into(),
            title: "Moved".into(),
            local_path: Some(f.root.join("new.wav").to_string_lossy().into_owned()),
            ..Default::default()
        }],
        ..Default::default()
    };
    f.storage
        .save_document("library/tracks.json", &tracks)
        .unwrap();
    let result = execute(&f.storage, PlaylistAction::Snapshot).unwrap();
    assert_eq!(result.playlists[0].entries[0].title, "Moved");
    assert_eq!(result.playlists[0].id, p.id);
    assert_eq!(
        load(&f.storage).unwrap().playlists[0].entries[0].title,
        "old"
    );
}
