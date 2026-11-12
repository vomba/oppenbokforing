# Packaged cash-method verification — 2026-11-09

## Status and scope

The bounded cash GUI flows passed on the copied `6c60fbe` app.
Native testing found a wrong SIE first-year header. The correction passed a rebuilt,
separately copied app's native export test. Independent review verdicts remain
separate from these observations and are retained in the plan review trail.

All data is disposable and synthetic. No real bank execution, bank authentication, OCR, named-bank compatibility or professional attestation is claimed.
This is not complete cash-method, closing, return or legal PDF validation.

## Build and copied artifact

Source revision: `6c60fbe2b113c20f95bcbc515e6518644d5510cf` on `cash-method-safety`.
The worktree was clean before verification artifacts were added.

- `PATH="$HOME/.cargo/bin:$PATH" npm run ci:public` → exit0 before packaging.
- `npm run tauri:build -- --ci --no-sign --bundles app,dmg` → exit0 with the normal Cargo path available.

Signing credential variables were removed from that build child only.
The build reported the existing four release unused-variable warnings; no suppression or unrelated cleanup was added.

The DMG was mounted read-only. Its app was copied into an owned temporary Applications directory, then the image was ejected.
No normal installed application or real workspace was overwritten.

| Artifact | SHA-256 |
|---|---|
| Main executable, built and copied | `23253f68e63e33337a42b41e82b796df31294b1a95b5517f725405c1e29d2ab8` |
| Copied parser helper | `327ba2288959306f1999549918c60c41d00f8fa9d81be26ffe4941926054eef2` |
| Complete arm64 DMG | `e48c8acda179cc41f13f7f6d23acb1c9f911f11b6ac2bca7a1fa204960150e53` |

Copied application: `/tmp/oppenbokforing-cash-uat.vtg3kD/Applications/ÖppenBokföring.app`.
It ran by its packaged executable outside the repository, with an isolated home and explicit disposable data/cache roots.
Actual Rust databases and retained documents were observed beneath that root.
Isolation of every macOS preference, native-picker cache or browser state is not claimed.

Only the verified executable/PID's native controls were driven. No devtools JavaScript, runtime permission flag or accounting bypass was added.
The native sessions used PIDs14318,87909 and97224. All three terminated gracefully with exit0.
macOS logged `sandbox_extension_issue_file_to_process failed ... Operation not permitted`; the functional sessions still ran.
This is not Finder/Gatekeeper admission or developer signing/notarization evidence.

## Native observations

| Check | Actual observation |
|---|---|
| Onboarding | F-skatt, salary0, registered annual VAT, cash method and May12 regime saved through the GUI. |
| Initially unknown issue facts | Action stopped with no origin, number, snapshot, voucher or journal. |
| Stale final review | The visible SEK10000 review was invalidated by an audited public draft update to SEK12500. Confirmation rejected without issuance. |
| Fresh review | The GUI displayed current customer/description and SEK12500. Actual first-year startJune1, completionJune10 and issueJune15 were explicit. |
| Identity-only issue | `2026-0001` retained origin, snapshot, PDF and audit. The year became June1–December31. Vouchers0, lines0, no1510. |
| Long dialogs | Native issue facts/final review and receipt facts/final review were contained in the window: width700, height716, window1280×874. |
| Receipt defaults | Actual amount/date/currency/bank identity and confirmations were blank or unknown, not copied from invoice total. |
| Partial receipt | SEK12499.99 stopped; source and fields remained available with no recognition. |
| Generated invoice PDF | The reimported issued PDF was rejected as bank evidence, without posting. Its original bytes remained unchanged. |
| Missing installed helper | Temporarily renaming only the disposable copied helper blocked recognition. Restoring it preserved the helper hash. |
| Cyclic PDF | The installed helper rejected the controlled cyclic fixture. A separate SQLite connection acquired `BEGIN IMMEDIATE` in0.000076 seconds during parsing. |
| First full receipt | One June20 voucher:1930DR1250000,3041CR1000000,2611CR250000 öre. No1510 and no issue-date recognition. |
| Second cash identity | `2026-0002` issued without another voucher. |
| Same PDF, distinct entry | A genuine second simulated entry in the already-consumed PDF was blocked under the selected one-PDF rule. |
| Same bank identity, new PDF | Reuse of the first receipt's identity was blocked with one receipt still posted. |
| Distinct receipt | A separate retained PDF/identity posted the second receipt onJune21. Total2 vouchers/6 lines. |
| Cash preparation | With both invoices paid, readiness still showed unsupported cash preparation. A GUI VAT approval attempt rejected without posting or locking. |
| Cold reopen | Recent-workspace reopening preserved numbers, retained bytes and all origin/issue/receipt digests. GUI history distinguished null issue accounting date from June20 receipt recognition. |
| Encrypted backup/restore | GUI backup and native-file-picked restore succeeded. Restore used a fresh directory, retained the logical workspace ID and preserved financial rows, proof digests and all document hashes. |
| Source opening | The compiled cash source action was confirmed at its requested Riksdagen URL in Firefox. Only URL metadata for the requested public document was recorded. |
| Deployed normalization fix | After a supplemental public expense with padded accounts/blank optional references, the copied GUI issued `2026-0003` without journals and recognized its full receipt onJune24. |

The first bank fixture contains two clearly labelled simulated entries.
Using its second entry was deliberately rejected despite a distinct transaction reference.
The bank-identity test deliberately attempted to assign an already-used receipt to another invoice.
Those negative attempts are not claims that their owner confirmations described valid real transactions.

Reimporting the invoice changed its document display filename through ordinary import deduplication.
The issued snapshot, retained PDF bytes and content hash did not change.

## Evidence and supplementary probes

- [Selected native Accessibility observations](artifacts/cash-native-accessibility-2026-11-09.txt) are excerpts from separate reads, not a combined terminal verdict.
- [State observations](artifacts/cash-native-state-2026-11-09.json) record22 phases with scoped read-only SQL and retained-byte checks.
- [Recorded-state assertions](artifacts/cash-native-state-assertions-2026-11-09.json) passed53 checks across those phases.
- [Writer availability](artifacts/cash-native-parser-writer-2026-11-09.json) records the independent transaction probe during cyclic parsing.
- [Source-browser observation](artifacts/cash-native-source-browser-2026-11-09.json) identifies only the requested source URL.
- [Supplementary public-service probes](artifacts/cash-native-public-probes-2026-11-09.json) distinguish library calls from packaged-GUI actions.

The supplemental runner used the application source and matching production dependency versions/checksums.
It guarded every database path, company and workspace against the disposable root.
Twelve nonmutating replay, changed-request, method-save and readiness probes passed on original/restored data.
Matching cached receipts worked without an adjacent parser helper; changed amounts rejected.
Both method-save routes rejected. This was after paid receipts, not native zero-voucher freeze evidence.

One explicit padded/unstaged expense was then posted through the public service on the restored copy only.
It created the expected5610/2641/1930 voucher. Subsequent cash issuance and receipt were actual copied-app GUI actions.
The original book remained at2 vouchers/6 lines; the restored continuation ended at4 vouchers/12 lines, including that expense.
These observations do not independently validate expense deductibility or input VAT treatment.

## Native SIE failure and corrected rerun

The Settings export wrote `#RAR 0 20260101 20261231` although the stored first year starts2026-06-01.
Voucher dates and values were correct. The renderer fabricated the header's January start.

The original failed export is preserved in [the text artifact](artifacts/cash-native-sie-first-year-failure-2026-11-09.txt).
[The failure record](artifacts/cash-native-sie-first-year-failure-2026-11-09.json) retains expected/actual header, source revision and hash.
Do not describe that artifact as a passing SIE result or silently replace it.
The correction retained a compiling public regression and passed full public CI.
The required review requested the rebuilt copied-app check rather than accepting automated tests alone.

The corrected app was built with `--ci --no-sign --bundles app,dmg`, then copied from a new read-only DMG mount.
It used a separate disposable application path: `Applications/ÖppenBokföring-SIE-fixed.app`.
The original copied app and failure artifact were not replaced.
Native recent-workspace opening selected the restored June-start book.
Native Settings exported into the separate `sie-output-corrected` destination.
It emitted **`#RAR 0 20260601 20261231`**.
All non-`#RAR` export lines are byte-identical to the original failure capture.
Four vouchers, twelve journal lines and all three raw cash-proof digests remained unchanged.

| Corrected artifact | SHA-256 |
|---|---|
| Copied main executable | `78804bed44b54599dd00966ece9e9063fb76e3d17aa74af3c726c435baac06c4` |
| Copied helper | `327ba2288959306f1999549918c60c41d00f8fa9d81be26ffe4941926054eef2` |
| Corrected complete DMG | `6844229af3e156352b5da7007c4482eefa98621a70c207f78d7865aa384b4a9a` |

The corrected source was base `6c60fbe` plus the uncommitted SIE date-provenance correction at build time.
Its `src-tauri/src/sie/mod.rs` hash was `0313195f492082e34952bb32607344393bddad118ded5b42409219deff3348dd`.
This binds the new artifact without pretending it used the unchanged pre-fix source tree.
No distinct SIE commit existed at native build time; the base/source/binary hashes
identify what was checked. Later commit metadata must not replace that build identity.
Subsequent commit `676b1965dd2c8b5bc07e6d8fddd61df814bac998` records the same checked SIE source; it was created after native verification.
[Corrected native evidence](artifacts/cash-native-sie-corrected-2026-11-09.json) retains those identities and the original failure hash.
The [corrected export text](artifacts/cash-native-sie-corrected-2026-11-09.txt) and [contiguous build output](artifacts/cash-native-sie-fixed-build-2026-11-09.txt) are retained.

## Automation and coverage qualifications

Native event delivery required a short run-loop interval after input/action dispatch.
Some initial automation attempts did not change React-controlled fields or opened the wrong assertion state.
Read-only backup-path input correctly required the native picker, not typing.
Assertions were corrected to the actual localized status text; those harness errors are not app fixes.

Empty HTML date inputs exposed a synthesized AX date but zero date segments.
Unknown-state action rejection and final reviewed ISO values, not that placeholder date, establish the tested state.
No screenshots or unrelated browser page content were recorded; global file-picker listings were excluded from published observations.

Automated suites separately cover the larger unknown-fact, overflow, race, cross-workspace, future/cross-year and legacy guard matrices.
The native run is not a claim that every variant was manually repeated.
Prior native invoice-v1 migration/backup preservation is separate engineering coverage, not independent accounting validation.

## Final gates and independent audit

The final `npm run ci:public` run passed all gates.
Its Rust transcript records347 passes, including six isolated worker executions, and one ignored harness entrypoint executed by those probes.
Frontend192 tests and helper-build26 tests passed. Production build, bindings, golden and M8 checks passed.
`npm run test:smoke` passed in integration-only mode; actual native packaging is independently recorded above.
[Final gate totals](artifacts/cash-final-gates-2026-11-09.json), [public CI output](artifacts/cash-final-public-ci-2026-11-09.txt) and [smoke output](artifacts/cash-final-smoke-2026-11-09.txt) are retained.

Independent audit `01a1ca1a-4cc4-7355-a36b-d8243b1900eb` approved this evidence without material findings.
It repeated53 assertion-only checks across22 phases and inspected both disposable databases read-only.
It verified journals, proof digests, retained bytes, source/binary identities, restore separation and all non-header export bytes.
It also checked the twelve supplementary probe records and their matching dependency lock.
The auditor did not rerun CI, launch native actions or mutate data.
That engineering audit is not professional or tax-authority attestation.

The accepted design and defect decisions now live in the owning durable specs.
The temporary cash task was retired after reconciliation.
The required final step review is retained separately in the plan review trail.

## Gate scope and remaining limits

The required native observations and corrected-export evidence are retained above.
The independent step verdict is separate from these recorded observations.
The checked SIE source and executable hashes bind the pre-commit native build.
No separate implementation commit is invented or required as a substitute for those checks.

Strict Clippy debt remains unchanged at9 library/16 library-test findings.
Native Intel/universal execution, other-platform enforcement, signing/notarization and legal PDF/SIE completeness remain unverified.
Cash CSV recognition, credits, partials, over/refunds, affected legacy/opening workflows and cash VAT/year-end approval remain blocked.
No broader cash-method or tax-return completion claim is made.
