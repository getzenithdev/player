# Minimal development interface

The interface provides a quiet workspace for implementing and checking the roadmap. It is a prototype, not evidence that the underlying music features are complete.

- Palette: canvas `#faf9fb`, surface `#ffffff`, text `#29252d`, secondary text `#706a76`, divider `#e7e3eb`, brand accent `#8b56a5`.
- Typography: Segoe UI on Windows, system equivalents elsewhere; 14px body text, 26px page titles, 600-weight headings.
- Layout: left navigation, a scrollable content area, and a persistent playback bar. The phase review screen pairs the numbered phase list with its acceptance checklist.
- Alignment: left-aligned content, short descriptions, and compact controls. Use whitespace and separators rather than decorative cards, shadows, or gradients.

```text
Navigation | Page title and phase shortcut
           | Content / honest empty state
           | Phase list | Manual review checklist
-----------+----------------------------------
Artwork    | Disabled playback controls | Queue
```

The brand icon is the only decorative element. Phase numbers represent the actual sequence, rather than decoration. On narrow windows, navigation becomes an icon rail and the phase list stacks above its details.

## Scope and review

Keep library, playlists, queue, lyrics, artwork, sources, equalizer, settings, and all 16 phase reviews reachable. Local playback from the phase 3 engine is available in the desktop application; the browser preview keeps the transport disabled. Local library imports and playlist/queue/history workflows are connected in the desktop application; provider connections, the remaining native integrations, and updates remain visibly pending. Navigation, equalizer preview, and manual checklist interactions work in memory only; they must not be presented as completed roadmap features.

The theme is persisted through the storage layer in the desktop application and restored on startup; in the browser preview it stays session-only and the control says so. **Configuración → Almacenamiento** reports the real storage root, permissions, document and backup counts, cache size, index counters, and recovery notices, and can create backups, export/import data without credentials, and clear the cache. Outside the desktop runtime those actions are disabled and explain why.

Manual review marks are session-only observations, not implementation status or automated test results. Avoid fabricated songs, providers, successful updates, and completion percentages. Verify navigation, keyboard use, small-window layouts, and the distinction between working previews and pending features.
