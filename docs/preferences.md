# Settings and Preferences (Phase 13)

The settings center is available under Configuración → Preferencias. Seven sections expose general behavior, appearance, playback, lyrics, artwork, storage, and privacy. Values are stored locally in the versioned config/settings.json document through StorageManager. Credentials never enter this document.

## Behavior

- Appearance applies globally: theme, accent, interface scale (75–200%), reduced animations, background, player layout, and lyric layout.
- Playback defaults apply on startup only when no queue/session is restored. Volume, shuffle, and repeat also apply immediately when changed in settings.
- Lyrics supports local/embedded/LRCLIB selection, size, font, colors, and global offset. External access still requires the existing provider permission. Local and embedded proposals honor the chosen providers.
- Artwork supports reorderable provider priority, animation, looping, battery saver, quality, and permitted downloads. Explicit per-track selections take precedence over automatic priority.
- Existing theme, lyrics, and artwork documents are mirrored on save. Later changes from their module screens are loaded back into the settings center.
- Edits autosave after 400 ms, flush on navigation, and can be explicitly saved or reset. Loading failures do not enable saving defaults over an unreadable configuration.
- Cache cleanup, backups, import/export, and connection revocation reuse the existing storage/provider implementations. Cache cleanup preserves library documents, playlists, history, and original files.

Native startup, close behavior, session restoration, shortcuts, and accessibility remain in Sistema/Accesibilidad. DSP remains in Audio/Ecualizador, persisted independently in config/audio.json. Portable mode is displayed from actual storage status and remains an explicit startup choice.

The interface currently ships in Spanish. System-language preference is reserved for future translations. Automatic updates use the signed Stable/Beta feeds described in [updates.md](updates.md); selectable output devices, gapless playback, and overlapping crossfade remain engine limitations. There is no telemetry collector or quality selector for sources that do not expose one. Cache size limits and automatic eviction are not implemented; usage and manual cleanup are available.

## Validation

Frontend coverage checks defaults, range/enum/color repair, provider deduplication, and browser fallback. Rust coverage checks strict validation, normalization, persistence, shared-document synchronization, legacy defaults, module edits after a save, and unknown-field compatibility. Existing storage tests verify cache cleanup preserves library data.

Installed-platform acceptance remains manual: change each option, leave the screen immediately, restart the app, confirm restoration and audible/visible effects, preserve a restored queue, exercise provider priority, and clean the cache while retaining library, playlists, and original music. No cross-platform listening or installed-build acceptance is inferred from unit tests.
