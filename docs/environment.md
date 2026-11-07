# Environment Setup and Base Project

## Framework and Dependencies

- Node.js 22 LTS with pnpm 10.33.4 (22.12+; Node 24 is also accepted on newer systems); the supported major is recorded in `.nvmrc` and `package.json`.
- Tauri 2 with a Rust desktop backend and a React/TypeScript frontend.
- Vite for frontend development and builds.
- ESLint with TypeScript and React rules, Prettier, and Vitest.
- Rust with Cargo, rustfmt, and Clippy, configured through `rust-toolchain.toml`.
- Git for version control and GitHub Actions for CI.
- Rodio 0.23 with Symphonia 0.5.5 for local decoding and CPAL 0.18 for audio output, as documented in [audio-engine.md](audio-engine.md).
- The Tauri dialog plugin for selecting local audio files.

JavaScript dependencies are installed locally in the repository. The Tauri CLI is a development dependency, so no global CLI installation is required. Commit `pnpm-lock.yaml` and `src-tauri/Cargo.lock` to retain dependency resolution.

No database, server, or external provider dependency is required. The audio engine from phase 3 is local and works offline; library, lyrics, artwork, and provider integrations are selected in their roadmap phases.

## Native Prerequisites

Follow the [official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for the selected toolchain and operating system.

### Windows

- Install Rust through rustup, using the MSVC toolchain.
- Install Visual Studio Build Tools with the **Desktop development with C++** workload and a Windows SDK.
- Install the Microsoft Edge WebView2 Runtime if it is not already available.
- Restart the terminal after installation so Rust and Cargo are available on PATH.

### macOS

- Install Xcode Command Line Tools with `xcode-select --install`.
- Install Rust through rustup.
- The application configuration sets a macOS 12.0 deployment target. Compatibility with Monterey still requires testing on actual supported environments.
- Vite targets ES2020/Safari 15 syntax and Safari 15 CSS instead of its newer default browser targets. This does not provide polyfills for unavailable runtime APIs.

### Linux

Install Rust and the GTK/WebKit development packages required by Tauri. For Ubuntu/Debian-based environments, the starting set is:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf pkg-config libasound2-dev
```

CPAL uses ALSA on Linux, so the ALSA development headers (`libasound2-dev`) are required to compile the audio engine.

Package names differ across distributions; consult the official prerequisites for Fedora and other environments.

## Install and Run

```sh
npm install --global pnpm@10.33.4
pnpm install --frozen-lockfile
pnpm run tauri dev
```

The desktop command prepares pinned YouTube helpers automatically on first use; that preparation needs Internet access. On Monterey use Node 22: Node 24 prebuilt binaries require macOS 13.5+. See [youtube-provider.md](youtube-provider.md).

The npm command above is only for installing pnpm itself. `package.json` pins pnpm 10.33.4 for local development and CI; use that version if managing pnpm through Corepack or another version manager. The JavaScript implementation of pnpm 10 runs on the existing Node 22 environment, including Monterey.

When migrating an existing npm checkout, stop running development processes and remove its old `node_modules` directory before the pnpm installation. Do not recreate `package-lock.json`; `pnpm-lock.yaml` is the sole JavaScript lockfile.

Use `pnpm add <package>` (or `pnpm add -D <package>`) to add dependencies and `pnpm install` after intentionally editing dependency declarations. Commit the updated manifest and lockfile together. The development frontend uses port 1420 with a strict port check. `pnpm run dev` starts only the browser frontend.

## Quality Tools

| Command | Purpose |
| --- | --- |
| `pnpm run typecheck` | Check application and Vite configuration types. |
| `pnpm run lint` | Run ESLint and reject warnings. |
| `pnpm run format` | Format source and configuration files. |
| `pnpm run format:check` | Check formatting without writing files. |
| `pnpm test` | Run Vitest; an empty suite is allowed during scaffolding. |
| `pnpm run test:watch` | Run Vitest in watch mode. |
| `pnpm run check` | Run frontend types, lint, formatting, and tests. |
| `pnpm run rust:check` | Check the native Rust project. |
| `pnpm run rust:fmt` | Check Rust formatting. |
| `pnpm run rust:lint` | Run Clippy with warnings treated as errors. |
| `pnpm run rust:test` | Run Rust tests. |

Documentation is excluded from automatic formatting to preserve the existing specifications. Rust formatting is managed by rustfmt.

## Builds

- `pnpm run build`: TypeScript validation and production frontend assets in `dist/`.
- `pnpm run tauri build`: native application and local bundles under `src-tauri/target/release/`.
- `pnpm run tauri build --no-bundle --target <target>`: compile a specific native target without creating installers.

Cross-compilation requires the appropriate Rust target and platform toolchain; adding a Rust target alone does not supply all native dependencies.

| Platform | Rust target | Initial CI |
| --- | --- | --- |
| Windows x64 | `x86_64-pc-windows-msvc` | Configured. |
| macOS Apple Silicon | `aarch64-apple-darwin` | Configured. |
| macOS Intel | `x86_64-apple-darwin` | Configured. |
| Linux x64 | `x86_64-unknown-linux-gnu` | Configured on Ubuntu. |
| Windows ARM64 | `aarch64-pc-windows-msvc` | Pending final scope decision and validation. |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | Pending final scope decision and validation. |

The CI workflow runs on pushes, pull requests, and manual dispatch. It installs dependencies, checks frontend quality, runs Rust checks, and compiles the desktop targets. It does not publish releases, sign installers, or validate every target OS version.

Use a clean checkout with `pnpm install --frozen-lockfile` and the committed Cargo lockfile for repeatable dependency installation. A fully reproducible release process also requires pinning and validating native build environments and signing procedures.

## Windows Application Control

During initial local validation, `cargo fmt` returned Windows Application Control error `4551`. Direct `rustfmt` checks on the Rust source files passed, but this does not establish that the formatting script works on this machine. If the error persists, review the blocked executable with the system administrator. The project does not change Windows security policies.

## References

- [Official project generator](https://v2.tauri.app/start/create-project/)
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri GitHub pipeline guide](https://v2.tauri.app/distribute/pipelines/github/)
