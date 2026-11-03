# YouTube provider / Proveedor YouTube

## Implementation

`youtube-auto` is the primary YouTube source. Rust launches a pinned local yt-dlp
executable with structured arguments, without a shell, user configuration,
plugins, browser cookies or remote challenge-script downloads. The packaged
Node 22 runtime executes the EJS component included in the official yt-dlp
standalone executable. No API key, Google account or self-hosted server is
required. External requests still require explicit user opt-in.

`youtube` remains the independent **YouTube API (alternativa)** provider. Its
existing ID, credentials and metadata-only capabilities are preserved. The UI
lets a user retry a failed automatic search with the configured API; this is
not an audio fallback. API results remain video entries without playback or
download actions.

The first selectable search target is YouTube automático. With external access
disabled the action remains disabled; local search is still selectable.

### Search and playback

- Flat searches return up to 25 video IDs, titles and channels without resolving
  every result's media formats. Live results are excluded when marked as live.
- Playback downloads a compatible AAC/M4A audio stream to a unique temporary
  directory inside `cache/audio/`, validates a decoded sample with the same decoder used for playback, then promotes the completed file to
  `youtube-<video-id>.m4a`. The existing Rodio/Symphonia MP4 decoder handles it.
- Playback starts **after preparation**, not while downloading. There is no
  progressive streaming, automatic YouTube queue prefetch or audio transcoding
  in this implementation. FFmpeg and Opus decoding are not required.
- Valid cached files are reused without contacting YouTube. Temporary files are
  removed on failure and cancellation; a completed cache file survives restart.
- Only exact 11-character video IDs are accepted, not arbitrary URLs or paths.
  Files are limited to 128 MiB; stdout to 4 MiB and stderr to 64 KiB. Deadlines
  are 60 seconds for search and 300 seconds for download. Only one extraction
  operation runs at a time. Cancel terminates the helper process tree and drains
  its pipes; disabling external queries also cancels it.
- Failures are sanitized rather than displaying signed media URLs or raw
  diagnostics. Local providers and their credentials remain independent.
- Videos without a compatible audio stream, restricted/private/age-gated
  videos, unavailable videos and live broadcasts can fail. There is no account
  cookie import, PO-token service or guarantee of uninterrupted catalog access.

### Packaged components and updates

`npm run youtube:prepare` downloads the exact versions and SHA-256 digests in
`scripts/youtube-tools.json`. It extracts only the Node executable and license,
then prepares `src-tauri/tools/`. Both Tauri development and desktop builds run
this preparation automatically. Tauri ships the folder as resources and Rust
resolves it using the application's resource directory, including macOS Finder
launches with a restricted PATH. End users do not install Node or Python.
The frontend-only `npm run build` remains independent of the helper download.

The initial preparation needs Internet access. Later preparations reuse the
verified components. Generated binaries are ignored by Git. To update, review
an official yt-dlp release and Node 22 release, replace pinned versions/hashes,
refresh the yt-dlp license notices from that release and run desktop CI. There
is no runtime self-update or unsigned remote code download. Do not replace the
bundled runtime with Node 24 while Monterey is a target.

The app's own source license does not replace licenses of these separate
executables. Node's license and yt-dlp's own and third-party notices accompany
the binaries. Upstream sources for the pinned helpers:

- https://github.com/yt-dlp/yt-dlp/tree/2026.10.26
- https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.26
- https://github.com/nodejs/node/tree/v22.23.4
- https://nodejs.org/dist/v22.23.4/

### Monterey compatibility

The app's minimum remains macOS 12.0, with `MACOSX_DEPLOYMENT_TARGET=12.0` in
CI and Safari 15/ES2020 frontend targets. Development accepts Node 22.12+ or
24, while `.nvmrc` selects 22 so Monterey developers can install dependencies.
Node 24 prebuilt executables require macOS 13.5; they are not used as helpers.
The pinned Node 22 branch declares macOS 11.0+ and the official universal
`yt-dlp_macos` executable declares macOS 10.15+.

Desktop CI launches the packaged helpers on macOS Intel and Apple Silicon, then
checks the Mach-O deployment metadata of every helper slice and the compiled
application with `scripts/check-macos-minimum.py`. A minimum newer than 12.0 or a
missing target architecture fails the check. This is a static compatibility
check, **not a substitute for running the app on Monterey**. CI's macOS 15
runners do not prove macOS 12 runtime compatibility.

Manual acceptance on macOS 12 Intel and Apple Silicon still requires:

1. Install the built app and launch it from Finder, without relying on PATH.
2. Enable external queries, search without a key and prepare/play authorized
   test content; pause, resume and seek.
3. Cancel search/download, test loss of network and confirm temporary cleanup.
4. Replay cached audio and local library content offline.
5. Configure the API alternative and verify metadata search without audio
   controls; connect the other providers using the built-in guides.

### Source permissions

An unofficial extractor does not grant permission to download or separate audio.
Use only content and access for which the necessary permissions exist. The API
alternative must also follow YouTube's developer policies. This adapter makes
no claim of approval by YouTube or unrestricted distribution rights.

## Español

YouTube automático es la opción principal, sin API key. Rust coordina yt-dlp y
Node 22 incluidos con la aplicación. La búsqueda es ligera y el audio AAC/M4A se
prepara en caché antes de reproducir. La API de YouTube queda como alternativa
independiente para búsqueda y metadatos, conservando las claves ya guardadas.
Las guías paso a paso están dentro de **Fuentes → Proveedores**.

La configuración y los ejecutables se eligen para macOS Monterey, pero la
aceptación real en macOS 12 sigue pendiente hasta ejecutar los flujos en ese
sistema. Las comprobaciones de compilación y de versión mínima declarada no
se presentan como una prueba de ejecución en Monterey.

## Validation record (2026-11-03)

- Verified pinned helper checksums and launched yt-dlp 2026.10.26 and Node
  v22.23.4 on Linux.
- Live keyless search returned results. This environment uses a TLS inspection
  proxy; the diagnostic invocation used its trusted system CA, with certificate
  verification enabled. Production does not disable TLS verification.
- A live test download reported success but the media response was a 195-byte
  HTML page saying "Site Unavailable", not audio; it failed independent ffprobe
  validation. The adapter rejects this response rather than caching it as music.
  Decoder validation additionally prevents accepting truncated containers.
  End-to-end live audio playback cannot be verified through this environment's
  unavailable media endpoint and is not claimed as verified here.
- Verified the downloaded macOS helper checksums and Mach-O minimum versions:
  yt-dlp x86_64 10.13 / arm64 11.0; Node x86_64 11.0. Upstream supports the
  standalone yt-dlp package on 10.15+, which is the conservative package minimum.
- A generated AAC/M4A sine-wave fixture verifies decoding and offline cache
  reuse without a helper or output device. Other adapter tests exercise malformed
  results, ID validation, deadlines, pipe limits, cancellation and secret-safe
  errors. Desktop CI also checks both macOS architectures.

Local checks: 108 Rust tests passed, Clippy with warnings denied and rustfmt
passed; 34 frontend tests, type checking, ESLint, Prettier and the Vite build
passed. Frontend checks/build were also run with the packaged Node v22.23.4.
The AAC fixture is an 80 ms, 440 Hz sine wave generated with FFmpeg, stored as
base64 so its provenance and test use are explicit; it contains no third-party
recording.
