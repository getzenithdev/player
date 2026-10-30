# Audio Engine (Phase 3)

The audio engine provides offline playback of local files. It is implemented in `src-tauri/src/audio.rs` and exposed to the interface through Tauri commands and events. The interface can open audio files, control playback, and inspect the current state; the music library, metadata, persistent queue, and audio processing remain later phases.

## Selection

| Candidate | Responsibility | Decision |
| --- | --- | --- |
| Rodio 0.23 | Playback facade with Symphonia decoding and CPAL output | Selected |
| Symphonia 0.5.5 | Decoding and format probing | Used through Rodio |
| CPAL 0.18 | Operating system audio output | Used through Rodio |
| Kira | Game-oriented mixing and effects | Not selected: it adds a second mixer model before phase 12 defines the processing requirements |

Rodio is MIT/Apache-2.0 licensed and keeps decoding and output replaceable. The default output stream is opened on a dedicated worker thread because CPAL streams are not `Send` on every platform. The worker owns the stream, the player, and the in-memory queue; commands never block the interface while a file is decoded or the device is opened.

## Architecture

```text
React interface
  │  invoke(audio_*)
  ▼
AudioEngine (Tauri state)
  │  command channel
  ▼
zenith-audio worker thread
  ├── Rodio Player + CPAL output
  ├── in-memory queue and current index
  └── playback snapshot
        │  audio:state / audio:position
        ▼
React interface
```

- `AudioEngine` keeps a command sender and an `Arc<Mutex<PlaybackSnapshot>>`.
- The worker opens the default output device at startup. If that fails, the application still starts and the snapshot reports an error.
- Every track is decoded through Rodio with byte length and seek support. The decoder exposes duration, sample rate, and channel count before playback.
- The worker polls the player position every 200 ms, detects the end of a track, applies repeat/shuffle, and emits position events while playing.
- A stalled output (for example after system sleep or a device change) is detected when the position does not advance for about four seconds. The worker recreates the output and resumes the current track at the last position, up to two consecutive attempts. The interface also exposes a manual **Reiniciar salida** action.

## Controls and State

Commands: `audio_get_state`, `audio_open_files`, `audio_play_index`, `audio_play`, `audio_pause`, `audio_toggle`, `audio_stop`, `audio_next`, `audio_previous`, `audio_seek`, `audio_set_volume`, `audio_set_muted`, `audio_set_repeat`, `audio_set_shuffle`, `audio_restart_output`, `audio_clear_queue`, and `audio_capabilities`.

Events:

- `audio:state` sends the complete playback snapshot after every state change.
- `audio:position` sends `{ positionMs, durationMs, status }` while playing.

The snapshot exposes the status (`idle`, `loading`, `playing`, `paused`, `stopped`, `error`), current track, position, duration, sample rate, channels, format, volume, mute, repeat, shuffle, queue, current index, error message, and the output configuration. Track titles currently come from the file name; tags, artwork, and stable metadata identities belong to phases 4 and 6.

## Format Matrix

The decoder features are enabled in `src-tauri/Cargo.toml`. "Enabled" means the build can decode the format; it does not replace playback verification with real reference files.

| Format | Decoder | Playback verified with reference files |
| --- | --- | --- |
| MP3 | Symphonia MP3 | Pending |
| FLAC | Symphonia FLAC | Pending |
| WAV | Symphonia WAV/PCM | Pending |
| OGG Vorbis | Symphonia OGG/Vorbis | Pending |
| AIFF | Symphonia AIFF/PCM | Pending |
| AAC in M4A/MP4 | Symphonia ISO MP4/AAC | Pending |
| ALAC in M4A | Symphonia ISO MP4/ALAC | Pending |
| Opus | No decoder in the selected Symphonia version | Not supported |
| M4A | Container only; the internal codec decides playback | AAC/ALAC enabled, other codecs rejected by the decoder |

Opus needs a separate decoder decision before it can be listed as supported. M4A files must be checked by codec rather than by extension. AAC and ALAC support depends on the decoder build and still requires the manual verification listed below.

## Manual Verification

1. Run `npm run tauri dev` and open files from **Cola → Abrir archivos**.
2. Play, pause, resume, stop, change tracks, seek, and adjust volume and mute.
3. Check repeat off/all/one and shuffle with a queue of at least three tracks.
4. Play short and long files at different sample rates and bit depths, including high-resolution audio, files with ID3 tags, embedded artwork, and damaged metadata.
5. Leave a track playing, suspend the computer, and verify that playback resumes or use **Reiniciar salida**.
6. Change the output device while playing and verify recovery.
7. Open an unsupported or corrupt file and verify that the interface reports an error without blocking the rest of the application.

The generated WAV unit test in `audio.rs` checks decoding metadata and duration without opening an audio device, so it is safe for headless CI.

## Known Limitations

- Opus is not supported by the selected decoder.
- Gapless playback, crossfade, balance, speed, ReplayGain, normalization, equalization, and output-device selection are prepared but implemented in phase 12.
- The queue is temporary. Persistence, playlist interchange, and session restoration belong to phase 5.
- Tags, embedded artwork, and library identity belong to phases 4 and 6.
- Native media keys, system media controls, and notifications belong to phase 11.
- The output watchdog is a recovery heuristic, not a platform device-change notification.
