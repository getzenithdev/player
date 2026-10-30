# Zenith Player

A local-first desktop music player built with Tauri 2, React, TypeScript, and Rust.

The repository provides the application foundation, a minimal development interface, and local audio playback from phase 3. The library, lyrics, artwork, and external providers are planned in the [roadmap](docs/roadmap.md). The audio engine selection, format matrix, and manual verification steps are documented in [audio-engine.md](docs/audio-engine.md).

## Development Interface

Navigate between the library, playlists, queue, lyrics, artwork, sources, equalizer, and settings. The **Fases** screen exposes all 16 roadmap phases with summarized manual acceptance checklists and shortcuts to their modules.

In the desktop application, **Cola → Abrir archivos** loads local audio files into a temporary queue. Playback, pause, stop, seeking, volume, mute, repeat, shuffle, and output restart are connected to the Rust audio engine. The browser-only preview keeps playback controls disabled because Tauri commands are unavailable there.

Navigation, temporary test playlists, equalizer controls, and review checkboxes work within the current session. The theme is the only preference persisted so far, in the desktop application. Pending native features have disabled controls, and manual review marks do not mark a phase as implemented. See the [interface design notes](docs/design.md).

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

Vitest runs the frontend unit tests (currently the storage wrappers' browser fallback and size formatting). The `--passWithNoTests` flag remains explicit; an empty run is not evidence of feature coverage.

GitHub Actions checks the frontend and compiles Windows x64, macOS Intel/Apple Silicon, and Linux x64. These build jobs do not establish compatibility with all operating system versions listed in the specification.

## Documentation

- [Product specifications](docs/specs.md)
- [Environment and commands](docs/environment.md)
- [Audio engine](docs/audio-engine.md)
- [Development roadmap](docs/roadmap.md)
