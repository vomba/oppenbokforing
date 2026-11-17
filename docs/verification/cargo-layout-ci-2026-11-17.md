# Cargo artifact layout correction — 2026-11-17

PR #58's [first remote run](https://github.com/vomba/oppenbokforing/actions/runs/44072966860) failed on both Ubuntu and macOS.
The runners updated to Cargo/Rust 1.100. Local verification used 1.99.1.
The new test paths end in `debug/build/oppenbokforing-desktop/<hash>/out`.
The real Cargo helper existed, but the old `debug/deps`-only mapping selected a missing runtime path.
This was not a flaky timeout or a reason to weaken parsing limits.

## Correction

A debug-only mapper recognizes the observed package name, 16-hex hash and fixed directory components.
It selects only the fixed profile sibling. It does not search ancestors or try alternate helpers after failure.
Unrecognized layouts retain ordinary adjacent discovery. Release discovery, parsing, resource limits and bookkeeping authorization remain unchanged.

`npm run test:rust` now invokes the existing native helper build/staging command first.
This makes the runtime prerequisite explicit instead of depending on Cargo's incidental integration-artifact layout.
The command uses the existing toolchain and dependencies. It grants no application or OS permissions and introduces no parser override.
Direct Cargo callers use the documented staging prerequisite.

The obsolete test calculation assumed two parent directories from every integration executable.
Its replacement checks the actual Cargo binary and real public-service behavior, not a guessed artifact path.
A copied test process under the new layout recognizes a valid receipt using the real fixed helper.
After removal of that owned profile copy, it rejects recognition even with the real parser at the wrong adjacent location.
All original six isolated probes, writer-availability checks, malicious inputs and positive compressed/ordinary controls remain.
New lookup tests are debug-scoped because their mapped implementation is also debug-only.

## Verification

- [Compiling behavioral RED](../../fixtures/historical-regressions/cargo-helper-layout-red.txt) → wrong selected path reproduced before the fix.
- PDF supervisor unit tests → 6 passed, including both exact layouts and nonmatching lookalikes.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --test cash_pdf_isolation` → 8 passed plus 8 bounded child executions. The existing harness entrypoint remains explicitly invoked.
- `PATH="$HOME/.cargo/bin:$PATH" npm run ci:public` → pass, 383 Rust passes including those child executions, 212 frontend tests and 26 helper-plan tests. The new real-helper prerequisite ran.
- `PATH="$HOME/.cargo/bin:$PATH" npm run test:smoke` → pass.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --release --no-default-features` → pass.
- Targeted Rustfmt check, `spec_validate` and `git diff --check` → pass.
- Strict Clippy → unchanged 9 library/16 library-test findings. No suppression was added.

The final missing-helper assertion was strengthened after the full gate and the complete isolation suite passed again.
The separate [native current-base observations](pr-sync-2026-11-17.md) retain their original source identity and 378-pass checkpoint.
They are not relabelled as executing this later debug-only change. No post-correction package run is claimed.
Remote CI still must pass on the updated PR head before merge.
