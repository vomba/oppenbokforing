# Packaged macOS verification — 2026-11-04

## Status and scope

**Package build passed. The first authorized native UI run found a blocking F-skatt onboarding defect.**
Launch and disposable Rust-data creation were observed. Invoice review, browser launch and invoice reopening were not reached in this first run; no success for those paths or GUI installation is claimed. This is an unsigned local development smoke build, not a signed/notarized release or a claim of complete accounting support. Cash-method implementation remains dependent on the runtime check.

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

## First native run — observed blocker

The packaged executable was launched as a managed foreground process with disposable
`HOME=/tmp/oppenbokforing-native-uat.kv06Kn`. Native Accessibility inspection was
restricted to that process, verified by bundle identifier and executable path;
no screenshots or unrelated application contents were captured. Rust created its
recent-workspace index and SQLite database beneath that disposable home, not the
normal application-data directory. Isolation of every macOS webview/preference
cache is not claimed.

A synthetic workspace reached onboarding. Real keyboard events entered
`UAT Verification Firma` / `UAT Test Owner`; native menu actions selected registered
VAT, annual reporting, invoice method and the May-12 regime. The review showed
F-skatt with zero salary and SEK 10,000 expected profit. Saving persisted those
profiles but stayed on Review with “FA-skatt kräver separat planering för lön och
firma” and “Skatteplanering behöver din förväntade lön.” Read-only inspection of the
disposable database confirmed `f_skatt`, salary `0`, and
`registered/yearly/invoice_method/annual_may_12`.

The native Accessibility transcript is retained in
`artifacts/f-skatt-onboarding-native-failure-2026-11-04.txt`. Initial direct AX value
assignment did not propagate React-controlled input state, so subsequent text entry
used normal keyboard events; this automation limitation is not reported as an app
bug. The application was gracefully terminated after the profiles were saved.

Root cause was verified in the profile compliance selector: registered F-skatt
falls back to the FA-skatt salary scenario, whose FA-specific predicates cannot
pass for an ordinary F-skatt owner. The UI correctly refuses navigation when that
backend result is false. This is a blocking profile-guidance classification bug,
not a reason to invent salary, change tax status, skip the check or authorize cash
posting. A test-first correction and rebuilt-package rerun are required.

## Remaining runtime procedure

Use only disposable test data. Rust app-data resolution in the installed Tauri/dirs sources respects an explicit `HOME`; verify actual paths during launch rather than assume all macOS webview/preferences storage is isolated. Do not open real working books or overwrite an installed application.

Pending observations:

1. Launch the identified packaged application with disposable application/workspace data.
2. Create a synthetic eligible 2026 annual/no-EU invoice-method workspace and invoice.
3. Verify the final review shows the fingerprint-bound current customer, lines and amounts; changed inputs must require a fresh review, not silent posting.
4. Click a compiled source and observe the actual system-browser action without inspecting unrelated browser tabs.
5. Issue the test invoice, reopen the workspace and inspect retained provenance/evidence.

Until actual observations are recorded, all five runtime outcomes above remain **not verified**. Do not infer them from the successful build, mocked UI tests or injected native-launch tests.
