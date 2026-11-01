# Artwork engine / Motor de portadas (Phase 8)

## Resolution and persistence

`library/artwork.json` is an atomic versioned document keyed by the stable library track ID. It stores validated asset manifests, manual selections and preferences. Originals, 256px thumbnails and 1024px previews live in `artwork/resources/` under content-derived SHA-256 filenames. Duplicate image bytes share the same files. Provenance includes provider, HTTPS source, attribution, permission/license, saved date and decoded dimensions.

A manual selection wins. Otherwise priority is Apple Music animated, Apple Music static, Archive.org, YouTube, local, default. Saved resources are always usable offline, including resources previously obtained from external providers. No network request is needed to display them. A missing saved file falls through to available candidates and local/default rendering; artwork errors never enter the audio worker.

A snapshot discovers embedded library artwork and associated same-name JPG/PNG or `cover`, `folder`, `front` JPG/PNG/WebP/GIF files when no resources have been saved. **Buscar archivos locales** explicitly repeats local discovery. The library grid, bottom player, artwork screen and minimalist/fullscreen lyric background consume the artwork engine. Static thumbnails are used in small grid/player views.

## Validation and animation

PNG, JPEG, WebP and GIF are content-probed and decoded, rather than accepted by extension. Limits: 15 MiB encoded, 8192px dimensions, at least 32px per side, 64 MiB decoder allocation. GIF animation is limited to 120 frames and 64 MiB cumulative decoded pixels. Invalid/truncated images and unsupported SVG/HTML are rejected. Dimensions are preserved rather than distorted to match an assumed square.

Animated GIF is decoded in a native blocking worker into bounded 512px frames; the UI schedules their real delays. Autoplay, manual pause, looping, thumbnail/preview quality and battery saving persist. Autoplay defaults off and battery saving defaults on. Visibility observers, document visibility and reduced-motion changes stop scheduling frames and show the static preview. Offscreen animations are not loaded. Original GIF, thumbnail and preview are kept; video/MP4/HLS animation is not supported by this initial implementation.

Resetting artwork removes song associations and the manual override; it does not delete the user's original file or shared resource bytes. **Guardar original como…** exports the validated original through a native save dialog. General storage cache cleanup leaves saved artwork and lyrics untouched. The general JSON bundle includes the manifest, not the binary originals; export originals separately when moving them between computers. Unreferenced shared resources are retained to avoid deleting another song's image; cache-size/garbage-collection policy remains a phase 13/15 task.

## Providers verified 2026-11-01

- Local: embedded and associated image files, explicit image import; no network.
- Archive.org: opt-in bounded HTTPS advanced search plus item metadata and original download. User reviews title/creator/license and explicitly requests validation/storage. Automatic download accepts recognized Creative Commons license URLs, preserves license/attribution and validates the actual downloaded image. A listed license is metadata supplied by the source, not a warranty of rights. Unavailable/invalid resources leave current artwork intact. Search text accepts artist, album, title, year or catalog/ISRC hints, while stored results are ranked by exact artist/album/title. Documentation: https://archive.org/developers/md-read.html and https://archive.org/advancedsearch.php.
- Apple Music static/animated: authorized local manifest adapter. https://developer.apple.com/musickit/ requires developer credentials for the catalog; no developer token is present. The iTunes Search API's promotional artwork terms (https://developer.apple.com/library/archive/documentation/AudioVideo/Conceptual/iTuneSearchAPI/index.html) are not treated as blanket permission to cache album artwork for entertainment. No web-page scraping or guessed animated URLs are used.
- YouTube: authorized local manifest adapter. Direct metadata search requires the official API connection/credentials from phase 9; no thumbnail URL is guessed from a video ID.

Direct Apple Music/YouTube search is explicitly unavailable until authorized API connections are configured. These provider limitations do not block local use or Archive.org. There is no claim that every external resource permits persistence.

## Authorized manifest

Import JSON with **Importar recurso autorizado**. `path` points to an image relative to the JSON or an absolute local path. This never fetches an arbitrary remote URL. `permitted_storage` records the user's assertion that they may retain that resource; it does not grant rights on its own.

```json
{
  "version": 1,
  "provider": "apple-music",
  "path": "my-authorized-cover.gif",
  "title": "Recording title",
  "artist": "Artist",
  "album": "Album",
  "source_url": "https://music.apple.com/",
  "license": "Describe the permission that allows local storage",
  "attribution": "Required credit / rightsholder",
  "permitted_storage": true
}
```

`provider` accepts `local`, `apple-music`, `archive-org`, `youtube`. External provider manifests require HTTPS provenance and attribution. Displayed links are HTTPS-only and never contain embedded credentials.

## Acceptance

Automated tests exercise priority, validated dimensions/content identity, invalid media/permissions, persistence across restart, missing-file fallback, disabled external requests, path traversal/identifier rejection, license domains and all animation gating conditions. Also manually check native dialogs/fullscreen, service failures, offline restart, animation visibility/reduced motion and battery controls on installed macOS/Windows/Linux builds before declaring platform acceptance.

## Español

Las portadas se validan antes de guardarse, conservando original, miniatura, vista previa y procedencia. Se pueden importar imágenes y GIF, buscar en Archive.org o importar recursos autorizados de Apple Music/YouTube. Las consultas externas son opcionales. Las animaciones respetan pausa, repetición, ahorro de batería, visibilidad y movimiento reducido. La búsqueda directa de Apple Music/YouTube requiere las conexiones autorizadas de la fase 9; no se presenta como disponible. Falta registrar la aceptación manual de las aplicaciones instaladas.
