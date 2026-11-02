---
id: rust-backend
type: module-design
status: draft
title: Rust application and shared infrastructure
parent: system-architecture
references: [rust-command-boundary, storage-contract, security-privacy, local-sqlite]
---

# Rust application boundary

## Responsibility

Own the native application facade, shared local persistence, accounting services,
and filesystem artifacts. `lib.rs` composes the desktop runtime and explicit
command allowlist. `commands.rs` adapts DTOs/results and orchestrates workspace
lifecycle; it is not purely a delegation layer. Domains remain callable without
the `desktop` feature for native tests and bindings export.

## Boundary

The renderer reaches this crate only through `rust-command-boundary`. Services
may use the shared pool, structured errors, audit helpers, and replay-key helpers;
they do not call the Tauri command adapter or depend on UI state. This is a
source-observed organization, not enforced Rust package isolation. The accepted
transaction requirement lives in `local-sqlite`; known gaps live in
`security-privacy`, not in exceptions to that requirement.

`storage-contract` owns money, posting-row, audit, and replay uniqueness rules.
Replay-key normalization only trims and rejects empty keys; payload comparison
and recovery belong to each operation. SQL errors become redacted `AppError`
responses; public errors and retained audit metadata have different privacy roles.
Workspace selection and in-flight operation semantics live in `system-architecture`.

## Meaningful service edges

These are direct function edges, not exclusive table ownership. Child frontmatter
records the corresponding service dependencies; shared-table reads are called out
where they change the boundary.

- Issuance/credits call ledger primitives and archive verification. PDF jobs call
  invoice snapshot/rendering and archive storage; issuance enqueues rows directly,
  rather than calling the PDF worker.
- CSV import calls evidence import. Expense and payment services verify evidence
  but write their own vouchers and staged/match rows; they do not funnel writes
  through ledger posting primitives.
- VAT calls ledger revenue reads; ledger calls VAT period guards. Workspace bootstrap
  calls VAT seeding while VAT calls workspace fiscal helpers. These are cycles,
  not a layered DAG.
- Onboarding profiles call compliance, which reads typed rules. Cash planning reads
  ledger/profiles/VAT; tax tasks read profiles/rules and return/package tables.
- Year-end calls rule/fiscal helpers and VAT period-key helpers, while aggregating
  journal and return tables itself. Accountant handoff calls SIE and year-end lookup.
- Path resolution reads settings, while settings use path validation. This is also
  a cycle; neither is an independent filesystem authorization service.

## Shared infrastructure not split into separate specs

Database opening/migration follows `local-sqlite`. Existing-open checks a registered
workspace and exact stored database path before migration; moving a directory is
not a supported relocation protocol. Counterparties are workspace-scoped lookup
records with audited creation; invoicing validates them through SQL, not a service call.

Recent-workspace history is an app-level plaintext file, capped at ten and keyed by
exact database path. Observed limitation (acceptance unconfirmed): it is directly
rewritten without synchronization; an error can occur after the active workspace
has already changed. It is a convenience index, not the workspace system of record.

Integration status reports BankID and open banking unavailable even when their
feature flags are enabled. Updater configuration/settings do not install a runtime
updater. Observed support limits are unconfirmed as permanent product exclusions;
no remote-service capability should be inferred from these switches.
