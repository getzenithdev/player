# Zenith Player

A local-first desktop music player built with Tauri 2, React, TypeScript, and Rust.

The repository provides the application foundation, a minimal development interface, and local audio playback from phase 3. The library, lyrics, artwork, and external providers are planned in the [roadmap](docs/roadmap.md). The audio engine selection, format matrix, and manual verification steps are documented in [audio-engine.md](docs/audio-engine.md).

## Development Interface

Navigate between the library, playlists, queue, lyrics, artwork, sources, equalizer, and settings. The **Fases** screen exposes all 16 roadmap phases with summarized manual acceptance checklists and shortcuts to their modules.

In the desktop application, **Cola → Abrir archivos** loads local audio files into a temporary queue. Playback, pause, stop, seeking, volume, mute, repeat, shuffle, and output restart are connected to the Rust audio engine. The browser-only preview keeps playback controls disabled because Tauri commands are unavailable there.

Navigation, temporary test playlists, theme selection, equalizer controls, and review checkboxes work within the current session. They do not save data or implement audio processing. Pending native features have disabled controls, and manual review marks do not mark a phase as implemented. See the [interface design notes](docs/design.md).

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

Vitest is installed for future unit tests. The test command explicitly allows an empty suite while the repository contains only the scaffold; a successful empty run is not evidence of feature coverage.

GitHub Actions checks the frontend and compiles Windows x64, macOS Intel/Apple Silicon, and Linux x64. These build jobs do not establish compatibility with all operating system versions listed in the specification.

## Documentation

- [Product specifications](docs/specs.md)
- [Environment and commands](docs/environment.md)
- [Audio engine](docs/audio-engine.md)
- [Development roadmap](docs/roadmap.md)
