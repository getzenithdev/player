# Progressive external audio

External **Play** (`prepare`) now returns a stable `zenith-source:<provider>:<encoded-id>` reference. It remembers provider metadata without downloading the audio. Only explicit **Download** writes a completed file into `cache/audio/`. Local file playback continues through its existing decoder.

The native audio worker opens remote references through `network_audio.rs`. A separate thread resolves credentials and short-lived URLs, receives HTTP data and decodes it. The device callback consumes decoded samples; it never waits for HTTP or runs an extractor. The existing processing chain still applies equalization, balance, volume and limiting.

## Buffer and seeking

- HTTP reads request 64 KiB byte ranges. The compressed byte cache is limited to 8 MiB in memory, including container metadata read from the end of MP4/M4A resources.
- Decoding runs up to approximately 15 seconds ahead and retains up to approximately 5 seconds behind the current position, with additional sample-count caps keeping decoded data near a 48 MiB maximum (plus the active chunk). Audio is limited to eight channels and 192 kHz to bound decoded memory. Playback attaches after at least half a second is decoded, or earlier for a completed shorter resource.
- The lighter region in the existing seek bar represents decoded samples actually held in memory. It can move or shrink as old samples are evicted; it does not estimate time from a percentage of downloaded bytes.
- Seeking inside decoded audio repositions immediately. Seeking elsewhere asks the decoder to locate the timestamp and uses HTTP byte ranges for the required data. Duration and seek accuracy remain dependent on the container and its index.
- Servers that ignore Range can play sequentially when their container permits it. Random access to unreceived or evicted bytes fails explicitly; it does not silently wait for a full download. Some M4A resources need Range to read their trailing index before starting.
- When decoded audio runs out, the device receives silence while the media position remains stationary. A half-second refill threshold reduces repeated short interruptions. The player reports buffering separately from paused/playing state.
- Stop, next, replacement of the queue and output restart discard the previous session. Extractor cancellation terminates its process group. Blocking HTTP operations have bounded timeouts; cancellation is checked between reads and requests, and abandoned sessions cannot attach to the new track.

## Providers

Google Drive resolves `alt=media` with OAuth headers. WebDAV retains Basic authorization in Rust. Dropbox obtains a temporary content link. OneDrive obtains an authorized content URL. YouTube and YouTube API results use the packaged extractor to resolve a direct HTTPS AAC/M4A resource with `--skip-download`; playback does not invoke the extractor's file downloader. Live streams and segmented HLS/DASH sources are outside this implementation, and a video without an available direct AAC/M4A format returns an explicit error.

On HTTP 401/403 the resolver is called again to refresh provider credentials or signed links, with a bounded retry count. Transient connection failures and server errors also have bounded retries. Persistent failures become player errors; there is no fallback to downloading the entire track before playback. Resolved links and authentication headers stay in native memory, while playlists and session history persist only the stable provider reference and track metadata.

## Validation status

No automated tests or live playback tests were run for this change, at the user's request. Actual provider access, seek behavior, interruption/resume and the installed Monterey build remain for user validation. No credentials or third-party services were accessed during implementation.

Compilation completed successfully with TypeScript (`tsc --noEmit`), Vite's production build and `cargo check --locked --offline`. The build reports the existing large JavaScript chunk advisory and a future Rust compatibility advisory for the `block` dependency. A DMG was not generated as part of this change.
