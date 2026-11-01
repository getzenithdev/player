# Zenith Player Development Roadmap

This document organizes the planned scope of the desktop player.

The features and platforms described are development targets, not verified compatibility claims. The repository includes the initial application scaffold and development tooling; no phase is considered complete without its deliverables and validation evidence.

## Project Principles

- Desktop application built with Tauri 2, React, TypeScript, and a Rust core.
- Persistence through JSON, lyrics, and media files; no databases.
- No mandatory self-hosted servers to run the application.
- Local playback, the library, and stored resources remain available offline.
- Independent providers: a service failure must not prevent application startup or access to local content.
- Local configuration, with access to external services only when enabled by the user.
- Avoid unnecessary duplication of music files.
- Respect each external source's authorized interfaces, licenses, and permissions.

The general scope and initial platform matrix are available in [specs.md](specs.md). This roadmap defines the phases and their completion conditions.

## Decisions Required Before Committing to Distribution

| Topic | Required decision |
| --- | --- |
| Developer accounts | Distinguish accounts needed by the application distributor from those required of users. Clarify the no-account requirement in relation to OAuth, external services, and signing/notarization. |
| Platforms | Confirm minimum versions, architectures, and Linux distributions through actual builds and tests. Windows ARM64 and Linux ARM64 require an explicit decision. |
| Audio engine | Compare libraries through prototypes; document decoding, output, licensing, and compatibility before selection. |
| Providers | Validate access, permissions, authentication, storage terms, and the availability of lyrics, artwork, and playback. |
| Credentials | Define protected storage using operating system mechanisms. Do not store unprotected passwords or tokens in configuration JSON files. |
| Linux distribution | Select package formats and define how each is updated; do not assume one mechanism applies to all. |
| Performance | Agree on reference library sizes and measurable limits for RAM, startup, search, and cache usage. |

## Phase 1. Environment Setup and Base Project

**Objective:** establish a minimal buildable application and a reproducible development foundation.

### 1.1. Tools

- Set up Node.js LTS, Rust with Cargo, and the Tauri CLI.
- Document platform prerequisites: Visual Studio Build Tools on Windows, Xcode tools on macOS, and GTK/WebKit dependencies on Linux, according to the requirements of the selected versions.
- Record environment versions and retain dependency lockfiles.

### 1.2. Project and Builds

- Initialize Tauri 2 with React and TypeScript.
- Prepare builds for macOS Intel and Apple Silicon, Windows x64, and Linux x64.
- Evaluate Windows ARM64 and Linux ARM64 according to the final matrix.
- Configure ESLint, Prettier, rustfmt, Clippy, and unit tests.
- Use Git and create an initial GitHub Actions pipeline for validation and builds.

**Dependencies:** scope definition and initial platform decisions.

**Deliverables:** minimal native window, base project, environment instructions, and CI pipeline.

**Acceptance and testing:** a clean clone supports dependency installation, checks, and builds on every platform declared in this phase. Record which builds were verified and the environments used.

## Phase 2. Fully Local Storage

**Objective:** preserve the library, preferences, and resources without databases or self-hosted servers.

### 2.1. StorageManager

Implement directory creation, configuration reading and writing, JSON updates, temporary files, cache cleanup, backups, and damaged-file recovery in Rust.

### 2.2. Logical Structure

```text
Player/
├── config/
│   ├── settings.json
│   ├── appearance.json
│   ├── audio.json
│   ├── providers.json
│   ├── shortcuts.json
│   └── privacy.json
├── library/
│   ├── tracks.json
│   ├── albums.json
│   ├── artists.json
│   ├── playlists.json
│   ├── favorites.json
│   ├── history.json
│   └── collections.json
├── lyrics/
├── artwork/
│   ├── animated/
│   ├── apple-music/
│   ├── archive-org/
│   ├── youtube/
│   └── local/
├── cache/
│   ├── audio/
│   ├── images/
│   ├── metadata/
│   └── temporary/
├── backups/
└── logs/
```

Resolve physical locations according to operating system conventions. Portable mode must be an explicit choice and must check write permissions. Provider JSON files contain configuration and references to protected credentials.

### 2.3. Integrity and Search

- Validate file structure and version when loading.
- Write through a temporary file and replacement, using a strategy verified on each operating system.
- Coordinate concurrent writes, create backups before major changes, and define format migrations.
- Recover the last valid state after an interruption and explain any recoverable loss to the user.
- Batch changes and evaluate handling large libraries without excessive rewrites.
- Build in-memory indexes for title, artist, album, genre, year, duration, and path; rebuild them at startup.
- Import and export configuration, library, favorites, playlists, lyrics, metadata, and preferences; exclude secrets from ordinary exports.

**Dependencies:** phase 1.

**Deliverables:** StorageManager, versioned schema, indexes, importer/exporter, backups, and recovery.

**Acceptance and testing:** the library, preferences, and playlists survive application shutdown and a computer restart. Simulate corrupted JSON, interrupted writes, a full disk, and insufficient permissions; verify recovery without overwriting the last valid copy.

**Implementation notes:** `StorageManager` (Rust, `src-tauri/src/storage/`) provides the versioned schema, atomic writes, backups, recovery, export/import, cache cleanup, and in-memory indexes; it is covered by 25 unit tests, and the settings screen exposes the real storage status. The theme is the first persisted preference. Surviving a restart and a computer restart, plus corrupted-file recovery on installed builds, still needs manual acceptance on each target platform, so this phase is tracked as "En pruebas" rather than completed.

## Phase 3. Audio Engine

**Objective:** provide stable offline playback of local music.

### 3.1. Selection and Formats

Evaluate Kira, Symphonia, and CPAL, with Rodio as an alternative, distinguishing decoding, playback, and output responsibilities. Justify the selection through testing, especially on macOS Monterey.

| Target format | Acceptance condition |
| --- | --- |
| MP3, FLAC, WAV, OGG Vorbis, Opus, and AIFF | Playback verified with reference files. |
| AAC and ALAC | Support depends on the selected decoder and testing. |
| M4A | Identify the internal codec; the extension alone does not guarantee playback. |

### 3.2. Playback and State

- Play, pause, resume, stop, skip forward/backward, and seek.
- Control volume, mute, repeat one track or the entire queue, and shuffle.
- Expose playback state, duration, position, sample rate, channels, format, and errors.
- Handle device changes and recovery after system sleep.
- Prepare support for gapless playback, crossfade, balance, speed, ReplayGain, normalization, equalization, and output selection. Processing is completed in phase 12.

**Dependencies:** phases 1 and 2; coordinate the track contract with phase 6.

**Deliverables:** audio engine selection, controls, state events, and a verified format matrix.

**Acceptance and testing:** play short and long files, different sample rates and bit depths, high-resolution audio, files with ID3 tags, embedded artwork, and damaged metadata. Verify seeking, pause/resume, and error recovery without blocking the interface.

**Implementation notes:** the selected engine, exposed controls, format matrix, and manual verification steps are documented in [audio-engine.md](audio-engine.md). The base engine is available; playback verification with reference files and Opus support remain pending.

## Phase 4. Music Library and Local Files

**Objective:** import, index, and organize music without implicitly modifying original files.

- Select individual files or multiple folders; add and remove locations and rescan them.
- Detect formats and read title, artist, album, track number, duration, year, and embedded artwork.
- Watch for new, deleted, modified, and moved files, using rescanning as a reconciliation mechanism.
- Detect potential duplicates through path, size, and metadata; use full hashes when identity needs confirmation.
- Provide songs, albums, artists, genres, folders, recently added, history, most played, and favorites views.
- Edit title, artist, album artist, album, genre, year, track number, disc number, and comments.
- Write tags to original files only when requested by the user; removing a library location does not delete its music.

**Dependencies:** phases 2 and 3; the model from phase 6.

**Deliverables:** scanner, folder monitoring, library, metadata editor, and duplicate detection.

**Acceptance and testing:** import a location twice without duplicating its entries; reconcile moved or deleted files and continue past unreadable files. Verify that internal edits do not change original files without an explicit request.

**Implementation notes:** phase 4 is implemented in `src-tauri/src/library.rs`, its native command/monitor adapter, and `src/Library.tsx`. It includes persistent locations, content-probed metadata/artwork, polling reconciliation, stable IDs across unambiguous hash-confirmed moves, duplicate reporting, grouped views, favorites/statistics, and an internal metadata editor with explicit staged tag writing. See [music-library.md](music-library.md) for the storage contract, tests, limitations, and manual platform acceptance checklist. The interface tracks this as "En pruebas" until those installed-build checks are recorded.

## Phase 5. Playlists, Queue, and History

**Objective:** manage playlists and an independent queue with session restoration.

- Create, delete, rename, and duplicate playlists; add, remove, and reorder tracks.
- Import/export M3U, M3U8, and internal JSON; evaluate PLS as an additional target.
- Add to the end of the queue or immediately after the current track; define these actions clearly in the interface.
- Remove, reorder, clear, save, and restore the queue without changing the source playlist.
- Record the last track, date, listening time, play count, and optionally playback position.
- Define when a listen counts as a play and how missing files are handled.

**Dependencies:** phases 2, 3, and 4.

**Deliverables:** playlist manager, interchange formats, persistent queue, and history.

**Acceptance and testing:** export and import a playlist while preserving order and compatible paths; restore a session and verify that queue changes do not alter saved playlists.

**Implementation notes:** phase 5 is implemented by the playlist manager/interchange module, audio-owned queue operations, and ordered session/history persistence. The interface includes persistent playlists, queue editing and save/restore, library-to-list/queue actions, and detailed listening history. See [playlists-queue.md](playlists-queue.md) for storage contracts, tests, PLS evaluation, counting/missing-file policies, and manual desktop acceptance. The phase is tracked as "En pruebas" until installed-platform acceptance is recorded.

## Phase 6. Metadata Engine

**Objective:** represent tracks from different sources with a stable internal identity.

### 6.1. Conceptual Model

```json
{
  "id": "local-track-001",
  "title": "Song Title",
  "artist": "Artist Name",
  "album": "Album Name",
  "duration": 245,
  "year": 2025,
  "track_number": 1,
  "source": "local",
  "source_id": null,
  "local_path": "/music/song.flac",
  "artwork_id": null,
  "lyrics_id": null
}
```

Duration is expressed in seconds. This example is not the final schema: document optional fields, unknown values, format version, and data specific to external sources. A cloud track does not require a local path.

### 6.2. Normalization and Identity

- Normalize whitespace and names while retaining original values.
- Distinguish Deluxe editions, remixes, and live versions; compare durations and distinguish artists with identical names.
- Identify potentially duplicated albums without automatically merging ambiguous matches.
- Retain MusicBrainz Recording/Release IDs, ISRCs, and Apple Music or other provider identifiers when available.
- Preserve internal identity even when external identifiers are missing; define how moved files are handled.

**Dependencies:** phase 2, with contracts coordinated with phases 3 and 4. Define the minimum model before implementing the library.

**Deliverables:** versioned universal schema, normalizer, and identification rules.

**Acceptance and testing:** distinguish versions with similar titles, preserve identities after restart, and handle incomplete metadata without losing entries.

**Implementation notes:** phase 6 is implemented by the universal metadata contract v1, the v2 storage envelope with v1 migration, source-identity upserts, NFC/whitespace normalization with original values, local identifier extraction, and conservative recording/album/artist comparison. The library exposes adapter JSON import, identifier search, provenance inspection and ambiguity notices. See [metadata-engine.md](metadata-engine.md) for contract details, matching rules, regression coverage and pending installed-platform acceptance. The phase is tracked as "En pruebas".

## Phase 7. Lyrics Engine

**Objective:** find, retain, and display synchronized or plain-text lyrics.

- Support LRC, extended LRC, TXT, word-level synchronization when available, and embedded lyrics where the format allows it.
- Separate adapters for LRCLIB, Lyrics Plus, other compatible providers, local files, and cache. Confirm the service and API associated with each provider before implementation.
- Score matches by title, artist, album, duration, and version; allow users to reject incorrect results and search again.
- Implement four modes: normal with an active line and scrolling; karaoke by line or word; minimalist with large text and an artwork background; full screen with unobtrusive controls.
- Adjust timing offset, size, typography, and colors; edit, save, download, and delete lyrics.

**Dependencies:** phases 2, 3, and 6; presentation integrated with phase 10.

**Deliverables:** parsers, adapters, result selection, cache, and four visual modes.

**Acceptance and testing:** maintain synchronization when pausing and seeking; apply timing offsets; display saved lyrics offline and handle missing or malformed lyrics without interrupting audio.

**Implementation notes:** phase 7 implements bounded LRC/extended LRC/TXT and generic embedded-tag adapters, LRCLIB search, LyricsPlus v2 snapshot import, reviewed candidate selection/rejection, atomic offline persistence, editing/export and four position-driven display modes. See [lyrics-engine.md](lyrics-engine.md) for timing semantics, tests, manual acceptance and the verified LyricsPlus live-service blocker. Tracked as "En pruebas".

## Phase 8. Artwork Engine

**Objective:** resolve valid artwork through independent providers and local resources.

| Priority when external search is enabled | Source |
| --- | --- |
| 1 | Apple Music: authorized animated resource. |
| 2 | Apple Music: static image. |
| 3 | Archive.org: static image. |
| 4 | YouTube: static image. |
| 5 | Embedded or associated local artwork. |
| 6 | Default image. |

- When offline or external queries are disabled, use stored resources, local artwork, or the default image directly.
- Search by artist, album, title, year, catalog identifier, and ISRC when available.
- Validate format, dimensions, artist/album match, duplicates, and provenance.
- Respect usage rights and distribution terms; store and play animated resources only when permitted.
- Control autoplay, pause, looping, transitions, quality, and battery saving; stop offscreen animations and respect reduced-motion preferences.
- Store permitted originals, thumbnails, previews, provider, source URL, date, and attribution/license information where applicable.

**Dependencies:** phases 2 and 6; presentation integrated with phase 10.

**Deliverables:** adapters, fallback search, validation, cache, and animation controls.

**Acceptance and testing:** verify priority, offline local use, and recovery from invalid images or unavailable providers. Missing artwork must not prevent music playback.

**Implementation notes:** phase 8 implements content-validated PNG/JPEG/WebP/GIF assets, local/embedded discovery, reviewed Archive.org queries/downloads, authorized provider-manifest adapters, persisted priority/manual selection, provenance and bounded animation controls across library/player/lyrics views. See [artwork-engine.md](artwork-engine.md) for provider credential blockers, supported animation formats, tests and installed-platform acceptance. Tracked as "En pruebas".

## Phase 9. External Music Sources

**Objective:** integrate local and external sources through a shared contract.

- Local provider connected to the library and audio engine.
- YouTube: search, metadata, availability, restrictions, and playback through a permitted official integration. Do not depend on unauthorized audio extraction.
- Google Drive: OAuth, browsing, search, metadata, authorized streaming and downloading, token management, and disconnection.
- Dropbox: equivalent capabilities adapted to its API.
- OneDrive: integration through Microsoft Graph.
- WebDAV: HTTPS URL, username, password, and root directory.
- Define capabilities per provider: authentication, search, reading, playback, downloading, and revocation. Do not assume all providers support the same operations.
- Manage protected credentials, token expiration, request limits, and provider-specific errors.

**Dependencies:** phases 2, 3, 4, and 6; prior validation of access to each service.

**Deliverables:** provider contract, adapters, connections, and authentication from the desktop application.

**Acceptance and testing:** connect, query, play where permitted, and disconnect each source. Test expired tokens, revoked permissions, and network loss; keep local playback operational.

## Phase 10. Graphical Interface and User Experience

**Objective:** build a consistent visual system and all player screens.

- Define navigation, components, typography, loading/error/empty states, and window behavior from the outset.
- Main window with a left sidebar, central content, and bottom player.
- Sidebar: home, search, library, songs, albums, artists, genres, playlists, favorites, history, downloads, sources, and settings.
- Home: recently played and added items, favorite albums and playlists, last playback, and recommendations based on local history.
- Global search with filters and results grouped by songs, albums, artists, playlists, and external sources.
- Bottom player: artwork, title, artist, favorite, previous, play/pause, next, progress, volume, queue, lyrics, and access to Now Playing.
- Now Playing: large or animated artwork, music information, synchronized lyrics, controls, queue, dynamic background, and colors extracted from artwork.
- Light, dark, and automatic themes, dynamic colors, and customization.
- Adapt to small, normal, maximized, and full-screen windows and different display densities.

**Dependencies:** contracts from phases 3 through 9. Design can begin after phase 1; complete integration requires the corresponding modules.

**Deliverables:** visual system, screens, reusable components, navigation, and themes.

**Acceptance and testing:** complete import, search, playback, playlist editing, and lyrics flows through the interface. Verify window sizes, empty states, and keyboard navigation.

## Phase 11. Native Integration and Accessibility

**Objective:** integrate the player with the target operating systems.

| Platform | Planned integrations |
| --- | --- |
| macOS | Menu bar, Dock, notifications, global shortcuts, media controls and keys, Media Remote where feasible, sleep handling, native window and icon, and context menu; Intel and Apple Silicon. |
| Windows | System Media Transport Controls, notifications, media keys, Jump Lists, optional startup launch, system tray, taskbar, and installer. |
| Linux | MPRIS, notifications, media keys, tray support where available in the desktop environment, and PipeWire/PulseAudio-compatible output through the selected libraries. |

Include screen reader support, contrast, scaling, visible focus, keyboard navigation, and reduced-motion preferences.

**Dependencies:** phases 1, 3, and 10.

**Deliverables:** platform integrations and a record of verified capabilities and limitations.

**Acceptance and testing:** control playback from the operating system, suspend/resume, and use the main flows with a keyboard. Verify accessibility and close/tray behavior on each target platform.

## Phase 12. Equalizer and Audio Processing

**Objective:** complete processing without playback interruptions or interface blocking.

- Ten-band equalizer: 31, 62, 125, 250, and 500 Hz; 1, 2, 4, 8, and 16 kHz.
- Flat, Rock, Pop, Classical, Jazz, Electronic, Hip-Hop, Bass Boost, Vocal, and Acoustic presets.
- Preamplification, balance, ReplayGain, normalization, crossfade, and volume.
- Save custom presets and enable/disable processing.
- Define gain headroom to prevent clipping and band behavior according to sample rate.
- Coordinate these features with gapless playback, speed, and audio engine output.

**Dependencies:** phases 2 and 3; visual controls from phase 10. Development can overlap with the final parts of the audio engine.

**Deliverables:** audio processing, presets, persistence, and controls.

**Acceptance and testing:** verify band response using reference signals, absence of clipping under defined conditions, uninterrupted parameter changes, and preset restoration. Measure processing load in the target environments.

## Phase 13. Settings and Preferences

**Objective:** centralize and persist application settings.

| Section | Planned settings |
| --- | --- |
| General | Language, startup launch, close behavior, session restoration, confirmations, and updates. |
| Appearance | Theme, accent, scale, animations, background, player layout, and lyrics style. |
| Playback | Quality where selectable for the source, initial volume, crossfade, gapless, shuffle, repeat, equalizer, and output device. |
| Lyrics | Providers, order, size, typography, color, synchronization, offset, and cache. |
| Artwork | Priority, animation, quality, autoplay, cache, permitted downloads, and local resources. |
| Storage | Locations, cache limit and cleanup, import/export, backups, and portable mode. |
| Privacy | Telemetry disabled by default, credentials, history/cache cleanup, and connection revocation. |

**Dependencies:** phase 2 and the modules exposing each setting; interface from phase 10.

**Deliverables:** settings center, validation, defaults, and persistence.

**Acceptance and testing:** save, restart, and restore settings; reject invalid values and verify that each control applies its effect. Verify that clearing the cache does not delete the library, playlists, or original files.

## Phase 14. Automatic Updates

**Objective:** distribute verified releases and offer updates within the application.

- Use GitHub Releases for versions, release notes, installers, update artifacts, signatures, and manifests.
- Compare semantic versions and check platform, architecture, and channel.
- Check at startup and manually, notify users, show release notes, download, verify signatures, install, restart when necessary, and allow postponement.
- Maintain Stable and Beta channels with separate manifests.
- Use HTTPS, protect signing keys, and reject unverified packages or packages intended for another target.
- Document recovery from interrupted installation and data migrations.
- Define specific strategies for AppImage, DEB, RPM, and Flatpak according to the selected formats, without assuming the updater covers them all.

**Dependencies:** phases 1, 2, and 13; distribution strategy coordinated with phase 16.

**Deliverables:** updater, channels, artifacts and manifests, signature verification, and release procedure.

**Acceptance and testing:** update from an earlier test version while preserving data; reject invalid signatures; test network failures, interrupted downloads, incompatible versions, and correct channel selection.

## Phase 15. Testing, Optimization, and Quality Assurance

**Objective:** validate modules and their interactions before release. Testing starts within each phase and is consolidated here.

- Functional tests for audio, library, lyrics, artwork, cloud sources, YouTube, playlists, preferences, and updates.
- Storage: unexpected shutdowns, interrupted writes, corrupted JSON, full disk, deleted files, moved folders, permissions, and excessive cache usage.
- Audio: extended playback, skipping, devices, sleep, formats, and high-resolution audio; detect dropouts and memory growth.
- Performance: measure startup, search, RAM, and CPU with reference libraries; limit cache, queries, duplicate downloads, and image/video resources.
- Security: review dependencies, permissions, credentials, signatures, input validation, and Tauri's CSP policy.

### Target Validation Matrix

| Platform | Planned minimum validation |
| --- | --- |
| macOS Monterey Intel and Apple Silicon | Installation, startup, playback, and native integration on both architectures. |
| macOS Ventura, Sonoma, Sequoia, and Tahoe 26 | Functional flows and integration on supported hardware combinations. |
| Windows 10 and 11 x64 | Installation, playback, integration, and updates. |
| Windows ARM64 | Include in the matrix if distribution is confirmed. |
| Linux Ubuntu and Fedora | Confirm versions, desktop environment, and package format; verify installation, playback, and integration. |
| Linux ARM64 | Include only if distribution is confirmed. |

**Dependencies:** release-candidate modules from phases 1 through 14.

**Deliverables:** test suite, defect reports, measurements, and a compatibility matrix with actual results.

**Acceptance and testing:** complete critical flows on committed targets, resolve release-blocking defects, and document limitations. Performance thresholds must be established before evaluating results.

## Phase 16. Version 1.0 Release

**Objective:** prepare and publish an installable, documented distribution.

- Confirm the final name, logo, icons, splash screen, visual identity, and official website.
- macOS: choose Intel, Apple Silicon, and Universal DMG packages; resolve Developer ID, signing, and notarization according to the selected strategy.
- Windows: NSIS or MSI installer, code signing, uninstaller, updates, and icons.
- Linux: AppImage, DEB, and RPM according to the agreed matrix; optional Flatpak.
- Publish an installation guide, user manual, FAQ, compatibility information, privacy policy, licenses, changelog, and bug-reporting instructions.
- Create v1.0.0 on GitHub Releases with notes and verified artifacts. This phase authorizes preparation work; actual publication takes place when the decision to launch the product is made.

**Dependencies:** phase 15 accepted, phase 14 validated, and distribution decisions resolved.

**Deliverables:** packages, user documentation, release procedure, and the published version when launch is executed.

**Acceptance and testing:** install final artifacts in clean environments, play music, verify signatures and update channels, and check download and documentation links.

## Execution Order and Dependencies

| Phases | Work |
| --- | --- |
| 1 | Environment, project, and CI. |
| 2 | Persistence, integrity, and indexes. |
| 3–6 | Audio, library, playlists, and metadata. Define the minimum contract from phase 6 before finalizing the library and playlists. |
| 7–8 | Lyrics and artwork. |
| 9 | External sources and authentication. |
| 10–13 | Complete the interface, native integration, processing, and settings. Visual design starts earlier and accompanies module development. |
| 14 | Updates and test distribution. |
| 15–16 | Final validation and launch. |

Numbering organizes deliverables; it does not require strictly sequential execution. Testing, documentation, and compatibility decisions accompany all phases.

## Planned v1.0 Scope

| Feature | Release target |
| --- | --- |
| Player, library, playlists, queue, and history | Complete, persistent local workflows. |
| Lyrics Engine and Artwork Engine | Search, local resources, cache, and presentation. |
| Animated artwork | Available when an authorized resource exists, with a static fallback. |
| YouTube | Permitted and validated integration. |
| Google Drive, Dropbox, OneDrive, and WebDAV | Working integrations within authorized capabilities. |
| Audio | Ten-band equalizer and processing committed in phases 3 and 12. |
| Themes | Light and dark, with persistent preferences. |
| Storage | Local files, without databases or mandatory self-hosted servers. |
| Updates | Verified artifacts and a validated mechanism for each distributed format. |
| Platforms | macOS Monterey through Tahoe 26, Windows 10/11, and selected Linux distributions, subject to the verified matrix. |

External services require feasibility validation before committing to a date. Any reduction in v1.0 scope must be recorded explicitly; listing integrations in this document does not mean they have been implemented.

## Features Planned for v2.0

- Synchronization across devices and libraries across computers.
- Mobile applications.
- Automatic lyrics translation.
- Advanced recommendations and statistics.
- 3D audio visualizers.
- Third-party plugins and downloadable themes.
- Additional streaming providers.
- Optional personal media server, without making it a requirement for local features.

## Development Tracking

Use tasks with the labels `critical`, `high priority`, `frontend`, `backend`, `testing`, and `documentation`. Statuses: pending, in progress, completed, and blocked.

Each phase must retain its objective, subtasks, dependencies, acceptance criteria, test evidence, documentation, and changelog. A blocker must identify its cause and the decision or work required to resolve it.

Minimum task template:

```markdown
### Phase N: name

- Status: pending
- Objective:
- Subtasks:
- Dependencies:
- Acceptance criteria:
- Tests and evidence:
- Documentation:
- Changes and decisions:
- Blockers:
```

No remote tasks have been created, and no phase is marked as completed. The statuses shown in the development interface ("Base disponible", "En pruebas", "Vista previa") summarize implementation and automated coverage only; they do not replace the manual acceptance recorded for each phase.
