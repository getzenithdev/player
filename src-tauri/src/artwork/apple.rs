//! Direct Apple album/cover resolution, ported from the dashboard's apple-motion.ts.
use super::*;
use std::{collections::BTreeSet, sync::{Condvar, Mutex, OnceLock}, time::Duration};

#[derive(Clone)]
struct Album {
    page: String,
    name: String,
    artist: String,
    album: String,
    image: Option<String>,
}
#[derive(Clone)]
struct Media {
    video: Option<String>,
    poster: Option<String>,
    album: Option<String>,
}
#[derive(Clone)]
struct Poster { asset: Asset, preview: std::path::PathBuf }
trait CacheValue: Clone {
    fn ttl(&self) -> u64;
    fn valid(&self) -> bool { true }
}
impl CacheValue for Album { fn ttl(&self) -> u64 { 6 * 3600 } }
impl CacheValue for Poster {
    fn ttl(&self) -> u64 { 6 * 3600 }
    fn valid(&self) -> bool { self.preview.is_file() }
}
impl CacheValue for Media {
    fn ttl(&self) -> u64 { if self.video.is_some() { 6 * 3600 } else { 30 * 60 } }
}
struct Entry<T> { result: Result<Option<T>, String>, expires: u64 }
struct State<T> { entries: BTreeMap<String, Entry<T>>, pending: BTreeSet<String> }
struct Shared<T> { state: Mutex<State<T>>, ready: Condvar }
static ALBUMS: OnceLock<Shared<Album>> = OnceLock::new();
static VIDEOS: OnceLock<Shared<Media>> = OnceLock::new();
static POSTERS: OnceLock<Shared<Poster>> = OnceLock::new();

// Share actual in-flight results, rather than marking a request as completed
// before the provider has answered. Errors expire much sooner than absence.
fn cached<T: CacheValue>(cache: &OnceLock<Shared<T>>, key: String, mut force: bool,
    fetch: impl FnOnce() -> Result<Option<T>, String>) -> Result<Option<T>, String> {
    let shared = cache.get_or_init(|| Shared {
        state: Mutex::new(State { entries: BTreeMap::new(), pending: BTreeSet::new() }),
        ready: Condvar::new(),
    });
    let mut state = shared.state.lock().unwrap_or_else(|p| p.into_inner());
    loop {
        if state.pending.contains(&key) {
            let (next, timeout) = shared.ready.wait_timeout(state, Duration::from_secs(30))
                .unwrap_or_else(|p| p.into_inner());
            state = next;
            if timeout.timed_out() { return Err("Apple Music tardó demasiado; vuelve a intentar".into()); }
            force = false;
            continue;
        }
        if !force && let Some(entry) = state.entries.get(&key)
            && entry.expires > now()
            && entry.result.as_ref().ok().and_then(|value| value.as_ref()).is_none_or(CacheValue::valid) {
            return entry.result.clone();
        }
        if state.pending.len() >= 8 { return Err("Hay varias búsquedas de portadas en curso; vuelve a intentar".into()); }
        state.pending.insert(key.clone());
        break;
    }
    drop(state);
    let result = fetch();
    let ttl = match &result { Ok(Some(value)) => value.ttl(), Ok(None) => 30 * 60, Err(_) => 60 };
    let mut state = shared.state.lock().unwrap_or_else(|p| p.into_inner());
    state.pending.remove(&key);
    state.entries.retain(|_, entry| entry.expires > now());
    if state.entries.len() >= 64
        && let Some(oldest) = state.entries.iter().min_by_key(|(_, entry)| entry.expires).map(|(key, _)| key.clone()) {
        state.entries.remove(&oldest);
    }
    state.entries.insert(key, Entry { result: result.clone(), expires: now() + ttl });
    shared.ready.notify_all();
    result
}

pub(super) struct Lookup { pub asset: Asset, pub warning: Option<String> }

fn name_key(value: &str) -> String {
    metadata::key(value).chars().filter(|c| c.is_alphanumeric()).collect()
}
fn clean_artist(value: &str) -> String {
    let mut value = value.trim().to_string();
    for suffix in [" - topic", "vevo", " official artist channel", " official channel", " official"] {
        if value.to_lowercase().ends_with(suffix) {
            value.truncate(value.len() - suffix.len());
            value = value.trim().to_string();
        }
    }
    value
}
fn title_key(value: &str) -> String {
    let mut value = value.to_lowercase();
    for label in ["official music video", "official video", "official audio", "official lyric video", "official lyrics", "official visualizer", "lyrics", "1080p", "4k", "hd", "hq"] {
        for (left, right) in [('(', ')'), ('[', ']'), ('{', '}')] {
            value = value.replace(&format!("{left}{label}{right}"), "");
        }
    }
    let end = [" (feat.", " (ft.", " feat.", " ft.", " featuring "]
        .iter().filter_map(|marker| value.find(marker)).min().unwrap_or(value.len());
    name_key(value[..end].trim())
}
pub(super) fn identity(track: &TrackEntry) -> String {
    serde_json::json!(["direct-v3", title_key(&track.title),
        name_key(&clean_artist(track.artist.as_deref().unwrap_or_default())),
        name_key(track.album.as_deref().unwrap_or_default())]).to_string()
}

fn media_url(value: &str, video: bool) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    let host = url.host_str()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() || url.port().is_some()
        || !(host.ends_with(".mzstatic.com") || (video && host == "mvod.itunes.apple.com")) { return None; }
    if video && !(url.path().ends_with(".m3u8") || url.path().ends_with(".mp4")) { return None; }
    Some(url.to_string())
}
fn album_page(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() || url.port().is_some()
        || !matches!(url.host_str(), Some("music.apple.com" | "itunes.apple.com")) { return None; }
    let parts: Vec<_> = url.path_segments()?.filter(|part| !part.is_empty()).collect();
    if !(parts.len() == 3 || parts.len() == 4) || parts[0].len() != 2
        || !parts[0].bytes().all(|c| c.is_ascii_lowercase()) || parts[1] != "album" { return None; }
    let id = parts.last()?;
    if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()) { return None; }
    Some(format!("https://music.apple.com/{}/album/{id}", parts[0]))
}
fn decode(value: &str) -> String {
    value.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'")
}
fn tags<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    html.split('<').filter_map(|part| {
        let end = part.find('>')?;
        let tag = &part[..end];
        let next = tag.as_bytes().get(name.len()).copied();
        (tag.get(..name.len()).is_some_and(|prefix| prefix.eq_ignore_ascii_case(name))
            && next.is_some_and(|c| c.is_ascii_whitespace() || c == b'/'))
            .then_some(tag)
    }).collect()
}
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    for (index, _) in lower.match_indices(name) {
        if index != 0 && !tag.as_bytes()[index - 1].is_ascii_whitespace() { continue; }
        let Some(rest) = tag[index + name.len()..].trim_start().strip_prefix('=') else { continue; };
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '\'' && quote != '"' { continue; }
        if let Some(end) = rest[1..].find(quote) { return Some(decode(&rest[1..end + 1])); }
    }
    None
}
fn meta(html: &str, property: &str) -> Option<String> {
    tags(html, "meta").into_iter().find_map(|tag| {
        (attribute(tag, "property").as_deref() == Some(property))
            .then(|| attribute(tag, "content")).flatten()
    })
}
fn cover_video(html: &str) -> Option<String> {
    for tag in tags(html, "amp-ambient-video") {
        if attribute(tag, "class").is_some_and(|value| value.split_whitespace().any(|class| class == "editorial-video--cover"))
            && let Some(url) = attribute(tag, "src").and_then(|url| media_url(&url, true)) { return Some(url); }
    }
    None
}
fn page_text(url: reqwest::Url) -> Result<String, String> {
    String::from_utf8(resources::get(url, 2 * 1024 * 1024)?).map_err(|_| "Página de Apple Music inválida".into())
}
fn album_media(page: &str, force: bool) -> Result<Option<Media>, String> {
    let page = album_page(page).ok_or("Álbum de Apple Music inválido")?;
    cached(&VIDEOS, page.clone(), force, || {
        let html = page_text(reqwest::Url::parse(&page).map_err(|e| e.to_string())?)?;
        let poster = meta(&html, "og:image").and_then(|url| media_url(&url, false)).map(|image| {
            let mut url = reqwest::Url::parse(&image).unwrap();
            if let Some((parent, file)) = url.path().rsplit_once('/')
                && file.as_bytes().first().is_some_and(u8::is_ascii_digit) && file.contains('x') {
                let path = format!("{parent}/1000x1000bb.jpg");
                url.set_path(&path);
            }
            url.to_string()
        });
        let album = meta(&html, "og:title").map(|title| title.split(" - Album by ").next().unwrap_or(&title).trim().to_string());
        Ok(Some(Media { video: cover_video(&html), poster, album }))
    })
}
fn field(value: &serde_json::Value, name: &str) -> String {
    value[name].as_str().unwrap_or_default().to_string()
}
fn first_title(value: &serde_json::Value) -> String {
    value.as_array().and_then(|items| items.first()).map(|item| field(item, "title")).unwrap_or_default()
}
fn collect(node: &serde_json::Value, found: &mut BTreeMap<String, (bool, Album)>, budget: &mut usize) {
    if *budget == 0 { return; }
    *budget -= 1;
    match node {
        serde_json::Value::Object(object) => {
            let kind = node["contentDescriptor"]["kind"].as_str().unwrap_or_default();
            if matches!(kind, "album" | "song")
                && let Some(page) = node["contentDescriptor"]["url"].as_str().and_then(album_page) {
                let mut name = field(node, "title");
                if name.is_empty() { name = first_title(&node["titleLinks"]); }
                let mut artist = field(node, "subtitle");
                if artist.to_lowercase().starts_with("album · ") { artist = artist.chars().skip(8).collect(); }
                if artist.is_empty() { artist = first_title(&node["subtitleLinks"]); }
                let is_album = kind == "album";
                if !name.is_empty() && found.get(&page).is_none_or(|(old_album, _)| is_album && !*old_album) {
                    found.insert(page.clone(), (is_album, Album {
                        page, name: name.clone(), artist, album: if is_album { name } else { String::new() }, image: None,
                    }));
                }
            }
            for child in object.values() { collect(child, found, budget); }
        }
        serde_json::Value::Array(items) => { for child in items { collect(child, found, budget); } }
        _ => {}
    }
}
fn search_page(track: &TrackEntry, query: Option<&str>) -> Result<Option<Album>, String> {
    let artist = clean_artist(track.artist.as_deref().unwrap_or_default());
    let album = track.album.as_deref().filter(|value| !value.trim().is_empty());
    let term = query.map(str::to_string).unwrap_or_else(|| format!("{} {}", artist, album.unwrap_or(&track.title)));
    let mut url = reqwest::Url::parse("https://music.apple.com/us/search").unwrap();
    url.query_pairs_mut().append_pair("term", &term);
    let html = page_text(url)?;
    let mut found = BTreeMap::new();
    for script in html.split("<script").skip(1) {
        let Some((tag, rest)) = script.split_once('>') else { continue; };
        if attribute(tag, "id").as_deref() != Some("serialized-server-data") { continue; }
        if let Some((json, _)) = rest.split_once("</script>")
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
            let mut budget = 50_000;
            collect(&value, &mut found, &mut budget);
        }
    }
    if found.is_empty() {
        for tag in tags(&html, "a") {
            let Some(label) = attribute(tag, "aria-label") else { continue; };
            let parts: Vec<_> = label.split('·').map(str::trim).collect();
            if parts.len() < 3 || !parts[1].eq_ignore_ascii_case("album") { continue; }
            if let Some(page) = attribute(tag, "href").and_then(|href| album_page(&href)) {
                found.entry(page.clone()).or_insert((true, Album {
                    page, name: parts[0].into(), artist: parts[2..].join(" · "), album: parts[0].into(), image: None,
                }));
            }
        }
    }
    let artist = name_key(&artist);
    let album = name_key(album.unwrap_or_default());
    let title = title_key(&track.title);
    let mut best = None;
    let mut best_score = 0;
    for (_, candidate) in found.into_values() {
        let name = name_key(&candidate.name);
        let performer = name_key(&clean_artist(&candidate.artist));
        if performer.is_empty() || !(performer == artist || performer.contains(&artist) || artist.contains(&performer)) { continue; }
        let mut score = 0;
        if !album.is_empty() && name == album { score += 100; }
        else if !album.is_empty() && (name.contains(&album) || album.contains(&name)) { score += 45; }
        if !title.is_empty() && name == title { score += 80; }
        if score > best_score { best_score = score; best = Some(candidate); }
    }
    Ok(best)
}
fn catalog_album(track: &TrackEntry, query: Option<&str>) -> Result<Option<Album>, String> {
    let artist = clean_artist(track.artist.as_deref().unwrap_or_default());
    let album = track.album.as_deref().filter(|value| !value.trim().is_empty());
    for entity in if album.is_some() { vec!["album", "song"] } else { vec!["song"] } {
        let term = query.map(str::to_string).unwrap_or_else(|| format!("{} {}", artist,
            if entity == "album" { album.unwrap_or_default() } else { &track.title }));
        let mut url = reqwest::Url::parse("https://itunes.apple.com/search").unwrap();
        url.query_pairs_mut().append_pair("term", &term).append_pair("entity", entity)
            .append_pair("media", "music").append_pair("limit", "25");
        let value: serde_json::Value = serde_json::from_slice(&resources::get(url, 1024 * 1024)?)
            .map_err(|_| "Respuesta del catálogo de Apple Music inválida")?;
        let Some(rows) = value["results"].as_array() else { continue; };
        let selected = rows.iter().enumerate().filter(|(_, row)|
            name_key(&field(row, "artistName")) == name_key(&artist) &&
            if entity == "album" { name_key(&field(row, "collectionName")) == name_key(album.unwrap_or_default()) }
            else { title_key(&field(row, "trackName")) == title_key(&track.title) }
        ).max_by_key(|(index, row)| {
            let name = field(row, "collectionName");
            let lower = name.to_lowercase();
            (album.is_some_and(|album| name_key(album) == name_key(&name)),
                name_key(&name) == title_key(&track.title),
                !["alternative", "alternate", "deluxe", "single", "remix", "live"].iter().any(|label| lower.contains(label)),
                std::cmp::Reverse(*index))
        });
        if let Some((_, row)) = selected {
            let page = row["collectionViewUrl"].as_str().and_then(album_page)
                .or_else(|| row["collectionId"].as_u64().map(|id| format!("https://music.apple.com/us/album/{id}")));
            if let Some(page) = page {
                let name = field(row, "collectionName");
                return Ok(Some(Album { page, name: name.clone(), album: name,
                    artist: field(row, "artistName"), image: row["artworkUrl100"].as_str()
                        .and_then(|url| media_url(url, false)).map(|url| url.replace("100x100bb", "1000x1000bb")),
                }));
            }
        }
    }
    Ok(None)
}

pub(super) fn search(s: &StorageManager, track: &TrackEntry, query: Option<&str>,
    existing: Option<&Asset>, force: bool) -> Result<Option<Lookup>, String> {
    if track.artist.as_ref().is_none_or(|artist| artist.trim().is_empty()) {
        return Err("Falta el artista para buscar la portada en Apple Music".into());
    }
    // A URL learned for the old metadata is not a valid hint after an edit.
    // Different songs may share an album, but unrelated editions must not share identity.
    let existing = existing.filter(|asset| {
        name_key(&clean_artist(&asset.artist)) == name_key(&clean_artist(track.artist.as_deref().unwrap_or_default()))
            && if let Some(album) = track.album.as_deref().filter(|album| !album.trim().is_empty()) {
                name_key(album) == name_key(&asset.album)
            } else { title_key(&track.title) == title_key(&asset.title) }
    });
    let known = existing.and_then(|asset| asset.source_url.as_deref()).and_then(album_page);
    let key = serde_json::json!([identity(track), query, known]).to_string();
    let target = cached(&ALBUMS, key, force, || {
        if query.is_none() && let Some(page) = &known {
            return Ok(Some(Album { page: page.clone(), name: track.album.clone().unwrap_or_default(),
                album: track.album.clone().unwrap_or_default(), artist: track.artist.clone().unwrap_or_default(), image: None }));
        }
        if let Some(page) = query.and_then(album_page) {
            return Ok(Some(Album { page, name: track.album.clone().unwrap_or_default(),
                album: track.album.clone().unwrap_or_default(), artist: track.artist.clone().unwrap_or_default(), image: None }));
        }
        match search_page(track, query) {
            Ok(Some(album)) => Ok(Some(album)),
            Ok(None) => catalog_album(track, query),
            Err(page_error) => match catalog_album(track, query) {
                Ok(Some(album)) => Ok(Some(album)),
                Ok(None) => Err(page_error),
                Err(error) => Err(format!("{page_error}; {error}")),
            },
        }
    })?;
    let Some(target) = target else { return Ok(None); };
    let (media, warning) = match album_media(&target.page, force) {
        Ok(media) => (media, None),
        Err(error) => (None, Some(error)),
    };
    let same_asset = existing.filter(|asset| asset.source_url.as_deref().and_then(album_page).as_deref() == Some(target.page.as_str()));
    let mut asset = if let Some(asset) = same_asset { asset.clone() }
    else { cached(&POSTERS, format!("{}:{}", s.document_path("artwork/resources").display(), target.page), force, || {
        let image = target.image.as_ref().or_else(|| media.as_ref().and_then(|media| media.poster.as_ref()))
            .ok_or("Apple Music no entregó una imagen de portada para este álbum")?;
        let bytes = resources::get(reqwest::Url::parse(image).map_err(|e| e.to_string())?, MAX_IMAGE)?;
        let asset = store(s, &bytes, Manifest {
            version: 1, provider: "apple-music".into(), path: String::new(), title: track.title.clone(),
            artist: target.artist.clone(), album: if target.album.is_empty() {
                media.as_ref().and_then(|media| media.album.clone()).unwrap_or_default()
            } else { target.album.clone() }, source_url: Some(target.page.clone()),
            license: "Apple Music catalog artwork".into(), attribution: "Apple Music".into(), permitted_storage: true,
        }, track)?;
        let preview = image_path(s, &asset.id, "preview")?;
        Ok(Some(Poster { asset, preview }))
    })?.ok_or("Apple Music no entregó la imagen de portada")?.asset };
    if warning.is_none() {
        asset.motion_url = media.and_then(|media| media.video);
    }
    asset.title = track.title.clone();
    asset.animated = asset.motion_url.is_some();
    asset.saved_at = now();
    Ok(Some(Lookup { asset, warning }))
}
