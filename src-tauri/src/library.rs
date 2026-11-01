//! Local library. All mutations share StorageManager's operation lock and commit
//! tracks + locations + user metadata as a single versioned atomic document.
use crate::storage::{
    StorageManager,
    indexes::{TrackEntry, TracksDocument},
};
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    tag::{Accessor, ItemKey, Tag},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const DOCUMENT: &str = "library/tracks.json";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MetadataEdit {
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub comments: Option<String>,
}
impl MetadataEdit {
    fn apply(&self, t: &mut TrackEntry) {
        t.title = self.title.trim().into();
        t.artist = self.artist.clone();
        t.album_artist = self.album_artist.clone();
        t.album = self.album.clone();
        t.genre = self.genre.clone();
        t.year = self.year;
        t.track_number = self.track_number;
        t.disc_number = self.disc_number;
        t.comments = self.comments.clone();
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LibraryAction {
    Snapshot,
    Import {
        paths: Vec<String>,
    },
    RemoveLocation {
        path: String,
    },
    Rescan,
    ImportMetadata {
        path: String,
    },
    Edit {
        id: String,
        metadata: MetadataEdit,
        #[serde(default)]
        write_tags: bool,
    },
    Favorite {
        id: String,
        favorite: bool,
    },
}
#[derive(Serialize)]
pub struct DuplicateGroup {
    pub ids: Vec<String>,
    pub confirmed: bool,
    pub reason: String,
}
#[derive(Serialize)]
pub struct LibrarySnapshot {
    pub tracks: Vec<TrackEntry>,
    pub locations: Vec<String>,
    pub duplicates: Vec<DuplicateGroup>,
    pub warnings: Vec<String>,
    pub metadata_analysis: crate::metadata::Analysis,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub(crate) fn load(storage: &StorageManager) -> Result<TracksDocument, String> {
    let mut doc = storage
        .load_document::<TracksDocument>(DOCUMENT)
        .map(|doc| doc.map(|d| d.data).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    if crate::metadata::prepare_document(&mut doc)? {
        storage
            .save_document(DOCUMENT, &doc)
            .map_err(|e| e.to_string())?;
        storage.rebuild_indexes();
    }
    Ok(doc)
}
pub(crate) fn save(storage: &StorageManager, doc: &TracksDocument) -> Result<(), String> {
    storage
        .save_document(DOCUMENT, doc)
        .map_err(|e| e.to_string())?;
    storage.rebuild_indexes();
    Ok(())
}
fn snapshot(
    storage: &StorageManager,
    mut doc: TracksDocument,
    mut warnings: Vec<String>,
) -> LibrarySnapshot {
    if let Err(error) = crate::session::overlay(storage, &mut doc.tracks) {
        warnings.push(error);
    }
    let mut hash_groups: HashMap<String, Vec<String>> = HashMap::new();
    let mut candidates: HashMap<String, Vec<&TrackEntry>> = HashMap::new();
    for t in doc.tracks.iter().filter(|t| !t.missing) {
        if !t.hash.is_empty() {
            hash_groups
                .entry(t.hash.clone())
                .or_default()
                .push(t.id.clone());
        }
        let n = &t.metadata.normalized;
        if !n.title.is_empty() && !n.artist.is_empty() && t.duration.is_some() {
            let signature = serde_json::to_string(&(
                &n.title,
                &t.metadata.artist_key,
                &t.metadata.qualifiers,
                crate::metadata::key(t.metadata.track_version.as_deref().unwrap_or_default()),
                crate::metadata::key(t.metadata.remixer.as_deref().unwrap_or_default()),
            ))
            .unwrap_or_default();
            candidates.entry(signature).or_default().push(t);
        }
    }
    let mut duplicates: Vec<_> = hash_groups
        .into_values()
        .filter(|ids| ids.len() > 1)
        .map(|ids| DuplicateGroup {
            ids,
            confirmed: true,
            reason: "Contenido completo idéntico (SHA-256)".into(),
        })
        .collect();
    for mut group in candidates.into_values() {
        group.sort_by_key(|t| t.duration);
        let mut start = 0;
        while start < group.len() {
            let mut end = start + 1;
            while end < group.len()
                && matches!(
                    crate::metadata::compare(group[start], group[end]).kind,
                    crate::metadata::MatchKind::SharedRecording
                        | crate::metadata::MatchKind::PossibleRecording
                )
            {
                end += 1;
            }
            if end - start > 1 {
                let ids: Vec<_> = group[start..end].iter().map(|t| t.id.clone()).collect();
                if !duplicates
                    .iter()
                    .any(|g| ids.iter().all(|id| g.ids.contains(id)))
                {
                    duplicates.push(DuplicateGroup {
                        ids,
                        confirmed: false,
                        reason: "Metadatos y duración compatibles; no se fusionaron entradas"
                            .into(),
                    });
                }
            }
            start = end;
        }
    }
    let metadata_analysis = crate::metadata::analyze(&doc.tracks);
    LibrarySnapshot {
        tracks: doc.tracks,
        locations: doc.locations,
        duplicates,
        warnings,
        metadata_analysis,
    }
}
pub fn execute(storage: &StorageManager, action: LibraryAction) -> Result<LibrarySnapshot, String> {
    let _operation = storage.library_lock();
    let mut doc = load(storage)?;
    let mut warnings = Vec::new();
    let before = doc.clone();
    match action {
        LibraryAction::Snapshot => {}
        LibraryAction::Import { paths } => {
            for path in paths {
                match fs::canonicalize(&path) {
                    Ok(p) if p.is_dir() || p.is_file() => {
                        let value = p.to_string_lossy().into_owned();
                        if !doc.locations.contains(&value) {
                            doc.locations.push(value);
                        }
                    }
                    _ => warnings.push(format!("No se pudo abrir {path}")),
                }
            }
            reconcile(storage, &mut doc, &mut warnings);
        }
        LibraryAction::RemoveLocation { path } => {
            doc.locations.retain(|p| p != &path);
            doc.tracks.retain(|t| {
                t.source.as_deref() != Some("local")
                    || t.local_path
                        .as_ref()
                        .is_some_and(|p| covered(Path::new(p), &doc.locations))
            });
        }
        LibraryAction::Rescan => reconcile(storage, &mut doc, &mut warnings),
        LibraryAction::ImportMetadata { path } => {
            crate::metadata::import_file(&mut doc, Path::new(&path))?
        }
        LibraryAction::Favorite { id, favorite } => {
            let track = doc
                .tracks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or("La pista ya no existe")?;
            track.favorite = favorite;
        }
        LibraryAction::Edit {
            id,
            metadata,
            write_tags,
        } => {
            if metadata.title.trim().is_empty() {
                return Err("El título es obligatorio".into());
            }
            if metadata.year.is_some_and(|year| year > 65535) {
                return Err("El año debe ser menor a 65536".into());
            }
            if [
                Some(metadata.title.as_str()),
                metadata.artist.as_deref(),
                metadata.album_artist.as_deref(),
                metadata.album.as_deref(),
                metadata.genre.as_deref(),
                metadata.comments.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|v| v.len() > 16_384)
            {
                return Err("Metadatos demasiado largos".into());
            }
            let track = doc
                .tracks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or("La pista ya no existe")?;
            if write_tags {
                let path = track.local_path.as_ref().ok_or("Sin archivo local")?;
                write_metadata(Path::new(path), &metadata)?;
                // A tag rewrite changes file size/hash. Reconcile immediately so
                // subsequent moves can still resolve the stable internal ID.
                track.modified_ns = 0;
            }
            metadata.apply(track);
            track.edited = true;
            if write_tags {
                reconcile(storage, &mut doc, &mut warnings);
            }
        }
    }
    crate::metadata::prepare_document(&mut doc)?;
    if doc != before {
        save(storage, &doc)?;
    }
    Ok(snapshot(storage, doc, warnings))
}
fn covered(path: &Path, locations: &[String]) -> bool {
    locations
        .iter()
        .any(|root| path == Path::new(root) || path.starts_with(Path::new(root)))
}
fn supported(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "mp3"
                | "flac"
                | "wav"
                | "ogg"
                | "oga"
                | "opus"
                | "aiff"
                | "aif"
                | "aifc"
                | "m4a"
                | "mp4"
                | "aac"
                | "alac"
                | "caf"
        )
    })
}
fn collect(path: &Path, files: &mut BTreeSet<PathBuf>, warnings: &mut Vec<String>) {
    // Never follow nested symlinks: overlapping roots are deduplicated and
    // symlink loops cannot escape the selected location or hang the scanner.
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            warnings.push(format!("{}: {e}", path.display()));
            return;
        }
    };
    if meta.file_type().is_symlink() {
        return;
    }
    if meta.is_file() {
        if supported(path) {
            files.insert(path.to_path_buf());
        }
        return;
    }
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(e) => collect(&e.path(), files, warnings),
                    Err(e) => warnings.push(e.to_string()),
                }
            }
        }
        Err(e) => warnings.push(format!("{}: {e}", path.display())),
    }
}
fn stamp(meta: &fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or_default()
}
fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65_536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(crate) fn reconcile(
    storage: &StorageManager,
    doc: &mut TracksDocument,
    warnings: &mut Vec<String>,
) {
    let mut files = BTreeSet::new();
    for root in &doc.locations {
        collect(Path::new(root), &mut files, warnings);
    }
    // Keep unavailable tracks (and all user data) instead of deleting them.
    // Permission errors are reported and never interpreted as user removal.
    for t in &mut doc.tracks {
        if t.source.as_deref() == Some("local") {
            t.missing = t
                .local_path
                .as_ref()
                .is_none_or(|p| !Path::new(p).is_file());
        }
    }
    for path in files {
        let result = (|| -> Result<(), String> {
            let text = path.to_string_lossy().into_owned();
            let stat = fs::metadata(&path).map_err(|e| e.to_string())?;
            let existing = doc
                .tracks
                .iter()
                .position(|t| t.local_path.as_deref() == Some(&text));
            if existing.is_some_and(|i| {
                doc.tracks[i].size == stat.len()
                    && doc.tracks[i].modified_ns == stamp(&stat)
                    && doc.tracks[i]
                        .metadata
                        .provider_data
                        .get("local_tags_version")
                        == Some(&serde_json::json!(1))
            }) {
                return Ok(());
            }
            let hash = hash_file(&path)?;
            // Only a single missing file with the same full hash is a move.
            // Identical copies present together keep separate identities.
            let moved: Vec<_> = doc
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.missing && t.hash == hash)
                .map(|(i, _)| i)
                .collect();
            let index = existing.or_else(|| {
                if moved.len() == 1 {
                    Some(moved[0])
                } else {
                    None
                }
            });
            let mut fresh = read_track(storage, &path)?;
            // Ignore a file changing during the read; the next pass retries it.
            let after = fs::metadata(&path).map_err(|e| e.to_string())?;
            if after.len() != stat.len() || stamp(&after) != stamp(&stat) {
                return Err("El archivo cambió durante la lectura; se reintentará".into());
            }
            fresh.hash = hash;
            fresh.size = stat.len();
            fresh.modified_ns = stamp(&stat);
            if let Some(i) = index {
                let old = &doc.tracks[i];
                fresh.id = old.id.clone();
                crate::metadata::retain_identifiers(&mut fresh.metadata, &old.metadata);
                fresh.added_at = old.added_at;
                fresh.favorite = old.favorite;
                fresh.last_played = old.last_played;
                fresh.play_count = old.play_count;
                if old.edited {
                    MetadataEdit {
                        title: old.title.clone(),
                        artist: old.artist.clone(),
                        album_artist: old.album_artist.clone(),
                        album: old.album.clone(),
                        genre: old.genre.clone(),
                        year: old.year,
                        track_number: old.track_number,
                        disc_number: old.disc_number,
                        comments: old.comments.clone(),
                    }
                    .apply(&mut fresh);
                    fresh.edited = true;
                }
                crate::metadata::prepare(&mut fresh)?;
                doc.tracks[i] = fresh;
            } else {
                crate::metadata::prepare(&mut fresh)?;
                doc.tracks.push(fresh);
            }
            Ok(())
        })();
        if let Err(e) = result {
            warnings.push(format!("{}: {e}", path.display()));
        }
    }
    warnings.truncate(100);
}
fn probe_file(path: &Path) -> Result<lofty::file::TaggedFile, String> {
    lofty::probe::Probe::open(path)
        .map_err(|e| e.to_string())?
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())
}
fn read_track(storage: &StorageManager, path: &Path) -> Result<TrackEntry, String> {
    // Lofty probes file contents rather than trusting the extension.
    let file = probe_file(path)?;
    let mut t = TrackEntry {
        id: format!("local-{:032x}", rand::random::<u128>()),
        title: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        source: Some("local".into()),
        local_path: Some(path.to_string_lossy().into_owned()),
        duration: Some(file.properties().duration().as_secs()),
        format: format!("{:?}", file.file_type()),
        added_at: now(),
        ..Default::default()
    };
    if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
        if let Some(title) = tag.title().filter(|v| !v.trim().is_empty()) {
            t.title = title.into_owned();
        }
        t.artist = tag.artist().map(|v| v.into_owned());
        t.album = tag.album().map(|v| v.into_owned());
        t.genre = tag.genre().map(|v| v.into_owned());
        t.year = tag.date().map(|date| u32::from(date.year));
        t.track_number = tag.track();
        t.disc_number = tag.disk();
        t.comments = tag.comment().map(|v| v.into_owned());
        t.album_artist = tag.get_string(ItemKey::AlbumArtist).map(str::to_owned);
        t.metadata.identifiers = crate::metadata::ExternalIds {
            musicbrainz_recording_id: tag
                .get_string(ItemKey::MusicBrainzRecordingId)
                .map(str::to_owned),
            musicbrainz_release_id: tag
                .get_string(ItemKey::MusicBrainzReleaseId)
                .map(str::to_owned),
            musicbrainz_release_group_id: tag
                .get_string(ItemKey::MusicBrainzReleaseGroupId)
                .map(str::to_owned),
            musicbrainz_artist_id: tag
                .get_string(ItemKey::MusicBrainzArtistId)
                .map(str::to_owned),
            musicbrainz_album_artist_id: tag
                .get_string(ItemKey::MusicBrainzReleaseArtistId)
                .map(str::to_owned),
            isrc: tag.get_string(ItemKey::Isrc).map(str::to_owned),
            ..Default::default()
        };
        if let Some(xid) = tag.get_string(ItemKey::AppleXid) {
            t.metadata
                .identifiers
                .provider_track_ids
                .insert("apple_xid".into(), xid.into());
        }
        t.metadata.track_version = tag.get_string(ItemKey::TrackSubtitle).map(str::to_owned);
        t.metadata.remixer = tag.get_string(ItemKey::Remixer).map(str::to_owned);
        for (name, key) in [
            ("catalog_number", ItemKey::CatalogNumber),
            ("release_country", ItemKey::ReleaseCountry),
            ("release_type", ItemKey::MusicBrainzReleaseType),
        ] {
            if let Some(value) = tag.get_string(key) {
                t.metadata
                    .provider_data
                    .insert(name.into(), serde_json::json!(value));
            }
        }
        if let Some(picture) = tag
            .pictures()
            .first()
            .filter(|p| p.data().len() <= 10 * 1024 * 1024)
        {
            let data = picture.data();
            let ext = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
                Some("png")
            } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
                Some("jpg")
            } else {
                None
            };
            if let Some(ext) = ext {
                let id = format!("{:x}.{ext}", Sha256::digest(data));
                let target = storage.document_path(&format!("artwork/local/{id}"));
                if !target.exists() {
                    fs::write(target, data).map_err(|e| e.to_string())?;
                }
                t.artwork_id = Some(id);
            }
        }
    }
    t.metadata
        .provider_data
        .insert("local_tags_version".into(), serde_json::json!(1));
    crate::metadata::prepare(&mut t)?;
    Ok(t)
}
fn write_metadata(path: &Path, edit: &MetadataEdit) -> Result<(), String> {
    let original_hash = hash_file(path)?;
    let mut file = probe_file(path)?;
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(file.primary_tag_type()));
    }
    let tag = file
        .primary_tag_mut()
        .ok_or("El formato no permite escribir etiquetas")?;
    tag.set_title(edit.title.trim().into());
    for (key, value) in [
        (ItemKey::TrackArtist, &edit.artist),
        (ItemKey::AlbumArtist, &edit.album_artist),
        (ItemKey::AlbumTitle, &edit.album),
        (ItemKey::Genre, &edit.genre),
        (ItemKey::Comment, &edit.comments),
    ] {
        tag.remove_key(key);
        if let Some(value) = value.as_ref().filter(|v| !v.is_empty()) {
            tag.insert_text(key, value.clone());
        }
    }
    tag.remove_date();
    tag.remove_key(ItemKey::Year);
    if let Some(n) = edit.year {
        tag.set_date(lofty::tag::items::Timestamp {
            year: u16::try_from(n).map_err(|_| "El año debe ser menor a 65536")?,
            ..Default::default()
        });
    }
    tag.remove_track();
    if let Some(n) = edit.track_number {
        tag.set_track(n);
    }
    tag.remove_disk();
    if let Some(n) = edit.disc_number {
        tag.set_disk(n);
    }
    // Stage the rewrite next to the original, and keep an exact recovery copy.
    // The original is untouched if encoding fails. Cross-platform rename-over
    // semantics are avoided by reserving a unique backup and then replacing.
    let suffix = format!("{:032x}", rand::random::<u128>());
    let staged = path.with_extension(format!("zenith-{suffix}.tmp"));
    let backup = path.with_extension(format!("zenith-{suffix}.bak"));
    fs::copy(path, &staged).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        file.save_to_path(&staged, WriteOptions::default())
            .map_err(|e| e.to_string())?;
        File::open(&staged)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        if hash_file(path)? != original_hash {
            return Err(
                "El archivo cambió durante la edición. Vuelve a explorar y reintenta.".into(),
            );
        }
        fs::rename(path, &backup).map_err(|e| e.to_string())?;
        if let Err(e) = fs::rename(&staged, path) {
            fs::rename(&backup, path).map_err(|restore| {
                format!(
                    "{e}; restauración fallida: {restore}. Copia: {}",
                    backup.display()
                )
            })?;
            return Err(e.to_string());
        }
        Ok(())
    })();
    let _ = fs::remove_file(staged);
    result
}
/// A play is counted after thirty seconds of actual playback in the audio
/// worker, once per started track. Missing tracks retain their listening data.
#[cfg(test)]
pub fn record_play(storage: &StorageManager, path: &str) -> Result<(), String> {
    let _operation = storage.library_lock();
    let mut doc = load(storage)?;
    if let Some(t) = doc
        .tracks
        .iter_mut()
        .find(|t| t.local_path.as_deref() == Some(path))
    {
        t.play_count = t.play_count.saturating_add(1);
        t.last_played = Some(now());
        save(storage, &doc)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "library/tests.rs"]
mod tests;
