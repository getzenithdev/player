# Equalizer and Audio Processing (Phase 12)

Phase 12 adds a ten-band equalizer and playback processing to the existing Rodio engine. The DSP lives in `src-tauri/src/audio_processing.rs`; the interface is `src/EqualizerPanel.tsx` with the contract and preset table in `src/equalizer.ts`.

Processing runs inside a `rodio::Source` wrapper on the audio worker thread. The interface never decodes or processes samples, so parameter changes do not block rendering and never restart the track.

## Signal chain

```text
decoder
  → 10 peaking biquads per channel
  → static gain (preamp + ReplayGain, capped by normalization)
  → automatic headroom (clip guard)
  → channel balance
  → bounded fade-in / fade-out
  → soft limiter (clip guard)
  → output
```

When **Activado** is off, the wrapper returns the decoded sample unchanged. Crossfade is part of the same switch, so disabling processing also disables fades.

- Each band is an RBJ peaking filter with Q 1.414 (one-octave spacing), recalculated when the sample rate or settings change.
- Filter state is reset on seek and when processing is re-enabled, avoiding a burst from stale state.
- The source reads a revision counter between samples. A settings update replaces coefficients in place; playback position, queue, gapless behavior, and output stream stay untouched.
- The chain runs inside the source, before the player's volume and output stages, so it composes with the existing volume control and output selection instead of replacing them.

## Bands

| # | Frequency | Band | Notes |
| --- | --- | --- | --- |
| 1 | 31 Hz | 31 | |
| 2 | 62 Hz | 62 | |
| 3 | 125 Hz | 125 | |
| 4 | 250 Hz | 250 | |
| 5 | 500 Hz | 500 | |
| 6 | 1 kHz | 1k | |
| 7 | 2 kHz | 2k | |
| 8 | 4 kHz | 4k | |
| 9 | 8 kHz | 8k | Disabled at sample rates at or below 16 kHz |
| 10 | 16 kHz | 16k | Disabled at sample rates at or below 32 kHz |

A band is bypassed when its center frequency is not below `0.45 × sample rate` (Nyquist margin). At 44.1 and 48 kHz all ten bands are active; the interface marks unsupported bands and explains why. Gains and preamp are clamped to ±12 dB.

## Presets

Built-in presets mirror the table in `src-tauri/src/audio_processing.rs` and `src/equalizer.ts`:

| Id | Label |
| --- | --- |
| `flat` | Plano |
| `rock` | Rock |
| `pop` | Pop |
| `classical` | Clásica |
| `jazz` | Jazz |
| `electronic` | Electrónica |
| `hip-hop` | Hip-Hop |
| `bass-boost` | Graves reforzados |
| `vocal` | Voz |
| `acoustic` | Acústica |

Editing any band marks the curve as `custom` until it matches a preset again. **Guardar curva actual** stores a named custom preset (up to 40, unique names); saving the same name replaces it. Deleting the active custom preset leaves the current curve untouched and changes the preset identity to `custom`.

## ReplayGain and normalization

`read_track_gain` reads `REPLAYGAIN_TRACK_GAIN`, `REPLAYGAIN_TRACK_PEAK`, `REPLAYGAIN_ALBUM_GAIN`, and `REPLAYGAIN_ALBUM_PEAK` through Lofty when a track is loaded. The parsers accept values such as `-7.25 dB`, `+2,5 dB`, and `0.987654`. The decoded file is never modified.

- **Por pista** applies the track gain (falling back to album gain).
- **Por álbum** applies the album gain (falling back to track gain).
- **Desactivado** applies no ReplayGain and makes normalization inert.
- Normalization caps the static gain so `gain × peak` does not exceed 0.891 (about −1 dBFS). It needs a ReplayGain peak tag; it is not a loudness scanner.

## Headroom and clipping protection

With **Protección de dinámica** enabled:

1. Automatic headroom is `-max(0, preamp + ReplayGain + highest supported boost)`. A 12 dB boost with 6 dB of preamp therefore receives −18 dB of headroom.
2. The static gain includes preamp, ReplayGain, normalization, and headroom.
3. Samples above 0.95 enter a `tanh` soft limiter and are clamped to ±1.0. The diagnostics panel reports **Limitador: Actuando** when it engages.

Disabling the guard removes both headroom and limiting, so the user is responsible for avoiding saturation. Balance only attenuates one channel; it never boosts the other.

## Crossfade

The fade length is bounded by half of the track duration and by `MAX_CROSSFADE_MS` (12 s). It is an equal-power fade-in at the start and fade-out at the end of each source.

This implementation does **not** overlap two decoders. It keeps the queue and gapless/session behavior deterministic, but the transition between tracks is a short silence-free edge fade rather than a true overlapping crossfade. A future gapless engine can replace the edge fade without changing the persisted setting.

## Persistence

Settings and custom presets are stored in the existing `config/audio.json` document through `StorageManager` (atomic writes, recovery, and export rules already apply). The document uses camelCase keys:

```json
{
  "enabled": true,
  "preset": "rock",
  "gains": [5, 4, 3, 1, -1, -2, -1, 1, 3, 4],
  "preampDb": 0,
  "balance": 0,
  "replayGain": "track",
  "normalization": false,
  "crossfadeMs": 0,
  "clipGuard": true,
  "customPresets": [
    { "name": "Sala", "gains": [3, 2, 1, 0, 0, 0, 0, 1, 2, 3], "preampDb": -2 }
  ]
}
```

The frontend debounces live changes by 280 ms and flushes the latest curve when leaving the screen. Values are normalized on both sides: finite numbers, clamped ranges, trimmed names, unique presets, and a 60-character name limit.

## Interface

**Ecualizador** is available from the main navigation and from **Configuración → Audio**. The screen contains:

- master enable switch with a live status message;
- preset selector, custom presets, and **Restablecer plano**;
- ten vertical band sliders with dB readouts and unsupported-band markers;
- preamp, balance, ReplayGain mode, normalization, crossfade, and clip guard;
- read-only diagnostics: sample rate, active bands, effective gain, headroom, limiter state, and applied ReplayGain.

All controls have labels and value text for assistive technology. The processing toggle disables the whole fieldset in the browser preview, where native commands are unavailable.

## Verification

Automated coverage in `audio_processing.rs`:

- preset completeness and gain ranges;
- clamping, NaN/Infinity handling, duplicate custom-preset removal, and 40-preset limit;
- custom preset save, replace, apply, and remove;
- unknown preset and blank-name rejection;
- sample-rate band support at 44.1, 32, and 16 kHz;
- headroom calculation for maximum boost plus preamp;
- ReplayGain normalization cap;
- peaking-filter magnitude at its center frequency (about +6 dB) and no boost far away;
- soft limiter bounded below full scale;
- fade length for a short track;
- storage round trip with custom presets;
- ReplayGain parser compatibility.

Run the manual throughput measurement in a release build:

```bash
cargo test --release --manifest-path src-tauri/Cargo.toml \
  audio_processing::tests::processing_throughput_reports_real_time_factor \
  -- --ignored --nocapture
```

The test processes five seconds of a mono 440 Hz sine wave through the full ten-band chain and fails below 5× real time. Record the printed result for the target machine.

Manual acceptance still requires playing reference signals and music on each target platform: confirm each band's audible effect, verify no clipping with boosted presets and loud material, change every control during playback, restart the application to confirm restoration, and measure CPU while playing a high-resolution file.
