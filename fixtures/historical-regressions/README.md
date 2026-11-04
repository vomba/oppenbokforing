# Pre-eligibility invoice engineering regressions

These are **engineering regression artifacts**, not independent accounting
validation. They preserve historical behavior outside the newly accepted case;
they do not authorize new quarterly, exempt, 6%, 12%, mixed-rate or 2027 invoices.

## Archived baseline and reproducibility

`pre-gate-invoice-shapes.json` was captured by executing production issuance at
commit `44f897e` in a temporary directory created with `git archive` (not a
checkout). `export-pre-gate-shapes.rs` contains the exporter; append it to that
commit's `src-tauri/tests/invoice_idempotency.rs`, then run:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features \
  --test invoice_idempotency export_pre_gate_historical_shapes -- --exact --nocapture
```

The `HISTORICAL_SHAPES=` line contains the JSON. Generated database IDs and times
are intentionally projected out. Recorded fields include exact totals, numbering,
account/date/rate-specific journal rows, old rule snapshot, PDF/replay job states
and audit actions. The baseline setup uses quarterly invoice-method reporting.
All five baseline cases (exempt, 6%, 12%, 25%, mixed 25%/12%) succeeded.

`missing-facts-red.rs` is the retained missing-facts regression in its original
API form. Append it instead and run the command retained in
`missing-facts-red.txt`. The transcript uses a tracked `.txt` extension because the
repository excludes generated `*.log` files. The recorded command runs from the
archived `src-tauri` directory; from the archive root add `--manifest-path src-tauri/Cargo.toml`.
Compilation succeeds and the assertion fails because
baseline issuance returns an **issued** invoice with voucher/PDF job despite no
case facts. This is behavioral RED proof, not a new-DTO compile failure. The
current integration test retains this regression with both optional new input
fields set to `None`.

## Current fixture boundary

`src-tauri/tests/common/mod.rs::seed_historical_invoice` is test-only SQL. It
materializes the pre-gate invoice/voucher/journal/snapshot/PDF/replay/audit shape
from a draft, using explicit historical dates, archived rate-code mappings and
the baseline line-rounding expression. The numerical/history assertions in the
calling suites remain in force. The helper never calls the current issue or
invoice-posting APIs, switches the profile to manufacture authorization,
inserts eligibility provenance, disables protection triggers, or uses a runtime
permit. An integration test compares every archived shape against this SQL
fixture, including the absence of validation provenance. Historical 2027 setup
retains its explicit old test rule and fiscal-year/number assertions.

### Classification of migrated tests

- **Live eligible issuance:** invoice idempotency, invoice PDF jobs, the M2 credit
  source now use annual/no-EU profiles and explicit
  favorable case facts plus a fresh preflight fingerprint. Existing assertions
  about credits, amounts, payment dates, snapshots, PDFs, jobs and replay remain.
  Negative issue tests still call the actual issue API even if preflight fails.
- **Historical unsupported accounting:** M3 milestone and regression settlement sources, M4
  quarterly VAT/12%/mixed/exempt cashflow and threshold sources, M6 SIE/accountant
  exports, M7 ledger reads and quarterly VAT traces, M8 exempt outstanding-count
  source, and the 2027 fiscal-year-numbering regression use historical SQL setup.
  They keep their existing profile/rate/date/number/accounting assertions; they
  are not new-issuance eligibility tests. Quarterly calendar/trace assertions now
  name the forward active rule identity, without a broader validation claim.
- **Exempt threshold preflight:** new exempt issuance is intentionally rejected
  with field `eligibility`, so the previous successful exempt preflight/block
  audit claim is no longer applicable. The replacement retains threshold
  equality (12,000,000), the one-ore crossing (12,000,001), warning/registration/
  charging diagnostics and active rule/source provenance on historical ledger
  data, plus explicit rejection of new exempt issuance. M4 retains its original
  11,000,000 / 12,500,000 threshold math and new exempt rejection.
- **Low-level positive posting:** method selection alone is no longer authority.
  Its former positive invoice calls now assert rejection; actual reviewed
  issuance supplies the positive sources. Both reversal entrypoints and the
  original four-voucher/zero-net-revenue/zero-net-VAT assertions remain. Separate
  cloned-pending-context tests exercise the invoice lower boundary, including
  wrong buckets, invoice, date and fiscal year, without a production bypass.
- **Rule-year rejection:** an unprovenanced 2027 request now fails at the stricter
  eligibility boundary, retaining draft/no-2027-year/no-snapshot assertions.
  Direct tests additionally cover missing/retired 2026 rules and changed actual
  payloads despite unchanged version IDs/checksums.

No fixture in this directory claims legal completeness of PDFs, books, credits,
settlements, VAT returns or year-end preparation.
