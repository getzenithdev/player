# Changelog

## Unreleased — Expanded player

- Simplify copy across the app, remove repeated slogans/headings, consolidate library views into a selector and move secondary queue/provider details into disclosures.
- Add anchored, interruptible player expansion without bounce, subtle press/menu/artwork feedback, and motion/transparency/contrast preference support.
- Expand the mini player over the current page instead of navigating to a separate Now Playing section.
- Show cover art and playback controls on the left, synchronized lyrics on the right, and a blurred cover background.
- Preserve the underlying page/filter/scroll and audio controller; support minimize, Escape, restored focus, queue switching and lyric/artwork editing.
- Share the lyric reading surface with the existing manager, retain plain-text/karaoke display, and avoid showing a previous track's lyrics during a track change.

## Unreleased — Phase 10

- Replace the phase-review workspace with a listening app inspired by the supplied HTML and the real Zenith icon.
- Add Inicio with real library/history shelves, grouped search, collection galleries and focused Now Playing with artwork, lyrics and queue.
- Keep playback persistent; add current-track favorites, lyrics/queue access, light/dark/system themes and artwork-derived backgrounds.
- Preserve previous import, editing, playlist, lyric, artwork, source and storage workflows through menus and consolidated settings.
- Add offline cached-download browsing and regression coverage; review browser layouts at normal and minimum desktop window sizes.
- Record the user's acceptance of phases 1–9 and keep phase 10 awaiting final visual acceptance.

## CI portability fixes

- Give view components distinct Panel filenames and reject case-insensitive module collisions before Quality/build checks.

- Normalize location-removal paths using the same canonical identity as import on macOS and Windows.
- Flush staged metadata files through writable handles on Windows; preserve original/recovery-copy safeguards.
- Run Intel macOS validation on the actual Intel runner instead of repeating ARM tests.

## Unreleased — Phase 9

- Add one external-source contract with declared capabilities, connection states, an explicit external opt-in and a single `sources_action` command.
- Connect local, YouTube (API-key metadata only), Google Drive, Dropbox, OneDrive (OAuth 2.0 with PKCE and loopback redirect) and HTTPS WebDAV sources.
- Keep connections and non-secret settings in the export-excluded `config/providers.json` while passwords, keys and tokens stay in the operating system credential store.
- Add search, folder browsing, validated identifiers, single-store downloads to `cache/audio/` and playback through the audio engine, with Spanish provider errors.
- Recover pending OAuth URLs after refreshing the screen, preserve unrelated authorizations on disconnect, and allow reconnecting after cancellation, timeout, restart or failed callbacks.
- Correct Dropbox scopes and bearer-authenticated revocation; declare OneDrive local disconnection without advertising unsupported remote revocation.
- Prefill saved connection settings, clear submitted secrets, allow expired credentials to be forgotten, and restrict playback/download actions by result kind.
- Cover the contract with 15 serial Rust tests and frontend unit tests; manual acceptance with real credentials is still pending.

## Unreleased — Phase 8

- Add validated content-addressed artwork originals, previews, thumbnails and provenance with offline fallback and manual priority overrides.
- Add local/embedded discovery, opt-in Archive.org search with license checks, and authorized Apple Music/YouTube manifest import.
- Connect artwork to library, player and lyrics backgrounds; add bounded GIF animation with pause, looping, visibility, reduced-motion and battery controls.
- Verify LyricsPlus v2 `syllabus` timing against its source and retain import compatibility with `syllables`.

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

## YouTube automático and provider setup guides

- Synchronize the fork with Zenith through `162ad41`, preserving the phase 9 and phase 10 UI work.
- Add primary keyless YouTube search and AAC/M4A cache preparation through packaged yt-dlp + Node 22, bounded execution and cancellation. Preserve the existing API as a metadata-only alternative.
- Add provider-specific setup guides and official-site buttons; align Dropbox and OneDrive callbacks with documented desktop registrations.
- Accept Node 22 for Monterey development and check macOS deployment metadata for the app and helper executables in CI. Actual Monterey runtime acceptance remains pending.
