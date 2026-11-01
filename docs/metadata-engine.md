# Phase 6 — Universal Metadata and Identity

Implemented in the Rust metadata engine and library UI; status **En pruebas** until installed-platform acceptance is recorded. This phase supplies a provider-neutral data contract, normalization, identity rules and review candidates. It does not connect external services or enable their playback.

## Versioned contract

The shared storage envelope now writes **version 2**. Version 1 payloads remain readable and are preserved through migration. This version barrier prevents older builds from writing the newly enriched track document and discarding fields they do not understand. Ordinary storage backups/recovery apply; preparing legacy tracks commits their metadata and any missing internal IDs atomically, preserving existing IDs, favorites, counters and references. Duplicate internal IDs or newer metadata versions are rejected instead of guessed or overwritten.

Within every track, `metadata.version` is **1**, independently of the envelope and interchange format. The existing top-level fields remain compatible:

| Field | Contract |
| --- | --- |
| `id` | Durable internal library-entry ID. Generated once when absent; never derived from title or a provider's ID. |
| `title` | Display string; empty means unknown. The UI supplies an unknown-title label. |
| `artist`, `album_artist`, `album`, `genre`, `comments` | Optional strings; unknown values are `null`. Names are normalized; comments preserve line breaks. |
| `duration` | Optional whole seconds, not milliseconds. Queue interchange still uses `durationMs`. |
| `year`, `track_number`, `disc_number` | Optional unsigned values. |
| `source` | Source namespace, e.g. `local`, `apple_music`, `youtube`, `google_drive`, or another adapter namespace. Legacy records without a source become `local` when a path exists, otherwise `unknown`. |
| `source_id` | Optional opaque, case-sensitive identifier inside that source's namespace. It is not a global ID. |
| `local_path` | Optional absolute path. Cloud/catalog records do not require it and cannot be played by the local engine without a local file. |
| `artwork_id`, `lyrics_id` | Optional references; absence does not remove the track. |
| `metadata` | Versioned original names, comparison keys, identifiers, variant fields, grouping keys and inert provider-specific metadata. |

Provider-specific metadata belongs in `metadata.provider_data`; authentication material belongs to protected provider storage, not this record. No provider data triggers a network request or executes code.

## Normalization and original values

Display names are trimmed, repeated Unicode whitespace is collapsed, and text is normalized to NFC. Comparison/search keys additionally lowercase. Accents, punctuation and version text are retained; this engine does not remove parenthesized text or strip accents to force a match.

`metadata.original` keeps the received title, artist, album artist, album, genre and comments before normalization. An explicit internal edit changes the display values without replacing the observed originals. A later provider/file refresh records its new raw names while preserving a user's internal edit overlay. Missing identifiers on a later scan do not erase retained identifiers. Source IDs and paths are opaque and are not name-normalized.

The local scanner reads MusicBrainz recording/release/release-group/artist/album-artist IDs, ISRC, Apple XID where available, track subtitle/version, remixer, catalog number, release country and release type through Lofty. Existing cached files get a one-time identifier extraction on their next reconciliation. This does not write their tags.

## Identification rules

- Re-ingesting the same nonempty `(source, source_id)` refreshes the same entry and preserves its internal ID, user edits, favorites, counters and known local resource references.
- Local input can also resolve the exact existing local path. Files moved in the scanner retain the phase 4 rule: only one missing entry with the same full-file SHA-256 can inherit that ID; ambiguous matches stay separate.
- Different source namespaces with the same source ID remain distinct. An input without a durable source ID or matching local path receives a new entry ID; title similarity is never an upsert key.
- MusicBrainz and provider IDs are retained exactly as supplied. UUID-shaped MusicBrainz IDs and structurally valid ISRCs can support comparison. Invalid values remain available for inspection but are not trusted for matching. Single-ID MusicBrainz fields containing a composite/noncanonical value remain raw rather than being split into guessed identities.
- Provider artist/album/track IDs are namespaced maps. Identical strings from different providers do not establish equivalence; different IDs in the same provider namespace, or distinct MusicBrainz artist IDs, distinguish same-name artists.
- Existing user statistics remain stored as before. Metadata identity preparation does not rebuild IDs from recording identifiers or migrate listening counts across unrelated entries.

## Versions, duration and candidates

Full normalized titles remain part of comparison, with explicit edition, track-version and remixer fields. Common qualifiers include live, remix, remastered/remaster, acoustic, instrumental, deluxe, demo, mono, stereo and extended. Unrecognized qualifiers are still preserved in the full title; they are not discarded. Different qualifiers/version fields or known durations differing by more than two whole seconds prevent a recording match.

Matching results distinguish identical full-file bytes, a shared recording identifier, a possible recording based on title/artist/duration, different recordings, and insufficient information. A shared recording ID can describe the same recording on different releases; it does not equate those library entries or album editions. No result automatically merges records, edits tags, deletes files or transfers favorites/history.

The library's duplicate report keeps SHA-256-confirmed copies separate from metadata/duration candidates. Conservative candidate buckets use normalized title, artist grouping identity and version fields; not every cross-provider alias will be suggested. A missing artist/duration is insufficient for a name-based candidate, rather than a reason to drop the record.

Artist grouping uses a known MusicBrainz/provider identity when available. Name-only buckets remain explicitly unresolved; the engine cannot infer whether two indistinguishable names are one person. Known identities and unresolved name buckets stay separate, and same-name collisions are reported for review.

Album grouping uses release identity when available, normalized full album title, album-artist identity/name, explicit edition and year. Unknown albums/artists are not all collapsed into one invented identity. Potential duplicate album groups are reported when compatible album/album-artist/edition names lead to different release/identity/year groups. Groups remain separate; a candidate is not a confirmed duplicate. Various-artists albums do not reuse a track artist's ID as the identity of an explicitly different album artist.

## Adapter interchange and UI

**Biblioteca → Importar metadatos JSON** accepts a bounded adapter snapshot without requiring a service connection. For example:

```json
{
  "format": "zenith-metadata",
  "version": 1,
  "tracks": [
    {
      "source": "custom_provider",
      "source_id": "catalog-item-123",
      "title": "Song Title",
      "artist": "Artist Name",
      "album": "Album Name (Deluxe)",
      "duration": 245,
      "edition": "Deluxe",
      "identifiers": {
        "provider_track_ids": { "custom_provider": "catalog-item-123" },
        "provider_artist_ids": { "custom_provider": "artist-456" }
      },
      "provider_data": { "territory": "MX" }
    }
  ]
}
```

The import excludes caller-controlled internal IDs and user state, resolves known source identities, and validates the entire snapshot on a copy before committing. Unsupported format/version, invalid source namespaces, invalid local paths, ambiguous source identities, files over 10 MiB or snapshots over 100,000 entries fail without partially importing entries. Normal legacy preparation, if needed before the import, may already have created its own backup/migration.

The library searches normalized names and retained source/recording/provider identifiers. Artist/album view grouping uses stable grouping keys, independently of display labels. Review notices expose ambiguous albums and same-name artist identities. The metadata editor shows read-only source identifiers and original names. Records without a local file stay browsable/editable/favoritable while local playback/queue controls remain disabled.

## Validation

Recorded automated checks: **70 Rust tests**, **15 frontend tests**, Clippy with warnings denied, Rust/Prettier formatting, TypeScript, ESLint and the production frontend build. Tests cover Unicode normalization and original preservation, v1→v2 migration/backups, stable IDs after reopen, future-version refusal, live/remix/Deluxe/duration distinctions, same-name artist IDs, namespace separation, raw/invalid IDs, duplicate album candidates, cloud records without paths, provider refresh/user state, incomplete data, atomic JSON import, source-data round trips and embedded MusicBrainz/ISRC extraction from actual WAV tags. Existing file-move, playlist, session, history and storage regression tests remain active.

Native tests use fixtures and do not need an output device. Browser smoke checks use simulated native responses; these do not establish installed application compatibility. Before platform acceptance, verify legacy-data startup/backups, metadata JSON import, local-tag refresh, ambiguous groups, original names and identifiers, favorites/playlists/history continuity, and restore/reopen on macOS Intel/Apple Silicon, Windows x64 and Linux x64. Re-measure startup/reconciliation with a reference large library; first identifier extraction hashes/reads changed legacy cache entries and commits a versioned document.

## Resumen en español

La fase 6 añade un modelo común para metadatos locales y de futuras fuentes externas. Normaliza nombres conservando los originales, retiene MusicBrainz/ISRC e identificadores de proveedor, mantiene identidades internas estables y distingue versiones, ediciones y artistas homónimos cuando hay información. Las posibles coincidencias se muestran para revisión y nunca se fusionan automáticamente. La migración crea respaldos y protege los nuevos datos frente a versiones antiguas de la aplicación.
