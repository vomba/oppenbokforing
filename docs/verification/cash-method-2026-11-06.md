# Bounded cash-method verification — 2026-11-06

Status: automated checks and native bundle/helper checks pass. Installed GUI cash UAT is pending.

## Scope and basis

These checks cover the bounded 2026 cash issue and full-PDF receipt implementation.
They do not establish complete cash accounting, unpaid closing, cash VAT/year-end approval or legal PDF completeness.
The bank PDFs are synthetic. No bank authentication, OCR, named-bank compatibility or professional attestation is claimed.

The checked build used the working tree after source-evidence commit `90e192c`.
The implementation commit and installed-runtime observations belong in the completed handoff.
Published invoice-v1 evidence and its native history remain separate preservation checks.

## Automated checks

- `PATH="$HOME/.cargo/bin:$PATH" npm run ci:public` → exit0, including26 helper-build tests,192 frontend tests, all Rust suites/probes, golden/M8, production build and bindings.
- `PATH="$HOME/.cargo/bin:$PATH" cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --bin oppenbokforing-pdf-helper` →2 passed after the entrypoint relocation.
- `git diff --check` → pass.
- `spec_validate` → no issues at the recorded check.
- `PATH="$HOME/.cargo/bin:$PATH" cargo clippy --manifest-path src-tauri/Cargo.toml --no-default-features --all-targets -- -D warnings` → exit101: existing9 library/16 library-test findings remain. No suppression was added.

The ignored PDF probe entrypoint is a subprocess harness, not omitted behavior.
Six ordinary tests invoke that entrypoint explicitly in bounded owned child processes.
They cover normal/compressed, malformed, missing-helper, cyclic and expansion cases.
The outer harness kills and reaps a regression that would otherwise hang the test runner.

Strict Clippy debt remains in backup, documents, VAT, year-end, invoice-PDF and SIE code.
The release build also reports four existing unused-variable warnings in commands/error handling.
These failures are not described as green lint or fixed by unrelated cleanup.

Initial frontend helper/component import failures were not qualifying behavioral RED.
Compiled backend RED transcripts and later frontend integration regressions are identified in the accounting dossier.

## Independent review

Source reviewers approved the independently prepared expectations and their applicability.
Implementation review then reproduced an unbounded PDF parser and rejected staged-expense continuation.
The implementation added process isolation and validated ordinary-expense match provenance.
The focused security follow-up approved those corrections.
A separate UI follow-up approved bounded modal layout and accurate CSV-only status wording.
Review IDs and qualifications are retained in [the cash dossier](../accounting-validation/2026-cash-pdf-full.md).
Final plan-step review and installed GUI UAT are separate gates.

## Native artifact checks

`PATH="$HOME/.cargo/bin:$PATH" npm run tauri:build -- --bundles app,dmg` → exit0 after packaging correction.
An earlier attempt failed because automatic binary discovery requested `pdf_helper`, not the explicit Cargo name.
Moving the standalone helper outside `src/bin` removed that name mismatch.
The actual bundle contains both named executables under `Contents/MacOS`.

| Artifact | SHA-256 |
|---|---|
| Main executable | `d57f93bf32ac4c95f7f0c1d2f250bdb30ae84502e357fb39aa320eda9be6cbf7` |
| Bundled parser helper | `327ba2288959306f1999549918c60c41d00f8fa9d81be26ffe4941926054eef2` |
| `ÖppenBokföring_0.1.0_aarch64.dmg` | `9ab1596756f98f22653ff5f6667c9f71231f5fb3f4ced9e11d4d79f28a8257f2` |

`file` identifies the helper as native arm64 Mach-O.
`codesign -dv --verbose=4` identifies only an ad-hoc linker signature, no Team ID and no sealed resources.
This is not a developer-signed, notarized or Gatekeeper-admitted release.

The bundled helper itself processed three retained synthetic fixtures with an empty environment:

- Normal compressed ObjStm → exit0, matching hash,6 objects/1 page,0.308 seconds.
- Cyclic `/Prev` → SIGXCPU, no valid reply,2.011 seconds.
- 512MiB decoded expansion → SIGABRT, no valid reply,0.061 seconds.

Every probe child was reaped. No fallback parser ran in the accounting process.
Exact input hashes, sizes, exits and timing are in [the artifact record](artifacts/cash-helper-bundle-2026-11-06.json).
These are build-bundle executions, not installed-app GUI transactions.
Resource limits are not universal RSS or stack-memory ceilings.

## Review correction: unstaged expense continuation

Review `rc_c993d9f2` found that unstaged expenses bypassed checks applied through bank matches.
A public `2025-12-31` expense after a 2026 origin wrongly allowed preflight, issuance and receipt.
The retained `cash-unstaged-expense-red.txt` transcript records those three unexpected successes before the fix.

A common predicate now validates every ordinary expense voucher against the immutable origin dates and scoped posting provenance.
It checks balanced compatible accounts, original expense-job values and any required staging links.
Valid staged and unstaged expenses remain supported, and first-origin emptiness is unchanged.
The continuation suite passes8 tests, including seven new unstaged corruption scenarios.
The full public CI gate also passes after this correction.

Follow-up review `rc_3e8ff58b` found a writer/reader normalization mismatch.
The writer accepts blank optional references and Rust-whitespace-padded account, document and staging IDs.
Continuation checks now use the same Rust helpers and normalize present reference fields only in memory.
Raw history and exact-input replay identity remain unchanged, and absent history fields are not populated.
Public regressions cover five valid input variants, invalid padded links and the existing origin/provenance guards.

The artifact hashes above identify the earlier implementation build, not these review corrections.
Rebuild the app/DMG before using them for post-fix GUI UAT.

## Remaining verification

- Mount the new DMG read-only and copy the app into a disposable Applications directory.
- Exercise first-year facts, unknown/stale stops, identity-only issue and retained PDF through the actual GUI.
- Record a simulated full bank receipt through that copied app and its adjacent helper.
- Verify receipt-date journals, duplicate/source guards, closing blockers, restart and backup/recovery.
- Inspect native dialog geometry and source opening.
- Record the exact source revision and artifact identity for that runtime.

Native Intel/universal execution remains unverified locally because only the arm64 Rust target is installed.
The build-plan tests preserve the existing universal release path but do not execute that cross-build.
Remote CI, other-platform enforcement and signed/notarized helper operation remain unverified.
