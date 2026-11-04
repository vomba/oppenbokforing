# Packaged macOS verification — 2026-11-04

## Status and scope

**Package build passed. Accessibility is now authorized; native UI verification has not yet run.**
No packaged invoice workflow, browser launch, reopening or GUI install outcome is claimed here yet. This is an unsigned local development smoke build, not a signed/notarized release or a claim of complete accounting support. Cash-method implementation remains dependent on the runtime check.

Code revision: `fe60b628a63ef72996238730be2884d8ba245e61` on `cash-method-safety`.
Artifact inspection: 2026-11-04T13:39:56Z. Platform: macOS arm64.

## Build command and observed result

Signing/notarization credential variables were removed from this child process only. No keychain, signing configuration or OS security settings were changed.

```sh
env -u APPLE_SIGNING_IDENTITY -u APPLE_ID -u APPLE_PASSWORD \
  -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH \
  -u APPLE_TEAM_ID -u APPLE_API_KEY_BASE64 \
  PATH="$HOME/.cargo/bin:$PATH" CI=true TAURI_SMOKE=1 \
  TAURI_BUILD_ARGS="--ci --no-sign --bundles app,dmg" npm run test:smoke
```

Result: **exit 0**. The script ran its fixture/i18n/M6/golden integration checks, generated development icons, built the production frontend and optimized desktop binary, and produced both `.app` and `.dmg`. It explicitly reported skipping signing because of `--no-sign`. This invocation is not the same as the earlier integration-only smoke and is not a substitute for the remaining UI checks.

The repository's icon generator temporarily changed four tracked PNGs. After bundling completed, only those build-generated changes were restored to their initially clean state. No icon design or source logic change is part of this verification. The artifact is reproducible through the command above, which includes that development-icon generation step.

## Artifact identity

- App: `src-tauri/target/release/bundle/macos/ÖppenBokföring.app`
- Installer: `src-tauri/target/release/bundle/dmg/ÖppenBokföring_0.1.0_aarch64.dmg`
- Bundle identifier from `Info.plist`: `se.oppenbokforing.desktop`.
- Version from `Info.plist`: `0.1.0`.
- App executable: `Contents/MacOS/oppenbokforing-desktop`; `file` identified it as a Mach-O 64-bit arm64 executable.
- SHA-256 of that executable (not a directory hash): `4a008e600da5532ce4eb79836c2bd362300b811d2118f929f37cffaf53472ee0`.
- SHA-256 of the complete DMG: `50f58b5d3a0336b525c6e8e1c4b79d7893d4cb8e78d454123ac0178f588f147a`.

The local build artifacts are not committed. Their paths and hashes identify the artifacts to exercise rather than an arbitrary installed copy.

## Observed warnings and limits

The release compile emitted four unused-variable warnings: `commands.rs`'s best-effort job error and three error/redaction bindings in `error.rs` that are used only in debug logging. They did not fail the build, were not suppressed, and are recorded as maintenance debt rather than silently fixed in this runtime-check task. Existing strict Clippy debt is separate from this package-build result.

Initial AppleScript and `AXIsProcessTrusted()` checks reported false. After the user selected UI automation, the normal Accessibility permission prompt was requested; its immediate result remained false. A subsequent `AXIsProcessTrusted()` check returned **true**. This requested normal OS consent; it did not grant or bypass permission programmatically. The runner ancestry is the ThinkRail application (`bun`/`launcher` inside `ThinkRail.app`). UI automation is authorized, but its results are still pending.

## Pending runtime procedure

Use only disposable test data. Rust app-data resolution in the installed Tauri/dirs sources respects an explicit `HOME`; verify actual paths during launch rather than assume all macOS webview/preferences storage is isolated. Do not open real working books or overwrite an installed application.

Pending observations:

1. Launch the identified packaged application with disposable application/workspace data.
2. Create a synthetic eligible 2026 annual/no-EU invoice-method workspace and invoice.
3. Verify the final review shows the fingerprint-bound current customer, lines and amounts; changed inputs must require a fresh review, not silent posting.
4. Click a compiled source and observe the actual system-browser action without inspecting unrelated browser tabs.
5. Issue the test invoice, reopen the workspace and inspect retained provenance/evidence.

Until actual observations are recorded, all five runtime outcomes above remain **not verified**. Do not infer them from the successful build, mocked UI tests or injected native-launch tests.
