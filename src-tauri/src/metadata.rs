//! Universal metadata contract v1. Internal IDs identify library entries, not
//! recordings. Matching only proposes candidates; it never merges user data.
use crate::storage::indexes::{TrackEntry, TracksDocument};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use unicode_normalization::UnicodeNormalization;
pub const VERSION: u32 = 1;
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ExternalIds {
    pub musicbrainz_recording_id: Option<String>,
    pub musicbrainz_release_id: Option<String>,
    pub musicbrainz_release_group_id: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub musicbrainz_album_artist_id: Option<String>,
    pub isrc: Option<String>,
    pub provider_track_ids: BTreeMap<String, String>,
    pub provider_artist_ids: BTreeMap<String, String>,
    pub provider_album_ids: BTreeMap<String, String>,
}
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OriginalValues {
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub comments: Option<String>,
}
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Normalized {
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub genre: String,
}
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MetadataRecord {
    /// 0 means legacy data requiring preparation, not a published format.
    pub version: u32,
    pub original: Option<OriginalValues>,
    pub normalized: Normalized,
    pub identifiers: ExternalIds,
    pub edition: Option<String>,
    pub track_version: Option<String>,
    pub remixer: Option<String>,
    pub artist_disambiguation: Option<String>,
    pub qualifiers: Vec<String>,
    pub artist_key: String,
    pub album_key: String,
    /// Provider-specific inert JSON, retained without executing requests.
    pub provider_data: BTreeMap<String, Value>,
}
pub fn display(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .nfc()
        .collect()
}
pub fn key(value: &str) -> String {
    display(value).to_lowercase().nfc().collect()
}
fn option(value: &Option<String>) -> Option<String> {
    value.as_deref().map(display).filter(|s| !s.is_empty())
}
fn valid_mbid(value: &Option<String>) -> Option<String> {
    let value = value.as_ref()?.trim().to_ascii_lowercase();
    let valid = value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        });
    valid.then_some(value)
}
fn canonical_isrc(value: &Option<String>) -> Option<String> {
    let v = value
        .as_ref()?
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    let b = v.as_bytes();
    (b.len() == 12
        && b[..2].iter().all(u8::is_ascii_alphabetic)
        && b[2..5].iter().all(u8::is_ascii_alphanumeric)
        && b[5..].iter().all(u8::is_ascii_digit))
    .then_some(v)
}
fn id_key(ids: &ExternalIds, album_artist: bool) -> Option<String> {
    let mb = if album_artist {
        valid_mbid(&ids.musicbrainz_album_artist_id)
            .or_else(|| valid_mbid(&ids.musicbrainz_artist_id))
    } else {
        valid_mbid(&ids.musicbrainz_artist_id)
    };
    if let Some(id) = mb {
        return Some(format!("musicbrainz:{id}"));
    }
    // Multiple provider identities are retained. A deterministic full map key
    // avoids treating unrelated provider-local IDs as globally equivalent.
    if !ids.provider_artist_ids.is_empty() {
        return Some(format!(
            "providers:{}",
            serde_json::to_string(&ids.provider_artist_ids).unwrap_or_default()
        ));
    }
    None
}
fn qualifiers(t: &TrackEntry) -> Vec<String> {
    let text = key(&format!(
        "{} {} {} {} {}",
        t.title,
        t.album.as_deref().unwrap_or_default(),
        t.metadata.edition.as_deref().unwrap_or_default(),
        t.metadata.track_version.as_deref().unwrap_or_default(),
        t.metadata.remixer.as_deref().unwrap_or_default()
    ));
    let words: BTreeSet<_> = text.split(|c: char| !c.is_alphanumeric()).collect();
    [
        "live",
        "remix",
        "remastered",
        "remaster",
        "acoustic",
        "instrumental",
        "deluxe",
        "demo",
        "mono",
        "stereo",
        "extended",
    ]
    .into_iter()
    .filter(|q| words.contains(q))
    .map(str::to_string)
    .collect()
}
pub fn prepare(t: &mut TrackEntry) -> Result<(), String> {
    if t.metadata.version > VERSION {
        return Err(format!(
            "Metadatos v{} no compatibles: {}",
            t.metadata.version, t.id
        ));
    }
    if t.id.is_empty() {
        t.id = format!("track-{}", crate::playlists::new_id());
    }
    if t.metadata.original.is_none() {
        t.metadata.original = Some(OriginalValues {
            title: t.title.clone(),
            artist: t.artist.clone(),
            album_artist: t.album_artist.clone(),
            album: t.album.clone(),
            genre: t.genre.clone(),
            comments: t.comments.clone(),
        });
    }
    t.title = display(&t.title);
    t.artist = option(&t.artist);
    t.album_artist = option(&t.album_artist);
    t.album = option(&t.album);
    t.genre = option(&t.genre);
    t.source = Some(
        t.source
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(if t.local_path.is_some() {
                "local"
            } else {
                "unknown"
            })
            .to_string(),
    );
    t.metadata.normalized = Normalized {
        title: key(&t.title),
        artist: key(t.artist.as_deref().unwrap_or_default()),
        album_artist: key(t
            .album_artist
            .as_deref()
            .or(t.artist.as_deref())
            .unwrap_or_default()),
        album: key(t.album.as_deref().unwrap_or_default()),
        genre: key(t.genre.as_deref().unwrap_or_default()),
    };
    let n = &t.metadata.normalized;
    t.metadata.artist_key = id_key(&t.metadata.identifiers, false).unwrap_or_else(|| {
        if n.artist.is_empty() {
            format!("unknown:{}", t.id)
        } else {
            format!("name:{}", n.artist)
        }
    });
    let release = valid_mbid(&t.metadata.identifiers.musicbrainz_release_id)
        .map(|id| format!("mb:{id}"))
        .or_else(|| {
            (!t.metadata.identifiers.provider_album_ids.is_empty()).then(|| {
                format!(
                    "providers:{}",
                    serde_json::to_string(&t.metadata.identifiers.provider_album_ids)
                        .unwrap_or_default()
                )
            })
        });
    let known_album_artist = if n.album_artist != n.artist && !n.album_artist.is_empty() {
        valid_mbid(&t.metadata.identifiers.musicbrainz_album_artist_id)
            .map(|id| format!("musicbrainz:{id}"))
    } else {
        id_key(&t.metadata.identifiers, true)
    };
    let artist = known_album_artist.unwrap_or_else(|| {
        if n.album_artist.is_empty() {
            format!("unknown:{}", t.id)
        } else {
            format!("name:{}", n.album_artist)
        }
    });
    t.metadata.album_key = serde_json::to_string(&(
        release,
        n.album.clone(),
        artist,
        key(t.metadata.edition.as_deref().unwrap_or_default()),
        t.year,
        if n.album.is_empty() {
            Some(t.id.clone())
        } else {
            None
        },
    ))
    .map_err(|e| e.to_string())?;
    t.metadata.qualifiers = qualifiers(t);
    t.metadata.version = VERSION;
    Ok(())
}
pub fn prepare_document(doc: &mut TracksDocument) -> Result<bool, String> {
    let before = doc.clone();
    let mut ids = BTreeSet::new();
    for t in &mut doc.tracks {
        prepare(t)?;
        if !ids.insert(t.id.clone()) {
            return Err(format!("Identidad interna repetida: {}", t.id));
        }
    }
    Ok(*doc != before)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    IdenticalBytes,
    SharedRecording,
    PossibleRecording,
    Different,
    Insufficient,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Assessment {
    pub kind: MatchKind,
    pub reason: String,
}
fn result(kind: MatchKind, reason: &str) -> Assessment {
    Assessment {
        kind,
        reason: reason.into(),
    }
}
pub fn compare(a: &TrackEntry, b: &TrackEntry) -> Assessment {
    if !a.hash.is_empty() && a.hash == b.hash {
        return result(
            MatchKind::IdenticalBytes,
            "Contenido completo idéntico (SHA-256)",
        );
    }
    let x = &a.metadata;
    let y = &b.metadata;
    if x.qualifiers != y.qualifiers
        || key(x.track_version.as_deref().unwrap_or_default())
            != key(y.track_version.as_deref().unwrap_or_default())
        || key(x.remixer.as_deref().unwrap_or_default())
            != key(y.remixer.as_deref().unwrap_or_default())
    {
        return result(MatchKind::Different, "Versiones o calificadores distintos");
    }
    let conflicting_mb = match (
        valid_mbid(&x.identifiers.musicbrainz_artist_id),
        valid_mbid(&y.identifiers.musicbrainz_artist_id),
    ) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    };
    let conflicting_provider = x
        .identifiers
        .provider_artist_ids
        .iter()
        .any(|(namespace, id)| {
            y.identifiers
                .provider_artist_ids
                .get(namespace)
                .is_some_and(|other| other != id)
        });
    if conflicting_mb || conflicting_provider {
        return result(MatchKind::Different, "Identidades de artista distintas");
    }
    if (!x.qualifiers.is_empty() || !y.qualifiers.is_empty())
        && x.normalized.title != y.normalized.title
    {
        return result(MatchKind::Different, "Versiones con títulos distintos");
    }
    if let (Some(left), Some(right)) = (a.duration, b.duration)
        && left.abs_diff(right) > 2
    {
        return result(
            MatchKind::Different,
            "Las duraciones difieren más de dos segundos",
        );
    }
    let shared_mbid = valid_mbid(&x.identifiers.musicbrainz_recording_id)
        .is_some_and(|id| Some(id) == valid_mbid(&y.identifiers.musicbrainz_recording_id));
    let shared_isrc = canonical_isrc(&x.identifiers.isrc)
        .is_some_and(|id| Some(id) == canonical_isrc(&y.identifiers.isrc));
    if shared_mbid || shared_isrc {
        return result(
            MatchKind::SharedRecording,
            "Identificador de grabación compartido; las entradas y ediciones siguen separadas",
        );
    }
    if x.normalized.title.is_empty()
        || x.normalized.artist.is_empty()
        || y.normalized.title.is_empty()
        || y.normalized.artist.is_empty()
        || a.duration.is_none()
        || b.duration.is_none()
    {
        return result(
            MatchKind::Insufficient,
            "Metadatos insuficientes para identificar la grabación",
        );
    }
    if x.normalized.title == y.normalized.title && x.normalized.artist == y.normalized.artist {
        return result(
            MatchKind::PossibleRecording,
            "Título, artista y duración compatibles; requiere revisión",
        );
    }
    result(MatchKind::Different, "Título o artista diferentes")
}
#[derive(Clone, Default, Serialize)]
pub struct Analysis {
    pub album_candidates: Vec<AlbumCandidate>,
    pub artist_collisions: Vec<ArtistCollision>,
}
#[derive(Clone, Serialize)]
pub struct AlbumCandidate {
    pub name: String,
    pub album_keys: Vec<String>,
    pub track_ids: Vec<String>,
}
#[derive(Clone, Serialize)]
pub struct ArtistCollision {
    pub name: String,
    pub identities: Vec<String>,
}
pub fn analyze(tracks: &[TrackEntry]) -> Analysis {
    let mut albums: BTreeMap<String, Vec<&TrackEntry>> = BTreeMap::new();
    let mut artists: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for t in tracks {
        let n = &t.metadata.normalized;
        if !n.album.is_empty() && !n.album_artist.is_empty() {
            albums
                .entry(
                    serde_json::to_string(&(
                        &n.album,
                        &n.album_artist,
                        key(t.metadata.edition.as_deref().unwrap_or_default()),
                    ))
                    .unwrap_or_default(),
                )
                .or_default()
                .push(t);
        }
        if !n.artist.is_empty() {
            artists
                .entry(n.artist.clone())
                .or_default()
                .insert(t.metadata.artist_key.clone());
        }
    }
    Analysis {
        album_candidates: albums
            .into_values()
            .filter_map(|group| {
                let keys: BTreeSet<_> =
                    group.iter().map(|t| t.metadata.album_key.clone()).collect();
                (keys.len() > 1).then(|| AlbumCandidate {
                    name: group[0].album.clone().unwrap_or_default(),
                    album_keys: keys.into_iter().collect(),
                    track_ids: group.iter().map(|t| t.id.clone()).collect(),
                })
            })
            .collect(),
        artist_collisions: artists
            .into_iter()
            .filter_map(|(name, ids)| {
                (ids.len() > 1).then(|| ArtistCollision {
                    name,
                    identities: ids.into_iter().collect(),
                })
            })
            .collect(),
    }
}
/// Adapter-facing input deliberately excludes internal IDs and user state.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IncomingTrack {
    pub source: String,
    pub source_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub comments: Option<String>,
    pub duration: Option<u64>,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub local_path: Option<String>,
    pub artwork_id: Option<String>,
    pub lyrics_id: Option<String>,
    pub identifiers: ExternalIds,
    pub edition: Option<String>,
    pub track_version: Option<String>,
    pub remixer: Option<String>,
    pub artist_disambiguation: Option<String>,
    pub provider_data: BTreeMap<String, Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    format: String,
    version: u32,
    tracks: Vec<IncomingTrack>,
}
pub fn import_file(doc: &mut TracksDocument, path: &Path) -> Result<(), String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 10 * 1024 * 1024 {
        return Err("El archivo supera 10 MiB".into());
    }
    let bundle: Exchange =
        serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if bundle.format != "zenith-metadata" || bundle.version != VERSION {
        return Err("Formato o versión de metadatos no compatible".into());
    }
    if bundle.tracks.len() > 100_000 {
        return Err("Demasiadas entradas de metadatos".into());
    }
    // Work on a copy so a later invalid entry cannot partially mutate the caller.
    let mut next = doc.clone();
    for incoming in bundle.tracks {
        upsert(&mut next, incoming)?;
    }
    *doc = next;
    Ok(())
}
pub fn upsert(doc: &mut TracksDocument, input: IncomingTrack) -> Result<String, String> {
    if input.source.is_empty()
        || input.source.len() > 64
        || !input
            .source
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        return Err("La fuente debe ser un identificador en minúsculas".into());
    }
    if input.source == "local" && input.local_path.is_none() {
        return Err("Una entrada local requiere local_path".into());
    }
    if input
        .local_path
        .as_ref()
        .is_some_and(|p| !Path::new(p).is_absolute() || p.contains('\0'))
    {
        return Err("local_path debe ser una ruta local absoluta".into());
    }
    if input
        .source_id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > 4096)
    {
        return Err("source_id vacío o demasiado largo".into());
    }
    let matches: Vec<_> = doc
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.source.as_deref() == Some(&input.source)
                && ((input.source_id.is_some() && t.source_id == input.source_id)
                    || (input.source == "local" && t.local_path == input.local_path))
        })
        .map(|(i, _)| i)
        .collect();
    if matches.len() > 1 {
        return Err("Identidad de origen ambigua; no se fusionaron entradas".into());
    }
    let existing = matches.first().copied();
    let mut t = TrackEntry {
        title: input.title,
        artist: input.artist,
        album_artist: input.album_artist,
        album: input.album,
        genre: input.genre,
        comments: input.comments,
        duration: input.duration,
        year: input.year,
        track_number: input.track_number,
        disc_number: input.disc_number,
        source: Some(input.source),
        source_id: input.source_id,
        local_path: input.local_path,
        artwork_id: input.artwork_id,
        lyrics_id: input.lyrics_id,
        metadata: MetadataRecord {
            identifiers: input.identifiers,
            edition: input.edition,
            track_version: input.track_version,
            remixer: input.remixer,
            artist_disambiguation: input.artist_disambiguation,
            provider_data: input.provider_data,
            ..Default::default()
        },
        added_at: crate::playlists::now(),
        ..Default::default()
    };
    prepare(&mut t)?;
    if let Some(i) = existing {
        let old = &doc.tracks[i];
        t.id = old.id.clone();
        t.favorite = old.favorite;
        t.play_count = old.play_count;
        t.last_played = old.last_played;
        t.listened_ms = old.listened_ms;
        t.last_position_ms = old.last_position_ms;
        t.added_at = old.added_at;
        t.local_path = t.local_path.or_else(|| old.local_path.clone());
        t.artwork_id = t.artwork_id.or_else(|| old.artwork_id.clone());
        t.lyrics_id = t.lyrics_id.or_else(|| old.lyrics_id.clone());
        t.duration = t.duration.or(old.duration);
        retain_identifiers(&mut t.metadata, &old.metadata);
        if old.edited {
            t.title = old.title.clone();
            t.artist = old.artist.clone();
            t.album = old.album.clone();
            t.album_artist = old.album_artist.clone();
            t.genre = old.genre.clone();
            t.year = old.year;
            t.track_number = old.track_number;
            t.disc_number = old.disc_number;
            t.comments = old.comments.clone();
            t.edited = true;
        }
    }
    t.missing = t
        .local_path
        .as_ref()
        .is_some_and(|p| !Path::new(p).is_file());
    prepare(&mut t)?;
    let id = t.id.clone();
    if let Some(i) = existing {
        doc.tracks[i] = t;
    } else {
        doc.tracks.push(t);
    }
    Ok(id)
}
#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;

/// Missing identifiers in a later scan do not erase previously retained IDs.
/// A new explicit value from the source wins; invalid raw values remain inert.
pub fn retain_identifiers(next: &mut MetadataRecord, old: &MetadataRecord) {
    macro_rules! retain {($($field:ident),*)=>{$(if next.identifiers.$field.is_none(){next.identifiers.$field=old.identifiers.$field.clone();})*};}
    retain!(
        musicbrainz_recording_id,
        musicbrainz_release_id,
        musicbrainz_release_group_id,
        musicbrainz_artist_id,
        musicbrainz_album_artist_id,
        isrc
    );
    for (k, v) in &old.identifiers.provider_track_ids {
        next.identifiers
            .provider_track_ids
            .entry(k.clone())
            .or_insert(v.clone());
    }
    for (k, v) in &old.identifiers.provider_artist_ids {
        next.identifiers
            .provider_artist_ids
            .entry(k.clone())
            .or_insert(v.clone());
    }
    for (k, v) in &old.identifiers.provider_album_ids {
        next.identifiers
            .provider_album_ids
            .entry(k.clone())
            .or_insert(v.clone());
    }
    for (k, v) in &old.provider_data {
        next.provider_data.entry(k.clone()).or_insert(v.clone());
    }
    if next.edition.is_none() {
        next.edition = old.edition.clone();
    }
    if next.track_version.is_none() {
        next.track_version = old.track_version.clone();
    }
    if next.remixer.is_none() {
        next.remixer = old.remixer.clone();
    }
    if next.artist_disambiguation.is_none() {
        next.artist_disambiguation = old.artist_disambiguation.clone();
    }
}
