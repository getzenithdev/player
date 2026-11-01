# Phase 5 — Playlists, Queue, and History

Implementation status: available, **En pruebas** until installed-build acceptance is recorded on each desktop target. These are persistent local workflows, with no database or server dependency.

## Playlists

**Listas** creates, renames, duplicates, and deletes playlists. Add files using the native picker or add a library track through **Añadir a lista…** in **Biblioteca**. The arrow buttons reorder entries; **Quitar** removes one occurrence. The same song may appear more than once: every occurrence has its own stable entry ID. Duplicating a list creates independent list and entry IDs.

**Reproducir lista** copies its entries into an independent queue. Playing from a particular row starts that row followed by the remaining rows and then the preceding rows. Missing entries stay visible, and playback starts with the first available file. Changing the queue never changes the saved playlist. **Guardar como lista** in **Cola** creates another independent list from the current queue.

The shared storage envelope (new writes v2, with v1 read compatibility) for `library/playlists.json` stores `data.playlists`. Entries retain a stable library track ID where available and a fallback local path, title, format, duration in milliseconds, and an availability hint. Paths/titles resolve against current library metadata on read; moving a library file therefore follows the library's stable identity. Removing a library location does not delete playlist entries or their files; an entry with an unavailable path remains visible.

## Import and export

Native dialogs import and export M3U, M3U8, and Zenith JSON. UTF-8, a leading BOM, CRLF, `#EXTINF` titles/durations, duplicate occurrences, and relative local paths are supported. Relative paths resolve from the imported playlist's parent directory. Exports use paths relative to the destination folder when the media is beneath it; other paths remain absolute. Foreign Windows drive/UNC paths are preserved when importing on another OS, where they remain unavailable until accessible under that path.

The internal JSON exchange is `{ "format": "zenith-playlist", "version": 1, "name": "…", "entries": […] }`, separate from the storage document's envelope. Import gives the playlist and each occurrence new IDs, and rebinds library identity from the local path instead of trusting IDs from another library. Missing local files are retained in order. Imports reject malformed JSON, unsupported versions, URLs, invalid local paths, files over 10 MiB, and lists over 100,000 entries without modifying existing playlists. Legacy non-UTF-8 M3U text is rejected with an encoding error rather than silently damaging names.

Export stages and synchronizes a temporary file beside the destination before replacement. Only matching `.m3u`, `.m3u8`, or `.json` destinations are accepted; original audio is never an export target. No import/export operation copies or changes music files. PLS was evaluated and deferred: M3U/M3U8 and versioned JSON cover the committed interchange scope; another parser would add compatibility surface without a concrete PLS requirement.

## Queue and session

**Cola** supports these distinct actions:

| Action | Result |
| --- | --- |
| Abrir y reemplazar cola | Select files and replace the queue, starting playback. |
| Añadir al final | Append selected tracks after all waiting entries. |
| Reproducir después | Insert selected tracks, in order, immediately after the current entry; append if no entry is selected. It does not interrupt the current track. |
| Arrows | Move one stable entry ID before another, preserving the playing entry and its position. |
| Quitar de cola | Remove one occurrence. If it is current, select the next remaining slot and preserve playing/paused intent. |
| Vaciar cola | Stop and clear the queue; playlists and history remain. |
| Guardar copia de la cola | Persist an explicit copy of the current queue, selected entry, position and controls. |
| Restaurar copia guardada | Restore that copy without autoplay, independently of later automatic checkpoints. |
| Guardar como lista | Create a new saved playlist from the queue. |

The queue, selected entry ID, position, volume, mute, repeat and shuffle are checkpointed on state changes and every five seconds during playback. A normal window close requests a final flush with a bounded wait. A crash/forced termination can lose the last checkpoint interval. The restored session never autoplays: the next press of Play loads the saved track and seeks to its position. Unsupported seeking falls back to the start with a visible explanation.

Missing queue entries are retained and skipped when advancing. If none remain available, traversal stops; repeat/shuffle cannot loop forever searching for a file. A missing restored current track displays a playback error if explicitly resumed; Next can move to an available entry. A file that exists but cannot be decoded still reports the audio engine's error; existence alone is not a codec compatibility guarantee.

Queue remove/move operations use entry IDs, so repeated paths remain distinguishable. The audio actor owns queue mutations. A separate ordered writer performs JSON I/O, coalesces pending checkpoints for the same visit, and preserves track-boundary, explicit-save, and flush messages. Scans and storage imports share the storage operation lock; the output mixer is not used for persistence.

## History and counting

`library/history.json` is the single versioned commit for the automatic session, explicit saved queue, last 1,000 listening visits, and cumulative per-track counters. Keeping these in one document makes a repeated checkpoint idempotent and avoids a session/counter split across two commits. Detailed visits are bounded; cumulative counters are retained when older visit rows are pruned.

A visit starts when a track successfully loads. Actual decoder position progress is sampled every 200 ms. Pauses, seeks, device resets, and discontinuities over one second do not add listening time. Output recovery and automatic session restoration retain the visit ID and credited time. A deliberate new playback or explicit saved-queue replay starts a new visit. A play counts once when that visit reaches 30 seconds of credited audio; shorter visits are retained with their listening time and do not increment play count. The final duration sample at natural completion is included when known.

**Historial** shows date, credited seconds, counted/short-listen status and replay for available entries, including tracks opened directly outside the library. The library's **Historial** and **Más escuchadas** views overlay these counters on phase 4's legacy statistics without rewriting or double-counting them. Restoring/replaying the queue and changing a playlist do not reset counters. History stores the last position; queue/session restoration uses its own selected-entry position.

## Validation and acceptance

Automated tests cover playlist CRUD/reordering/independent duplicates and persistence, M3U/M3U8/JSON round trips with missing files and Unicode, BOM/CRLF/EXTINF/relative paths, rejection without partial mutation, moved library identity, stable queue entry ordering, session reopen, explicit saved queue versus automatic session, queue/source-playlist isolation, idempotent cumulative counters, short visits, and pause/seek/reset/suspension accounting. Existing storage corruption/recovery and audio policy tests remain active.

Recorded validation: **56 Rust tests** and **12 frontend tests** pass; Rust formatting, Clippy with warnings denied, frontend typechecking/lint/format, and the production frontend build pass. A browser smoke test with simulated native responses passed playlist creation/addition/duplication/export, queue append/reorder/save/clear/restore/save-as-playlist, history rendering, and 1100px/640px layouts with no JavaScript runtime errors. Queue screens were visually inspected. These are automated and simulated-runtime checks, not installed-platform acceptance.

Run `npm run check`, `npm run build`, `npm run rust:fmt`, `npm run rust:test`, and `npm run rust:lint`. Native tests do not require an audio device. Browser smoke tests with simulated IPC check UI integration but do not establish native dialog or physical playback compatibility.

Installed-build acceptance remains pending on macOS Intel/Apple Silicon, Windows x64, and Linux x64:

1. Create, rename, duplicate, reorder, export and import lists with Unicode paths, repeated entries, and missing media. Restart and compare ordering.
2. Play a list, append/play-next/remove/reorder/clear its queue, and confirm its playlist bytes are unchanged. Save and restore an explicit queue copy after further mutations.
3. Close during playing and paused states, reopen, verify there is no autoplay, and resume the same track/position. Test force termination and the checkpoint interval.
4. Listen for less/more than 30 seconds; pause, seek, recover output, repeat and restart the app. Verify per-visit seconds and play counts without double counting.
5. Move/delete/restore music and disconnect a drive; verify missing entries stay visible, valid entries can be reached, and all-missing repeat/shuffle terminates.
6. Test read-only/full storage, damaged history JSON and backup recovery, blocked exports, native file dialogs, keyboard navigation, and light/dark/small-window presentation.

## Resumen en español

La fase 5 permite guardar, editar, duplicar y ordenar listas, importar/exportar M3U/M3U8/JSON y gestionar una cola independiente. La sesión se recupera sin reproducirse automáticamente y puede guardarse una copia explícita de la cola. El historial conserva tiempo real de escucha, fecha, posición y reproducciones: se contabiliza una después de 30 segundos efectivos, sin sumar pausas ni búsquedas. Los archivos ausentes se conservan y no se borra música al quitar entradas de una lista o de la cola.
