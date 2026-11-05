# Automatic Updates (Phase 14)

Zenith Player uses the Tauri updater plugin with GitHub Releases as its
distribution source. The updater accepts only a newer semantic version whose
manifest channel matches the selected **Stable** or **Beta** channel. The
manifest target is selected by Tauri for the running operating system and
architecture; a package for another target is never installed.

## Release procedure

1. Keep the updater public key in `src-tauri/tauri.conf.json`. The matching
   private key must exist only in the CI secret `TAURI_SIGNING_PRIVATE_KEY`.
2. Build each target with `createUpdaterArtifacts` enabled. CI publishes the
   installer, its `.sig` file, and a channel-specific `latest.json` to a
   GitHub Release.
3. Include `channel`, `version`, `notes`, target metadata, artifact URLs, and
   signatures in each manifest. Stable and Beta manifests are separate feeds;
   never copy a Beta artifact into Stable.
4. Publish release notes with migration and rollback information. Data remains
   in the versioned local storage root and is not replaced by an update.

The updater verifies the signed manifest and artifact before installation.
HTTPS is required; unsigned, malformed, older, or incompatible packages are
rejected. The application checks on startup and exposes **Configuración →
Preferencias → General → Actualizaciones** for a manual check, channel
selection, release notes supplied by the manifest, download progress, and
installation. Windows restarts after installation; macOS and Linux relaunch
the application to complete it.

## Recovery and package formats

An interrupted download is discarded and can be retried from the manual
check. If installation fails, start the previous application version and
restore the most recent data backup from **Biblioteca y datos**. The update
does not delete `config/`, `library/`, `lyrics/`, or original music files.
Before a migration that changes a stored schema, create a backup and provide a
backward-compatible reader; never migrate in place without an atomic write.

AppImage and the Windows/macOS installers use the Tauri updater artifacts.
DEB and RPM updates are distributed through the selected package repository
and are not silently replaced by the in-app updater. Flatpak updates belong to
the Flatpak remote and are likewise handled by Flatpak. Each package format
must therefore document its own signature, rollback, and repository policy.

The public key currently identifies the release-signing key used by CI. A key
rotation requires publishing the new application version through the old key
first, then changing the pinned key and documenting the transition.
