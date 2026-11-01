# Changelog

## Unreleased — Phase 7

- Add offline lyrics storage, LRC/extended LRC/TXT and embedded-tag readers, LRCLIB search and LyricsPlus v2 snapshot import.
- Add result ranking/rejection, local editing/export, per-song/global timing and four audio-position-driven display modes.
- Add bounded provider requests, opt-in external queries and regression coverage; document the unavailable LyricsPlus live service.

## Unreleased — Phase 6

- Add universal metadata v1 with retained original names, Unicode NFC/whitespace normalization and provider-neutral identities.
- Upgrade storage writes to v2 with v1 migration/backups and downgrade protection.
- Retain MusicBrainz/ISRC/Apple XID and namespaced provider identifiers, including provider-specific metadata.
- Import metadata snapshots with stable source-identity refresh, preserving user state and records without local files.
- Separate artist/release identities and report recording/album candidates without automatic merges.
- Add identifier search, original/provenance inspection and migration/identity regression coverage.

## Phase 5

- Persist playlists with create, rename, duplicate, delete, add/remove and reorder operations, preserving duplicate song occurrences.
- Import/export UTF-8 M3U, M3U8, and versioned Zenith JSON with local relative-path resolution and missing-file preservation.
- Add independent queue append/play-next, stable-entry reordering/removal, explicit save/restore, and save-as-playlist actions.
- Restore queue, selected track, position and playback controls after restart without autoplay, with five-second checkpoints and a final close flush.
- Record detailed listening visits, credited time, last position and cumulative play counts; count a play once after 30 effective seconds.
- Follow moved library identities, skip missing entries during navigation, and retain missing entries for recovery.
- Replace preview-only playlists/queue UI with native workflows, library actions and a dedicated history view.
- Add backend/frontend regression tests and document persistence, interchange, counting rules and desktop acceptance limitations.
