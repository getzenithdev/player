# Zenith Player

A local-first desktop music player built with Tauri 2, React, TypeScript, and Rust.

The repository provides the application foundation, local audio playback, and a persistent local music library with import, monitoring, metadata editing, favorites, statistics, and duplicate reporting. Lyrics, external artwork providers, and cloud sources remain planned in the [roadmap](docs/roadmap.md). The audio engine selection, format matrix, and manual verification steps are documented in [audio-engine.md](docs/audio-engine.md).

## Development Interface

Navigate between the library, playlists, queue, lyrics, artwork, sources, equalizer, and settings. The **Fases** screen exposes all 16 roadmap phases with summarized manual acceptance checklists and shortcuts to their modules.

In the desktop application, **Cola → Abrir y reemplazar cola** loads local audio files into an independent persistent queue. Playback, pause, stop, seeking, volume, mute, repeat, shuffle, and output restart are connected to the Rust audio engine. The browser-only preview keeps playback controls disabled because Tauri commands are unavailable there.

Playlists, queue sessions, listening history, and the theme persist in the desktop application. Equalizer preview controls and manual review checkboxes remain session-only. Pending native features have disabled controls, and manual review marks do not mark a phase as implemented. See the [interface design notes](docs/design.md).

## Local Music Library

**Biblioteca** supports importing files or multiple folders, searching songs and metadata, grouped views, favorites, history, and playback. Locations are reconciled every ten seconds or on demand. Metadata edits stay internal unless the user explicitly requests writing tags to the original; explicit writes retain a recovery copy. See [Music Library and Local Files](docs/music-library.md) for identity rules, limitations, and desktop acceptance steps.

## Playlists, Queue, and History

**Listas** supports persistent playlist editing and M3U/M3U8/JSON interchange. **Cola** supports appending, play-next insertion, reordering, removal, an explicit saved copy, and automatic session restoration without autoplay. **Historial** records credited listening time and play counts, including files outside the library. See [Phase 5](docs/playlists-queue.md) for data contracts, counting rules, and installed-build acceptance.

## Local Storage

Phase 2 stores every document as versioned JSON under a single per-platform root: `%APPDATA%\Zenith Player` on Windows, `~/Library/Application Support/Zenith Player` on macOS, and `$XDG_DATA_HOME` (or `~/.local/share`) on Linux. Portable mode is an explicit choice: placing a `zenith-portable` marker file next to the executable moves the root into a `Zenith Player` folder beside it.

Each write goes to a temporary file that is flushed to disk and then replaced, so an interrupted write never destroys the previous copy. Documents are wrapped in a versioned envelope (`{"version":1,"updated_at":…,"data":…}`); a file written by a newer major version is preserved instead of being overwritten. A damaged document is moved to `logs/` and restored from the newest usable backup in `backups/` (the five newest per document), and the event is reported in **Configuración → Almacenamiento** and in `logs/storage.log`.

**Configuración → Almacenamiento** shows the real storage root, write permissions, document and backup counts, cache size, and the in-memory index counters. It can create a manual backup, export and import the data bundle (configuration, library, and lyrics — never `config/providers.json`), and clear the cache without touching documents or music files. In the browser preview these actions stay disabled and explain that they require the desktop application.

The Rust layer (`src-tauri/src/storage/`) carries 25 unit tests covering layout, atomic writes, recovery, backup pruning, export/import, and indexes. Manual acceptance on installed builds (survive a restart, simulate a corrupted file end to end) is still pending, which is why the phase is marked "En pruebas" instead of completed.

## Development

Install Node.js 24 and the [native prerequisites](docs/environment.md) for your operating system, then run:

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
- [Playlists, queue, and history](docs/playlists-queue.md)
- [Development roadmap](docs/roadmap.md)
