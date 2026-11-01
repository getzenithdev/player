# Changelog

## Unreleased — Phase 5

- Persist playlists with create, rename, duplicate, delete, add/remove and reorder operations, preserving duplicate song occurrences.
- Import/export UTF-8 M3U, M3U8, and versioned Zenith JSON with local relative-path resolution and missing-file preservation.
- Add independent queue append/play-next, stable-entry reordering/removal, explicit save/restore, and save-as-playlist actions.
- Restore queue, selected track, position and playback controls after restart without autoplay, with five-second checkpoints and a final close flush.
- Record detailed listening visits, credited time, last position and cumulative play counts; count a play once after 30 effective seconds.
- Follow moved library identities, skip missing entries during navigation, and retain missing entries for recovery.
- Replace preview-only playlists/queue UI with native workflows, library actions and a dedicated history view.
- Add backend/frontend regression tests and document persistence, interchange, counting rules and desktop acceptance limitations.
