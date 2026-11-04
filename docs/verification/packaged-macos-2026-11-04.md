# Packaged macOS verification — 2026-11-04

## Status and scope

**The bounded native workflow passed on the corrected unsigned package.**
The first artifact exposed a real F-skatt onboarding blocker; that failure and its
separate corrected rerun are retained below. The corrected run verified setup,
missing/stale review rejection, current financial review, one actual source-browser
action, issuance, PDF retention and cold reopen. This is not a signed/notarized
release, Finder/Gatekeeper admission test, visual PDF review or complete app UAT. This is an unsigned local development smoke build, not a signed/notarized release or a claim of complete accounting support. Cash-method implementation remains dependent on the runtime check.

First artifact code revision: `fe60b628a63ef72996238730be2884d8ba245e61` on `cash-method-safety`. The corrected artifact has its separate revision below.
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

## First artifact identity

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

Initial AppleScript and `AXIsProcessTrusted()` checks reported false. After the user selected UI automation, the normal Accessibility permission prompt was requested; its immediate result remained false. A subsequent `AXIsProcessTrusted()` check returned **true**. This requested normal OS consent; it did not grant or bypass permission programmatically. The runner ancestry is the ThinkRail application (`bun`/`launcher` inside `ThinkRail.app`). At that first-build checkpoint, UI results were still pending; corrected-run observations are recorded below.

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

## Corrected artifact and native rerun

Corrected code revision: `47369a1586518451e70a537c70011f4159fbbee9`.
The test-first F-skatt classification fix and its independent review passed
267 Rust / 158 frontend tests. It adds meaningful F-skatt guidance rather than
skipping checks or inventing salary; FA salary requirements and every posting
eligibility guard remain unchanged.

The same unsigned package command was rerun. Its contiguous output is retained in
`artifacts/corrected-macos-build-2026-11-04.txt`. It again produced app and DMG with
signing skipped, with the same four recorded release warnings. Generated icon
changes were again restored after bundling.

Corrected artifact identities:

- App executable SHA-256: `7717428697ce31cca20d3f41eef36be265bd48bd256414ef9d518ff1c2c7a5de`.
- DMG SHA-256: `9fd4da7c773ec0bc41f4e9429c36c90d1fd49f418f4ab77cc127208e286ff63a`.
- The DMG was mounted read-only, its app copied into a disposable `Applications`
  directory and the image ejected. The copied executable's hash matched the built
  app executable. No installed application was overwritten.
- The copied app was launched by its packaged executable outside the repository,
  with `HOME=/tmp/oppenbokforing-native-uat-fixed.yWRchu`. Actual Rust data files were
  created under that home. The observed synthetic workspace label is
  `UAT Native Smoke 2026-11-0`; its business/owner are the UAT values above.
- Only that verified executable/PID's native controls were driven. Date entry used
  native keyboard/mouse events with owned-window and native hit-target checks;
  screenshots were not captured. No devtools JavaScript, runtime test-mode flag or
  accounting bypass was added to the app.

Observed outcomes:

1. **Setup passed:** a real F-skatt profile with zero salary and registered annual
   invoice-method VAT saved and reached the dashboard. No FA salary workaround was
   used. The optional tour was skipped through its normal control.
2. **Unknown facts stopped:** the invoice review initially had no selected fact
   answers. Trying to continue showed the missing/uncertain-answer stop message.
   The final successful facts were entered through native controls.
3. **Stale confirmation stopped without posting:** the initial native final review
   displayed gross SEK 1,250. A temporary, data-root-guarded test arranger called the
   public Rust `update_draft` service to model another caller changing this unposted
   draft to net SEK 2,000 / VAT 500 / gross 2,500. This was an audited service call,
   not a draft-editor UI claim or raw SQL posting. Confirming the old review returned
   to the retained facts step. Read-only before/after checks confirmed unchanged
   invoice posting state, zero vouchers/lines/validation/snapshots/documents or issue
   jobs, unchanged audit count, and the next invoice number still 1.
4. **Current financial review passed:** a fresh native preflight showed gross
   SEK 2,500 and the changed service description, rather than the older list amount.
   The retained fingerprint is
   `f9b6933df5ff7e7f643c8e72f3eeef843918c9db80d9701dbb05ea44f773da5d`.
5. **One native source action passed:** a native pointer click on the annual/no-EU
   Skatteverket reference opened in Firefox. Browser URL metadata matched the
   registered publisher/document path and was used only to identify this public reference; no unmatched URLs,
   other tab data or page contents were recorded. The earlier BFN PDF attempt did
   not yield confirmed document-display evidence and is not counted as a PDF-viewer
   pass or proof that every reference was opened.
6. **Issuance passed:** native confirmation created `2026-0001` dated 2026-11-04,
   one voucher and three lines: debit 1510 250,000 öre; credit 2611 50,000;
   credit 3041 200,000. There was no bank movement. The immutable issue and validation
   snapshots, successful replay and PDF job were retained. No email, payment or tax
   filing was initiated.
7. **Cold reopen and retention passed:** after closing the workspace and restarting
   the packaged app, its normal recent-workspace control reopened the same test
   data. The invoice and validation UI showed the original identity/fingerprint;
   read-only checks matched the earlier invoice, amounts, jobs, provenance and next
   number, with no duplicate posting. Lifecycle audit events appended normally.
   Retained PDF bytes begin with `%PDF-` and match the archive's SHA-256:
   `46ead1a55e6d3081faace1f6f12259aefc4de79207832361cd183129ecd8c292`
   (2,328 bytes). The PDF was not opened in a viewer or visually/legal-content checked.

Evidence: `native-old-review-2026-11-04.txt`,
`native-current-review-2026-11-04.txt`, `native-issued-invoice-2026-11-04.txt`,
`native-reopened-provenance-2026-11-04.txt`,
`corrected-native-runtime-state-2026-11-04.json`,
`native-ledger-pdf-check-2026-11-04.json` and
`native-source-browser-2026-11-04.json` under `artifacts/`.

The managed native sessions ended with exit code 0. Their output included
`sandbox_extension_issue_file_to_process failed ... Operation not permitted` for
the staged app during OS activation. This diagnostic is retained as an observation,
not hidden or treated as evidence of successful Finder/Gatekeeper admission. No
security control was disabled to avoid it. No signed distribution, normal global
installation, all-platform source opening, visual PDF correctness, full tax
preparation or cash-method posting has been verified by this run.

## Post-native automated gate and handoff

The complete post-native `npm run ci:public` returned exit 0: **267 Rust tests,
158 frontend tests**, fixtures/golden/M8, production build and generated bindings
check passed. The single-run output is retained in
`artifacts/post-native-public-ci-2026-11-04.txt`. Strict Clippy was rerun separately
and remains blocked by the same 9 library / 16 including-test findings, retained in
`artifacts/post-native-clippy-2026-11-04.txt`; no suppressions were introduced.

Independent review approved both the F-skatt source fix and the corrected native
evidence step. The blocking setup gap is closed for this tested flow. Broader
release/platform/PDF/tax claims remain excluded, and actual cash-method recognition
has not been implemented or enabled by this verification.

## Original runtime procedure and coverage

Use only disposable test data. Rust app-data resolution in the installed Tauri/dirs sources respects an explicit `HOME`; verify actual paths during launch rather than assume all macOS webview/preferences storage is isolated. Do not open real working books or overwrite an installed application.

The original procedure was:

1. Launch the identified packaged application with disposable application/workspace data.
2. Create a synthetic eligible 2026 annual/no-EU invoice-method workspace and invoice.
3. Verify the final review shows the fingerprint-bound current customer, lines and amounts; changed inputs must require a fresh review, not silent posting.
4. Click a compiled source and observe the actual system-browser action without inspecting unrelated browser tabs.
5. Issue the test invoice, reopen the workspace and inspect retained provenance/evidence.

The corrected-run observations above satisfy this bounded functional procedure.
They are actual packaged native observations, not inferred from build/mocked tests.
The first failed artifact, remaining diagnostic and all stated exclusions remain
part of the evidence; this does not establish full product or release readiness.
