# Music Library and Local Files — Phase 4

Status: implemented, with desktop platform acceptance pending. The interface labels this phase **En pruebas** so automated checks are not confused with verified macOS/Windows/Linux installations.

## Using the library

Open **Biblioteca** and choose **Agregar archivos** or **Agregar carpetas**. Both dialogs support multiple selections. Music stays in its original location; importing never copies, renames, deletes, or rewrites it. Opening files directly from **Cola** does not import them into the library; phase 5 now persists that independent queue and its listening history.

Search matches title, artist, album artist, album, genre, year, and path. Library views include songs, albums (separated by album artist), artists, genres, folders, recently added, favorites, history, most played, and duplicates. Click a song to play the available songs in the current view, beginning with that song. Missing files remain visible and cannot be played.

Expand **Ubicaciones** to remove a location. This only removes its library entries; a track covered by another selected location remains. Removing a location never deletes its music. Favorites, internal edits, and statistics for entries removed this way are removed from the library too; reimporting them creates new entries.

**Volver a explorar** performs immediate reconciliation. A background polling monitor does the same every ten seconds while the desktop app is running. Deleted or disconnected files remain marked unavailable, preserving user data until restored. New and modified files are read on the next pass; unchanged files reuse cached metadata and hashes. Restarting also starts monitoring stored locations.

## Metadata and original files

Lofty probes file contents and reads title, artist, album artist, album, genre, date/year, track/disc number, comments, duration, format, and embedded artwork. Missing titles fall back to the filename. Embedded JPEG/PNG artwork up to 10 MiB is stored under `artwork/local/` with a SHA-256 filename and served through a bounded native command; no unrestricted filesystem asset scope is enabled. Other image formats fall back to the placeholder.

The scanner considers MP3, FLAC, WAV, OGG/OGA, Opus, AIFF/AIF/AIFC, M4A/MP4, AAC, ALAC, and CAF candidates. A recognized suffix is only a candidate: content probing and decoder capability determine actual support. Invalid or unsupported metadata is reported per file without interrupting the remaining import. Opus metadata import does not add Opus playback support to the phase 3 engine.

**Editar → Guardar en biblioteca** persists a metadata overlay. Original bytes remain unchanged, including after later scans. Explicit internal edits take precedence over subsequent external tag changes; unedited tracks pick up external changes.

Writing tags requires selecting **Escribir también las etiquetas en el archivo original** and pressing **Guardar y escribir archivo**. The rewrite is staged beside the original and synchronized before replacement. Encoding failures leave the original intact, a concurrent content change cancels replacement, and an exact uniquely named `.zenith-….bak` copy remains beside the music for recovery. Read-only directories or files produce an error. Supported tag fields depend on the format. A tag write and the JSON commit cannot be one filesystem transaction; if an interruption occurs between them, the next scan reconciles the file and the recovery copy remains available.

## Persistence and identity contract

The shared storage envelope (new writes v2, with v1 migration support) for `library/tracks.json` stores `data.tracks` and `data.locations`. Locations default to an empty array when loading older data. Each track retains the shared phase 2/6 fields plus album artist, comments, format, file size, modification timestamp, SHA-256, import date, availability, favorites, internal-edit flag, play count, and last-played time. Unknown metadata is `null`. Duration is whole seconds; timestamps are Unix seconds except `modified_ns`, an internal nanosecond cache key. Local IDs are randomly generated, independent of the path.

All library commands, background reconciliation, history updates, and storage import/export share the storage operation lock. A mutation commits tracks, locations, metadata overlays, and user state in one atomic JSON document, with the existing backup/recovery mechanism, then rebuilds search indexes. Grouped albums/artists/genres are derived views and are not separately rewritten. Favorites and legacy phase 4 counters remain with their track. Phase 5 now stores playlists separately and session/history/cumulative listening data in `library/history.json`, overlaying its counters on library views. See [playlists-queue.md](playlists-queue.md).

Canonical selected paths and a set of discovered paths prevent repeated and overlapping imports from creating entries twice. Nested symlinks are skipped to avoid cycles or unexpected traversal. Explicitly selecting a symlink resolves its target first. A single missing entry with an identical full-file SHA-256 is treated as a move and retains its ID, favorites, edits, and statistics. If several missing entries share the hash, the scanner preserves the ambiguity and creates a new entry rather than guessing.

Duplicate detection reports full-file SHA-256 matches as confirmed byte-identical copies. Matching size, normalized title/artist/album and duration indicates potential duplicates. Copies remain separate entries; nothing is automatically merged or deleted. Audio with different tags can have different whole-file hashes, even if its audio payload is the same.

History/most-played count one play after thirty seconds of actual advancing playback per started track. Pausing, stalled output, and seeking do not add listening time; output recovery preserves the counter. Shorter listens do not increment play count. Phase 5 now retains these short visits and their credited listening time in the detailed history, alongside session restoration; see [playlists-queue.md](playlists-queue.md).

## Validation

Automated Rust tests create real PCM WAV fixtures, including embedded tags/artwork, without requiring an audio device. They cover repeated and overlapping imports, content probing with a misleading extension, additions/changes/deletions/moves/restoration, ambiguous identical files, corrupt-file continuation, symlink cycles, concurrent scan/edit, persistence across reopening, explicit tag write/readback and recovery copies, and internal edits without original-file changes. Existing storage tests remain in place. Frontend tests exercise search, grouping, favorites, history, recently added, most-played and duplicate selection.

Recorded validation for this implementation:

- `npm run check`: TypeScript, ESLint, Prettier, and 9 Vitest tests passed.
- `npm run build`: production frontend build passed.
- `cargo fmt -- --check` and `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: 44 Rust tests passed (12 library, 25 storage, 7 audio), using Linux native dependencies and no audio device.
- Browser smoke test with simulated native responses: library rows, favorites, metadata save, album grouping, a 640px viewport, and Escape dismissal passed with no runtime JavaScript errors. Screens were visually inspected at 1100px and 640px. This checks presentation/IPC usage, not native dialog or device behavior.

Run:

```sh
npm run check
npm run build
npm run rust:fmt
npm run rust:lint
npm run rust:test
```

Before calling the phase platform-accepted, run these manual checks on installed macOS Intel/Apple Silicon, Windows x64, and Linux x64 builds:

1. Import files and multiple folders, including overlaps; restart and confirm tracks, locations, edits, and favorites persist.
2. Add, rename, move, edit, delete, and restore music while the app is open; verify the ten-second reconciliation and explicit rescan.
3. Disconnect a drive or remove access permissions; verify warnings and unavailable tracks, then reconnect it.
4. Play an imported track, pause/resume and seek; verify statistics after thirty seconds of actual playback.
5. Edit metadata internally and compare original bytes. Explicitly write tags to a disposable copy, inspect readback and the exact backup, and repeat on read-only media.
6. Verify embedded artwork, empty/error states, keyboard focus, dialog cancellation and light/dark themes. Test supported reference formats; WAV automated coverage alone is not a complete format matrix.
7. Measure startup, scan duration and memory with an agreed reference library. The current implementation reads all candidate bytes once for initial hashes and rewrites the JSON on changes; a very large library requires performance measurement before any scalability claim.

Phase 6 enriches each record with a universal metadata contract, preserved originals and source identifiers; see [metadata-engine.md](metadata-engine.md).

## Cambios para usuarios

La biblioteca ya permite agregar archivos o varias carpetas, buscar y explorar música, reproducirla, guardar favoritos y editar metadatos. Las ubicaciones se revisan automáticamente y los archivos ausentes se conservan para poder recuperarlos. Las posibles copias se muestran sin borrar ni fusionar música. Editar en Zenith no modifica el archivo original: la escritura de etiquetas es una opción explícita y deja una copia de recuperación junto al archivo.
