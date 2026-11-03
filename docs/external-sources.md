# External sources / Fuentes externas (Phase 9)

## Contract

Every provider implements the same contract in `src-tauri/src/sources.rs`: a declared set of capabilities, a connection state, network operations behind an explicit opt-in, and credentials that live in the operating system credential store. The interface never offers an action the provider does not declare.

| Provider | Auth | Capabilities | Notes |
| --- | --- | --- | --- |
| Local | none | search, read, playback | Library and playback without network. |
| YouTube automático (primary) | none | search, read, playback, download | Local yt-dlp + Node 22, audio prepared as AAC/M4A cache files. |
| YouTube API (alternative) | API key | auth, search, read | Metadata only; existing credentials preserved. |
| Google Drive | OAuth 2.0 + PKCE | auth, search, read, playback, download, revoke | User-supplied `client_id`, scope `drive.readonly`. |
| Dropbox | OAuth 2.0 + PKCE | auth, search, read, playback, download, revoke | User-supplied `client_id`, scopes `files.metadata.read files.content.read account_info.read`. |
| OneDrive | OAuth 2.0 + PKCE | auth, search, read, playback, download | User-supplied `client_id`, scope `Files.Read offline_access`; disconnect locally and remove consent in the Microsoft account. |
| WebDAV | password | auth, search, read, playback, download | HTTPS endpoint, username and password; no revocation endpoint exists. |

Connection states are `disconnected`, `pending`, `connected`, `expired`, `revoked` and `error`. See [youtube-provider.md](youtube-provider.md) for extraction, packaging, limitations and Monterey checks.

One Tauri command, `sources_action`, receives an action enum tagged `kind` in camelCase (`snapshot`, `external`, `settings`, `connect`, `finish`, `disconnect`, `revoke`, `search`, `browse`, `download`) and always returns the full `Snapshot`: providers, results, external flag, pending authorization URL, last download path and warnings.

## Connections and secrets

`config/providers.json` is the same protected document the storage export already excludes. It stores connections (state, account label, endpoint, expiry, `has_secret`), the non-secret settings the user typed (`client_id`, `endpoint`, `username`) and the external opt-in. It never stores a credential: passwords, API keys, client secrets and tokens are serialized only into the credential store through `keyring::Entry::new("Zenith Player", provider)`. Tests use an in-memory `SecretStore`, so no platform keyring service is required to run them.

## Network rules

- Network actions are refused until **Permitir consultas a servicios externos** is enabled, matching the lyrics and artwork policy. The local provider keeps working with the switch off.
- HTTPS is required everywhere. Loopback HTTP is accepted only for the OAuth redirect (`127.0.0.1`, `localhost`, `[::1]`) and for local tests. Credentials embedded in a URL are rejected.
- The browser is opened without a shell, so authorization query parameters are never interpreted by a command interpreter.
- Limits: 4 MiB JSON and XML responses, 16 KiB `HEAD`/probe reads, 200-character queries, 200 results per answer, 5 000 browsed entries, 128 MiB per download, 20 s request timeout, 300 s download and authorization timeouts.
- Entry identifiers come back from the provider and are decoded and validated (`relative_id`, `safe_id`) before they are used in a URL or path, so traversal and absolute paths are rejected.
- Errors are reported in Spanish and never include a token, password or client secret.

## OAuth 2.0 with PKCE

`Connect` validates the supplied fields, binds a loopback listener (Google: `127.0.0.1:<ephemeral port>`; Dropbox: fixed `127.0.0.1:53682`; OneDrive: localhost URI with an ephemeral IPv4 port) and returns `pending_url`; the UI shows it and the application opens the default browser. The authorization request carries `state`, an S256 `code_challenge` and the provider's `redirect_uri`. `Finish` waits up to 300 s for `GET /callback`, rejects a callback with a different `state`, exchanges the code with the `code_verifier`, stores the tokens in the credential store and reads the account label best-effort. Only one authorization can be pending at a time; a new `Connect` cancels the previous listener.

Access tokens are refreshed on demand when a refresh token exists; a refused refresh marks the connection `expired` and the next query explains that the authorization must be repeated. `Revoke` calls the provider's revocation endpoint when it has one and always deletes the stored credential; `Disconnect` deletes the credential and resets the connection without touching other providers.

The pending authorization URL survives a panel reload or snapshot refresh in the same process. Disconnecting another provider preserves that authorization. Disabling external requests cancels it; canceled, timed-out, superseded or abandoned authorizations become retryable errors, including pending states left by a previous application process. Failed callbacks persist an error and the interface reloads the state so the user can reconnect. The loopback listener stops after its first terminal answer.

Dropbox revocation uses `POST /2/auth/token/revoke` with bearer authentication and no body; Google revocation prefers the refresh token. OneDrive does not advertise remote revocation: Graph's `revokeSignInSessions` invalidates sessions across applications, needs additional permissions, and does not support personal accounts. See the [Dropbox endpoint mapping](https://www.dropbox.com/developers/reference/migration-guide), [Dropbox scope guidance](https://dropbox.tech/developers/customizing-scopes-in-oauth-flow) and [Microsoft Graph revocation contract](https://learn.microsoft.com/en-us/graph/api/user-revokesigninsessions?view=graph-rest-1.0).

## Searching, browsing, and downloads

Search and browse return `Entry` values (`folder`, `audio`, `video`, `file`) with a relative identifier, a display name, a detail line, an optional size and an optional HTTPS source URL. Google Drive, Dropbox and OneDrive search their API and browse folders; WebDAV sends `PROPFIND` for the requested path and filters that listing by name for a search; the local provider searches the library index. Folders are opened only for providers that declare `read` and only from the interface's **Abrir carpeta** action.

Downloads are stored once under `cache/audio/` with a content-derived name and an extension taken from the file name or content type (`mp3`, `flac`, `wav`, `ogg`, `m4a`, `aac`, `aiff`, `caf`); a second download of the same entry reuses the cached file. Google Drive uses `alt=media`, Dropbox uses `files/download` with `Dropbox-API-Arg`, OneDrive asks Graph for `@microsoft.graph.downloadUrl`, and WebDAV downloads the validated URL with basic authentication. The returned absolute path is handed to the audio engine (**Reproducir** or **Reproducir descarga**), so playback always goes through files the engine already understands. The YouTube API alternative refuses downloads explicitly; YouTube automático prepares compatible AAC/M4A through its local helper, and the local provider returns the file path of the library track.

## Interface

**Fuentes** shows the opt-in switch, a card per provider with its capabilities, state, account label or endpoint, expiry, last error and the required connection fields (secrets are masked, the address is validated by the browser before it is sent). A pending authorization shows the URL and **Ya lo autoricé, continuar**. Searching picks a source, browsing opens folders, and each result offers the actions its provider declares. Warnings are listed instead of blocking the rest of the screen. The browser preview has no Tauri commands, so actions report the unavailable command and local library use stays in the other screens.

Saved non-secret connection fields prefill reconnect forms; successfully submitted secrets are cleared from the form state. Expired or failed connections can be disconnected to forget credentials. Folders offer browsing, files offer downloading, and only audio entries offer playback.

## Tests and acceptance

`src-tauri/src/sources_tests.rs` holds 15 tests that share one serial guard because the pending authorization is a single global slot: capability matrix, document/secret separation with restart persistence, external opt-in gating, WebDAV `PROPFIND` browse and search against a loopback mock server, single stored download, insecure endpoint and traversal rejection, OAuth round trip, state mismatch, refresh and expired handling, network loss with the local library still usable, provider-specific revocation, pending URL recovery, unrelated disconnect, cancellation/restart/timeout recovery, and helper rules (ISO 8601 duration, percent encoding, basic auth, query parsing). `src/sources.test.ts` covers field kinds and labels, connection/query/disconnect gating, folder/file/audio actions, state hints and size formatting.

Manual acceptance with real credentials (Drive, Dropbox, OneDrive, WebDAV, YouTube API key) on installed Windows/macOS/Linux builds is still pending, which is why the phase is tracked as "En pruebas" instead of completed.

Validation recorded on Windows x64, 2026-11-02: `npm run check` passed (28 frontend tests), `npm run build` passed, `cargo test --locked --manifest-path src-tauri/Cargo.toml` passed (97 Rust tests, including the 15 source tests), Clippy with `--all-targets -- -D warnings` passed, and rustfmt passed. Rust tests ran outside the sandbox because its socket restrictions reject the loopback mock servers; no real provider credentials were used. Clippy reported an environment warning about copying incremental-cache files instead of hard-linking them.

## Español

La fase 9 conecta servicios externos mediante un contrato común: cada proveedor declara sus capacidades y la interfaz solo ofrece las acciones que declara. Las contraseñas, claves y tokens viven en el almacén de credenciales del sistema; `config/providers.json` guarda únicamente conexiones y ajustes, y la exportación de datos nunca lo incluye. Las consultas externas siguen desactivadas hasta que las permites explícitamente.

YouTube automático es la opción principal, sin clave, con búsqueda y preparación de audio AAC/M4A mediante yt-dlp local. YouTube API queda como alternativa independiente de búsqueda y metadatos con tu propia clave, sin reproducción ni descarga. Google Drive, Dropbox y OneDrive usan OAuth 2.0 con PKCE y tu propio ID de cliente, con autorización en el navegador y caducidad gestionada; WebDAV exige HTTPS con usuario y contraseña. Las descargas se guardan en la caché local y se reproducen con el motor de audio, de modo que la biblioteca local sigue funcionando con la red caída o las autorizaciones caducadas. Falta la aceptación manual con credenciales reales en las aplicaciones instaladas.

## Setup guides

Every provider card includes ordered setup instructions and an official-site button. Drive uses a Desktop OAuth client and drive.readonly; Dropbox uses PKCE, a fixed registered return URI and read-only scopes; OneDrive uses a public desktop client and localhost return URI. WebDAV uses the server HTTPS endpoint and preferably an app password. See `src/providerGuides.ts` for the exact UI instructions. No credentials need to be committed or sent to the application distributor.
