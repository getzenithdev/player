# Native Integration and Accessibility

Phase 11 connects the player to the operating system and makes the interface
usable with assistive technology. The Rust side lives in
`src-tauri/src/native.rs`; the frontend preferences and announcements live in
`src/native.ts`, `src/shortcuts.ts`, and `src/accessibility.ts`.

## Architecture

Playback still has one owner: the audio worker in `src-tauri/src/audio.rs`. The
native layer never manipulates the output stream directly.

- Every published snapshot calls `native::sync_snapshot`, which updates the OS
  media controls and may show a track notification.
- Every tick calls `native::sync_progress`, which reports position changes to
  the media controls no more than once per second.
- Media keys, tray items, and the macOS application menu call the same
  `AudioEngine` methods used by the React interface.
- The worker compares the time between ticks. A gap of five seconds or more is
  treated as a system suspend/resume, and the output stream is rebuilt so
  devices reset by sleep recover without user action.

Nothing in the native layer blocks startup. A missing tray, a denied global
shortcut, or a notification failure is stored as a notice and shown in
**Configuración → Sistema** instead of failing the launch.

## Settings

Preferences persist in `config/shortcuts.json`, an existing document from the
storage layout. Every field is optional and defaults are applied on read, so an
older file remains valid. Saving normalizes impossible combinations (for
example, close-to-tray requires the tray) on both the Rust and TypeScript sides.

| Field | Default | Effect |
| --- | --- | --- |
| `mediaControls` | `true` | Windows SMTC, Linux MPRIS, or macOS Now Playing |
| `globalShortcuts` | `true` | Media play/pause, next, previous, and stop keys |
| `notifications` | `true` | Track notification when the window is not focused |
| `tray` | `true` | Tray/menu-bar icon with playback and window actions |
| `closeToTray` | `false` | Closing the window hides it and keeps playback alive |
| `autostart` | `false` | Launch at login |
| `reducedMotion` | `false` | Force the reduced-motion visual override |
| `highContrast` | `false` | Force the high-contrast visual override |

## Platform coverage

| Platform | Implemented | Documented limitation |
| --- | --- | --- |
| Windows | SMTC through `souvlaki`, native notifications, global media keys, tray icon, close-to-tray, launch at login, installer unchanged | Jump Lists and taskbar thumbnails are not implemented |
| macOS | Now Playing through `souvlaki`, application menu playback commands, menu-bar tray icon, notifications, global shortcuts, launch at login through a LaunchAgent | Media Remote and a custom Dock menu are not implemented |
| Linux | MPRIS through `souvlaki`, desktop notifications, global shortcuts, tray where the desktop environment supports it | Tray availability depends on the environment; no GNOME-specific extension is bundled |

Audio output keeps using the existing Rodio/CPAL path, which reaches
PipeWire/PulseAudio through the system output device.

## Accessibility

- A polite live region announces the current track, artist, album, and playback
  state without moving focus.
- Native notices and integration errors are also announced and remain visible
  in the settings screen.
- `Ctrl/Cmd + K` focuses search. Space toggles playback, the arrow keys seek by
  five seconds, `Ctrl/Cmd + Left/Right` change track, and `M` mutes. Text fields
  and focused buttons, links, summaries, selects, and editable regions keep
  their own key behavior.
- `:focus-visible` outlines are always present; high contrast thickens them, and
  forced-colors mode uses the system highlight color.
- The `reducedMotion` preference forces animations and transitions off even
  when the operating system does not request it. The existing
  `prefers-reduced-motion` media query remains the platform-level baseline.
- The `highContrast` preference strengthens text, borders, hover states, and
  accents for both dark and light themes.
- The layout keeps using relative units and the webview zoom, so system scaling
  is respected without a separate scale setting.

## Testing

Rust unit tests cover defaults, normalization, persistence across a restart,
missing documents, suspend detection, and duration conversion. Frontend tests
cover keyboard mapping, editing/activation guards, announcement text, and visual
override attributes.

Automated checks do not prove platform behavior. The following remain manual:

- Playback from Windows SMTC, Linux MPRIS, and macOS Now Playing/Control Center.
- Media keys with the window hidden and with another application focused.
- Notification permissions and appearance on each desktop environment.
- Tray behavior, close-to-tray, and quitting from the tray/menu.
- Launch at login after signing out and back in.
- Suspend/resume with a real device reset.
- Screen-reader reading order, high contrast, reduced motion, keyboard-only
  navigation, and system scaling.

The phase stays **En pruebas** until those checks are recorded on the target
platforms.
