# Zenith Player

A local-first desktop music player built with Tauri 2, React, TypeScript, and Rust.

The repository provides the application foundation, local audio playback, and a persistent local music library with import, monitoring, metadata editing, favorites, statistics, and duplicate reporting. Lyrics, artwork providers, and external source connections are implemented; user acceptance is recorded in the [roadmap](docs/roadmap.md). The audio engine selection, format matrix, and manual verification steps are documented in [audio-engine.md](docs/audio-engine.md).

## Listening Interface

Phase 10 turns the development workspace into a daily listening app inspired by the supplied HTML: Zenith branding, dark/light/system themes, Inicio, grouped search, collection galleries, favorites, playlists, history, downloads and connected sources. Clicking the mini player expands artwork and controls on the left and synchronized lyrics on the right over a blurred cover background. Minimize or Escape returns to the same page without losing its filters or position. The independent queue and lyric/artwork editing remain accessible from the expanded player.

All existing library, metadata, playlist, lyric, artwork and provider actions remain reachable through these listening flows. Advanced tools use disclosures instead of filling the main track rows. Development phase checklists are no longer part of the product. See the [interface design notes](docs/design.md) for navigation, design decisions and validation.

In the desktop application, **Cola → Abrir y reemplazar cola** loads local audio files into an independent persistent queue. Playback, pause, stop, seeking, volume, mute, repeat, shuffle, and output restart use the Rust engine. The browser-only preview keeps native actions disabled. Themes, playlists, sessions and listening history retain the existing persistence contracts.

The user confirmed phases 1–9 validated on 2026-11-02. Historical module documents retain their original test/acceptance notes; the updated roadmap records this later acceptance.

## Local Music Library

**Biblioteca** supports importing files or multiple folders, searching songs and metadata, grouped views, favorites, history, and playback. Locations are reconciled every ten seconds or on demand. Metadata edits stay internal unless the user explicitly requests writing tags to the original; explicit writes retain a recovery copy. See [Music Library and Local Files](docs/music-library.md) for identity rules, limitations, and desktop acceptance steps.

## Universal Metadata

Phase 6 adds normalized names with original-value preservation, source-neutral track identity, retained MusicBrainz/ISRC/provider IDs, edition/artist grouping, and conservative duplicate review. **Biblioteca → Importar metadatos JSON** ingests adapter snapshots, including records without local files; it does not connect external services. See [Metadata engine](docs/metadata-engine.md) for schema versions, migration, matching rules and validation.

## Playlists, Queue, and History

**Listas** supports persistent playlist editing and M3U/M3U8/JSON interchange. **Cola** supports appending, play-next insertion, reordering, removal, an explicit saved copy, and automatic session restoration without autoplay. **Historial** records credited listening time and play counts, including files outside the library. See [Phase 5](docs/playlists-queue.md) for data contracts, counting rules, and installed-build acceptance.

## External Sources

Phase 9 adds **Fuentes**, where every provider declares its own capabilities, so the interface only offers search, browsing, playback, download, or revocation when the provider supports it. Local library search works with no network and with the external opt-in disabled. YouTube automático is the primary keyless provider, preparing AAC/M4A audio in cache through local yt-dlp and Node 22 helpers. YouTube API remains a separate metadata-only alternative with existing keys preserved. Google Drive, Dropbox, and OneDrive connect through OAuth 2.0 with PKCE using your own client ID and a loopback redirect; WebDAV needs an HTTPS address with username and password.

Connections and non-secret settings persist in `config/providers.json`, which the data export never includes, while passwords, API keys, client secrets, and tokens stay in the operating system credential store. Network actions stay off until **Permitir consultas a servicios externos** is enabled. Downloads are stored once in `cache/audio/` and handed to the audio engine, so playback keeps working when a token expires or the network is lost. See [External sources](docs/external-sources.md) for the contract, provider endpoints, limits, tests, and pending manual acceptance.

## Local Storage

Phase 2 stores every document as versioned JSON under a single per-platform root: `%APPDATA%\Zenith Player` on Windows, `~/Library/Application Support/Zenith Player` on macOS, and `$XDG_DATA_HOME` (or `~/.local/share`) on Linux. Portable mode is an explicit choice: placing a `zenith-portable` marker file next to the executable moves the root into a `Zenith Player` folder beside it.

Each write goes to a temporary file that is flushed to disk and then replaced, so an interrupted write never destroys the previous copy. New writes use the v2 envelope (`{"version":2,"updated_at":…,"data":…}`), with v1 read/migration support; a file written by a newer major version is preserved instead of being overwritten. A damaged document is moved to `logs/` and restored from the newest usable backup in `backups/` (the five newest per document), and the event is reported in **Configuración → Biblioteca y datos** and in `logs/storage.log`.

**Configuración → Biblioteca y datos** shows the real storage root, write permissions, document and backup counts, cache size, and the in-memory index counters. It can create a manual backup, export and import the data bundle (configuration, library, and lyrics — never `config/providers.json`), and clear the cache without touching documents or music files. In the browser preview these actions stay disabled and explain that they require the desktop application.

The Rust layer (`src-tauri/src/storage/`) carries 25 unit tests covering layout, atomic writes, recovery, backup pruning, export/import, and indexes. The user reported prior phases validated on 2026-11-02; historical platform checklists remain documented for reference.

## Development

Install Node.js 22 (22.12+; use this version on Monterey) or Node.js 24 on newer systems, and the [native prerequisites](docs/environment.md) for your operating system, then run:

```sh
npm ci
npm run tauri dev
```

For a browser-only frontend preview:

```sh
npm run dev
```

The browser preview does not provide Tauri's native APIs.

## Checks and Builds

```sh
npm run check
npm run build
npm run rust:fmt
npm run rust:lint
npm run rust:test
npm run tauri build
```

`npm run build` compiles the frontend only. `npm run tauri build` builds the desktop application and local bundles. Signing and public releases are not configured yet.

Vitest runs frontend unit tests for storage browser fallbacks, size formatting, library search, grouping, views, playlist references, and stable queue reorder commands. The `--passWithNoTests` flag remains explicit; an empty run is not evidence of feature coverage.

GitHub Actions checks the frontend and compiles Windows x64, macOS Intel/Apple Silicon, and Linux x64. These build jobs do not establish compatibility with all operating system versions listed in the specification.

## Documentation

- [Product specifications](docs/specs.md)
- [Environment and commands](docs/environment.md)
- [Audio engine](docs/audio-engine.md)
- [Music library and local files](docs/music-library.md)
- [Universal metadata and identity](docs/metadata-engine.md)
- [Playlists, queue, and history](docs/playlists-queue.md)
- [Development roadmap](docs/roadmap.md)

Lyrics / Letras: [Phase 7 implementation and provider status](docs/lyrics-engine.md).

Artwork / Portadas: [Phase 8 implementation and provider status](docs/artwork-engine.md).

External sources / Fuentes: [Phase 9 implementation and provider status](docs/external-sources.md).

### YouTube without an API key / YouTube sin API key

YouTube automático is the primary provider, with the existing API retained as a metadata-only alternative. Desktop builds include pinned yt-dlp and Node 22 helpers. Audio is prepared as an AAC/M4A cache file before playback. Setup guides for every provider are inside Fuentes → Proveedores. See [the provider documentation](docs/youtube-provider.md) for packaging, limits, source permissions and the pending actual-Monterey acceptance checks.
