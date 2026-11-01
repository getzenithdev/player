use super::*;
struct Fixture {
    root: PathBuf,
    music: PathBuf,
    storage: StorageManager,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("zenith-library-{:032x}", rand::random::<u128>()));
        let music = root.join("music");
        fs::create_dir_all(&music).unwrap();
        let storage = StorageManager::open(root.join("data"), false).unwrap();
        Self {
            root,
            music,
            storage,
        }
    }
    fn wav(&self, name: &str) -> PathBuf {
        let path = self.music.join(name);
        // One second of mono PCM; fixtures need no network or audio device.
        let mut data = Vec::new();
        let size = 16_000u32;
        data.extend(b"RIFF");
        data.extend((size + 36).to_le_bytes());
        data.extend(b"WAVEfmt ");
        data.extend(16u32.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(8000u32.to_le_bytes());
        data.extend(16000u32.to_le_bytes());
        data.extend(2u16.to_le_bytes());
        data.extend(16u16.to_le_bytes());
        data.extend(b"data");
        data.extend(size.to_le_bytes());
        data.resize(44 + size as usize, 0);
        fs::write(&path, data).unwrap();
        path
    }
    fn import(&self) -> LibrarySnapshot {
        execute(
            &self.storage,
            LibraryAction::Import {
                paths: vec![self.music.to_string_lossy().into_owned()],
            },
        )
        .unwrap()
    }
    fn scan(&self) -> LibrarySnapshot {
        execute(&self.storage, LibraryAction::Rescan).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn edit() -> MetadataEdit {
    MetadataEdit {
        title: "Edited title".into(),
        artist: Some("Artist".into()),
        album_artist: Some("Album artist".into()),
        album: Some("Album".into()),
        genre: Some("Rock".into()),
        year: Some(2026),
        track_number: Some(2),
        disc_number: Some(1),
        comments: Some("My comment".into()),
    }
}
#[test]
fn repeated_and_overlapping_imports_are_idempotent() {
    let f = Fixture::new();
    let path = f.wav("song.wav");
    let first = f.import();
    let second = f.import();
    assert_eq!(first.tracks, second.tracks);
    assert_eq!(second.locations.len(), 1);
    let result = execute(
        &f.storage,
        LibraryAction::Import {
            paths: vec![path.to_string_lossy().into_owned()],
        },
    )
    .unwrap();
    assert_eq!(result.tracks.len(), 1);
    assert_eq!(result.tracks[0].duration, Some(1));
    assert_eq!(f.storage.search("song").len(), 1);
}
#[test]
fn moved_missing_and_restored_files_keep_identity_and_user_data() {
    let f = Fixture::new();
    let path = f.wav("song.wav");
    let first = f.import();
    let id = first.tracks[0].id.clone();
    execute(
        &f.storage,
        LibraryAction::Favorite {
            id: id.clone(),
            favorite: true,
        },
    )
    .unwrap();
    record_play(&f.storage, &path.to_string_lossy()).unwrap();
    let moved = f.music.join("renamed.wav");
    fs::rename(&path, &moved).unwrap();
    let result = f.scan();
    assert_eq!(result.tracks.len(), 1);
    assert_eq!(result.tracks[0].id, id);
    assert!(result.tracks[0].favorite);
    assert_eq!(result.tracks[0].play_count, 1);
    let outside = f.root.join("outside.wav");
    fs::rename(&moved, &outside).unwrap();
    assert!(f.scan().tracks[0].missing);
    fs::rename(outside, &moved).unwrap();
    assert!(!f.scan().tracks[0].missing);
}
#[test]
fn identical_copies_are_reported_and_never_merged() {
    let f = Fixture::new();
    let a = f.wav("a.wav");
    fs::copy(a, f.music.join("b.wav")).unwrap();
    let result = f.import();
    assert_eq!(result.tracks.len(), 2);
    assert_ne!(result.tracks[0].id, result.tracks[1].id);
    assert_eq!(result.duplicates.len(), 1);
    assert!(result.duplicates[0].confirmed);
}
#[test]
fn ambiguous_missing_duplicates_do_not_steal_an_identity() {
    let f = Fixture::new();
    let a = f.wav("a.wav");
    let b = f.music.join("b.wav");
    fs::copy(&a, &b).unwrap();
    f.import();
    fs::remove_file(a).unwrap();
    fs::rename(b, f.music.join("c.wav")).unwrap();
    let result = f.scan();
    assert_eq!(result.tracks.len(), 3);
    assert_eq!(result.tracks.iter().filter(|t| t.missing).count(), 2);
}
#[test]
fn corrupt_files_do_not_interrupt_good_files_and_can_be_retried() {
    let f = Fixture::new();
    let bad = f.music.join("broken.mp3");
    fs::write(&bad, b"not audio").unwrap();
    f.wav("good.wav");
    let result = f.import();
    assert_eq!(result.tracks.len(), 1);
    assert!(!result.warnings.is_empty());
    fs::remove_file(bad).unwrap();
    f.wav("repaired.wav");
    assert_eq!(f.scan().tracks.len(), 2);
}
#[test]
fn internal_edits_survive_rescan_and_restart_without_writing_music() {
    let f = Fixture::new();
    let path = f.wav("song.wav");
    let bytes = fs::read(&path).unwrap();
    let id = f.import().tracks[0].id.clone();
    execute(
        &f.storage,
        LibraryAction::Edit {
            id,
            metadata: edit(),
            write_tags: false,
        },
    )
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    // Force an external file modification to exercise overlay preservation.
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&[0, 0])
        .unwrap();
    assert_eq!(f.scan().tracks[0].title, "Edited title");
    let reopened = StorageManager::open(f.root.join("data"), false).unwrap();
    let result = execute(&reopened, LibraryAction::Snapshot).unwrap();
    assert_eq!(
        result.tracks[0].album_artist.as_deref(),
        Some("Album artist")
    );
    assert_eq!(result.locations.len(), 1);
}
#[test]
fn explicit_tag_writes_preserve_a_recovery_copy_and_read_back_metadata() {
    let f = Fixture::new();
    let path = f.wav("song.wav");
    let bytes = fs::read(&path).unwrap();
    let id = f.import().tracks[0].id.clone();
    execute(
        &f.storage,
        LibraryAction::Edit {
            id,
            metadata: edit(),
            write_tags: true,
        },
    )
    .unwrap();
    assert_ne!(fs::read(&path).unwrap(), bytes);
    let file = lofty::read_from_path(&path).unwrap();
    let tag = file.primary_tag().unwrap();
    assert_eq!(tag.title().as_deref(), Some("Edited title"));
    assert_eq!(tag.artist().as_deref(), Some("Artist"));
    assert_eq!(tag.track(), Some(2));
    assert_eq!(tag.date().map(|d| d.year), Some(2026));
    let backup = fs::read_dir(&f.music)
        .unwrap()
        .flatten()
        .find(|e| e.path().extension().is_some_and(|e| e == "bak"))
        .unwrap();
    assert_eq!(fs::read(backup.path()).unwrap(), bytes);
    assert_eq!(f.scan().tracks.len(), 1);
}
#[test]
fn removing_a_location_does_not_delete_music_and_respects_overlap() {
    let f = Fixture::new();
    let path = f.wav("song.wav");
    f.import();
    execute(
        &f.storage,
        LibraryAction::Import {
            paths: vec![path.to_string_lossy().into_owned()],
        },
    )
    .unwrap();
    let result = execute(
        &f.storage,
        LibraryAction::RemoveLocation {
            path: f.music.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    assert_eq!(result.tracks.len(), 1);
    let result = execute(
        &f.storage,
        LibraryAction::RemoveLocation {
            path: path.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    assert!(result.tracks.is_empty());
    assert!(path.exists());
}
#[test]
fn simultaneous_rescans_and_edits_do_not_lose_updates() {
    let f = Fixture::new();
    f.wav("song.wav");
    let id = f.import().tracks[0].id.clone();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..5 {
                f.scan();
            }
        });
        scope.spawn(|| {
            execute(&f.storage, LibraryAction::Favorite { id, favorite: true }).unwrap();
        });
    });
    assert!(f.scan().tracks[0].favorite);
}
#[cfg(unix)]
#[test]
fn nested_symlinks_do_not_create_cycles_or_duplicate_entries() {
    let f = Fixture::new();
    f.wav("song.wav");
    std::os::unix::fs::symlink(&f.music, f.music.join("loop")).unwrap();
    assert_eq!(f.import().tracks.len(), 1);
}

#[test]
fn probes_content_and_reads_embedded_metadata_and_artwork() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use lofty::{
        picture::{MimeType, Picture, PictureType},
        tag::TagType,
    };
    let f = Fixture::new();
    let path = f.wav("tagged.wav");
    let png=STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a1GkAAAAASUVORK5CYII=").unwrap();
    let mut file = probe_file(&path).unwrap();
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_title("Embedded title".into());
    tag.set_artist("Embedded artist".into());
    tag.set_album("Embedded album".into());
    tag.set_track(7);
    tag.push_picture(
        Picture::unchecked(png.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build(),
    );
    file.insert_tag(tag);
    file.save_to_path(&path, WriteOptions::default()).unwrap();
    // The misleading suffix must not force an MP3 decoder on WAV contents.
    let misleading = f.music.join("tagged.mp3");
    fs::rename(path, misleading).unwrap();
    let result = f.import();
    assert_eq!(result.tracks.len(), 1);
    let t = &result.tracks[0];
    assert_eq!(t.title, "Embedded title");
    assert_eq!(t.artist.as_deref(), Some("Embedded artist"));
    assert_eq!(t.track_number, Some(7));
    assert_eq!(t.format, "Wav");
    let artwork = t.artwork_id.as_ref().unwrap();
    assert_eq!(
        fs::read(f.storage.document_path(&format!("artwork/local/{artwork}"))).unwrap(),
        png
    );
}

#[test]
fn external_tag_updates_refresh_unedited_tracks() {
    let f = Fixture::new();
    let path = f.wav("song.wav");
    let first = f.import();
    write_metadata(&path, &edit()).unwrap();
    let updated = f.scan();
    assert_eq!(updated.tracks[0].id, first.tracks[0].id);
    assert_eq!(updated.tracks[0].title, "Edited title");
    assert_eq!(updated.tracks[0].genre.as_deref(), Some("Rock"));
}
