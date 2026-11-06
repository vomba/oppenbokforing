# Packaging and Release Plan

## Release Order

1. macOS development builds — **current default** (`npm run tauri:dev`, unsigned `npm run tauri build`).
2. macOS signed and notarized beta — **deferred** (optional; not worth paid Apple membership until wide distribution is needed). See `docs/apple-signing-setup.md`.
3. Windows signed beta.
4. Linux AppImage or deb package.

## Packaging Stack

- Tauri bundler for platform packages.
- Tauri updater for later beta updates.
- Platform code signing before **wide public** distribution (optional for personal/local use).
- Local migration smoke tests before every release.

## Required PDF helper artifact

Cash receipt inspection uses a separately exec'd `oppenbokforing-pdf-helper`.
It adds no library dependency or renderer shell permission. The Tauri dev/build
hooks run `npm run pdf:build` before the frontend command. Direct desktop Cargo
checks on a fresh checkout must stage it first:

```sh
npm run pdf:build
cargo check --manifest-path src-tauri/Cargo.toml --features desktop
```

The build script uses the hook's target/profile, builds the existing Rust helper
binary, and stages the target-suffixed file under `src-tauri/binaries/` (generated,
not committed). The bundler strips the target suffix. On macOS, the required runtime
location is `Contents/MacOS/oppenbokforing-pdf-helper`, beside the main executable.
Missing or non-executable helper blocks new cash recognition; it does not authorize
an unbounded fallback or block historical completed recovery.

Native builds use the host architecture. The existing universal macOS release path
builds both installed Apple targets and verifies their combined helper with
`/usr/bin/lipo`; both per-architecture staged files are also retained for Tauri's
Cargo builds. The script never installs targets/tools or substitutes the host for
another requested architecture. Local native arm64 staging/resource probes were
executed; Intel/universal execution, remote CI and signed/notarized helper operation
require their own verification. Pure Node tests of the build plan are included in
`ci:public`; they are not a cross-build execution result.

Resource and privacy guarantees live in `security-privacy`. Packaging checks must
inspect the actual copied app/DMG for the helper and exercise its installed fixed
path. A successful standalone Cargo test or a file in `target/` is not proof that
the bundle contains it. Signing and notarization, when enabled, must include the
helper; unsigned local builds remain explicitly unsigned.

## Release Artifacts

- macOS `.dmg` or `.app`.
- Windows `.msi` or `.exe`.
- Linux AppImage or `.deb`.
- Checksums for every artifact.
- Release notes.
- Migration compatibility notes.

## macOS Plan

- Configure app identifier `se.oppenbokforing.desktop`.
- Add app icon before first signed build.
- Set hardened runtime.
- Sign app bundle.
- Notarize release build.
- Verify first launch, workspace create, backup, restore, and export.

## Windows Plan

- Configure signed installer.
- Verify app data path.
- Verify file dialogs.
- Verify backup and restore with paths containing spaces and Swedish characters.
- Verify migration after app update.

## Linux Plan

- Package AppImage first unless distribution-specific packages are needed.
- Verify app data path.
- Verify file dialogs.
- Verify desktop launcher metadata.

## Update Strategy

- No auto-update in the first internal build.
- Add updater after migration and backup checks are stable.
- Tauri updater plugin is configured in `src-tauri/tauri.conf.json` with `"active": false` by default.
- Enable beta updates only via release CI env (`TAURI_UPDATER_ENABLED=1`, endpoint, pubkey).
- Before applying an update, prompt for backup if a schema migration is pending.
- Preserve compatibility with the previous release's backup format.

## Signing and Notarization (macOS beta)

- CI workflow: `.github/workflows/release-macos.yml` (tag `v*` or manual dispatch).
- PR CI: `.github/workflows/ci.yml` (`test:all` + integration smoke).
- Signing setup: `docs/apple-signing-setup.md`.
- Set `bundle.macOS.signingIdentity` via `APPLE_SIGNING_IDENTITY` secret in CI.
- Notarize with `APPLE_API_KEY` / `APPLE_API_ISSUER` / `APPLE_API_KEY_BASE64` secrets (not committed).
- Hardened runtime is enabled in `tauri.conf.json`.
- Local unsigned dev builds remain the default developer path (`TAURI_SMOKE=1 npm run test:smoke`).

## Release Gates

- `npm run build` passes.
- Rust tests pass.
- SQLite migration tests pass.
- Golden scenario tests pass.
- Packaged app smoke test passes, including helper presence and normal/resource-failure receipt paths from the installed bundle.
- `npm run test:smoke` passes (integration mode; set `TAURI_SMOKE=1` for package build).
- Backup and restore smoke test passes.
- Security/privacy release checklist passes.

## Rollback Strategy

- Keep previous installer available during beta.
- Backups must be restorable by the same or newer app version.
- Database migrations are forward-only, so rollback uses backup restore rather than schema downgrade.

