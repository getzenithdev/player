# Lyrics engine / Motor de letras (Phase 7)

The native lyrics command runs file inspection and bounded network requests off the audio thread. `library/lyrics.json` is an atomic, versioned document containing selected lyrics, provenance, rejection IDs, per-track offset and display preferences. Saved selections survive restart, are exported by the storage bundle, and remain available offline. Removing a selection never edits the audio file or its sidecar.

## Formats and timing

- UTF-8 TXT, LRC and extended LRC: BOM/CRLF, repeated line stamps, fractional timestamps, metadata, `[offset:]`, inline `<mm:ss.mmm>` word times and Unicode.
- Embedded `Lyrics` and `UnsyncLyrics` tags through Lofty, plus same-name `.lrc`/`.txt` sidecars. Binary ID3 SYLT is not converted by Lofty's generic tag API; use an LRC sidecar for those recordings.
- Malformed stamps remain visible with warnings; invalid word order falls back to line timing. Input is limited to 1 MiB/10,000 lines and 24-hour timestamps.
- The highlighted line/word is derived directly from audio position, including after a seek, pause, resume or queue repeat. No independent lyrics clock drifts during pause.
- Effective offset = file LRC offset + global preference + song adjustment. A positive offset displays lyrics earlier. UI offsets are bounded to ten minutes.
- Normal, line/word karaoke, large minimalist text, and actual browser fullscreen with Escape/exit control. Typography, text/active colors and size persist. Reduced motion disables smooth scrolling.

## Provider verification (2026-11-01)

- LRCLIB: https://lrclib.net/docs.txt documents `GET /api/search`, title/artist/album/duration fields and plain/synced text. Native requests identify Zenith, are serial, use timeouts/response limits, and respect `429`/`503` Retry-After through a shared cooldown. External queries are disabled by default. Search sends song metadata only when explicitly requested; no library-wide background requests.
- Ranking: exact normalized title 45, artist 30, album 10, duration within 2 seconds 15. Edition/remix/live words remain part of title; results are always proposals requiring selection. Rejected IDs are retained locally and excluded from subsequent searches. Edited text never gets replaced by a search.
- LyricsPlus refers specifically to https://github.com/ibratabian17/LyricsPlus (YouLy+, cookie branch), not the unrelated Spicetify extension. Its README declares its public host unavailable and its upstreams require account credentials. Zenith imports exported v2 JSON timelines (`lyrics[].time`, text and optional timed `syllabus[]` words) without contacting undocumented mirrors or handling third-party account cookies. Live LyricsPlus search remains blocked until a documented authorized service is available.
- Cache/local adapters are separate from the live provider. A provider outage returns a warning and retains the current selection. Instrumental/empty provider records do not erase lyrics.

## User flows and tests

Select or follow a library recording, open LRC/TXT or LyricsPlus JSON, review the result, and use **Usar y guardar**. Edit locally, save, export LRC/TXT through a native save dialog, reject mismatches or remove the saved selection. Plain-text exports remove timestamps; LRC exports preserve the saved source text (display-only offsets remain preferences).

Automated tests cover stamp fractions/repeats/order, word synchronization, Unicode/offsets, malformed/plain input, invalid preferences, duration/version scoring, LyricsPlus timeline conversion and save/restart/reject/delete. Frontend tests cover seek boundaries, negative/positive offsets and word highlighting using the same audio clock. Installed desktop fullscreen/dialog and real-service availability checks remain manual acceptance tasks, not compatibility claims.

## Español

Las letras se guardan localmente por identidad de canción. Puedes importar, elegir, rechazar, editar, descargar y quitar letras sin modificar tus archivos de música. El desfase, la tipografía, los colores y los cuatro modos se conservan. La sincronización sigue la posición real del audio al pausar o saltar. LRCLIB solo se consulta al activar las búsquedas y solicitarlas. LyricsPlus admite importación de respuestas v2; la consulta en vivo está bloqueada por la disponibilidad del servicio documentado. Falta registrar la aceptación manual de las aplicaciones instaladas en cada plataforma.
