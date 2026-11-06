---
id: storage-contract
type: module-design
title: SQLite storage contract
status: active
parent: system-architecture
references: [local-sqlite, retained-evidence, product-goal]
---

# SQLite Storage Contract

The current schema uses the 21 forward-only SQLx migrations in this document.
This inventory does not establish implementation of every product requirement.

`product-goal` and the owning module specs define required behavior.
New storage support must not appear as an already-existing migration here.

## Scope

This schema supports v1 local desktop workflows: workspace setup, profiles, compliance rules, invoicing, ledger, documents/imports, reconciliation, VAT returns, year-end packages, SIE export, and workspace settings.

## Migrations

| Migration | Purpose |
|-----------|---------|
| `202607050001_initial.sql` | Core workspace, rules, profiles, fiscal years, accounts, vouchers, journal lines, audit, jobs |
| `202607070001_seed_2026_rules.sql` | 2026 rule seed data |
| `202607080001_m2_invoicing.sql` | Counterparties, invoices, credit notes |
| `202607080002_m2_idempotency_guards.sql` | Idempotency indexes |
| `202607080003_m3_documents_imports_reconciliation.sql` | Documents, CSV imports, staged transactions, reconciliation matches |
| `202607080004_reconciliation_payment_guards.sql` | Payment reconciliation guards |
| `202607080005_m4_vat_cash.sql` | Fiscal periods, VAT codes/returns, tax reservations |
| `202607080006_voucher_accounting_date.sql` | Voucher accounting date column |
| `202607080007_m5_year_end.sql` | Year-end packages, NE fields |
| `202607100001_m6_workspace_settings.sql` | Workspace locale settings |
| `202607100002_m7_ledger_read_indexes.sql` | Ledger read-query indexes |
| `202607100003_invoice_pdf_document.sql` | Invoice PDF document reference |
| `202607100004_workspace_path_settings.sql` | Default export/backup directories |
| `202607110001_dashboard_tour.sql` | `dashboard_tour_completed` on `workspace_settings` |
| `202607110002_simple_mode.sql` | `simple_mode` on `workspace_settings` |
| `202607270001_posted_voucher_immutability.sql` | Posted voucher/line protection and append-only audit triggers |
| `202608200001_tax_task_deadlines.sql` | VAT deadline regime and rule-backed filing schedules |
| `202609180001_year_end_artifact_integrity.sql` | Year-end artifact hashes |
| `202609180002_invoice_issue_snapshots.sql` | Immutable issue-time invoice provenance |
| `202609180003_invoice_snapshot_recovery.sql` | Explicit historical snapshot recovery audit |
| `202609180004_vat_review_snapshots.sql` | Nullable VAT review revision, canonical snapshot JSON and SHA-256 digest; no historical backfill |

## Tables

**Core (initial):**

- `workspaces`, `rule_versions`, `tax_rules`
- `sole_trader_profiles`, `tax_profiles`, `vat_profiles`
- `fiscal_years`, `accounts`, `vouchers`, `journal_lines`
- `audit_events`, `local_jobs`

**Invoicing (M2):**

- `counterparties`, `invoice_sequences`, `invoices`, `invoice_lines`, `credit_notes`

**Documents and reconciliation (M3):**

- `documents`, `csv_imports`, `staged_transactions`, `reconciliation_matches`

**VAT and cash (M4):**

- `fiscal_periods`, `vat_codes`, `vat_returns`, `vat_return_boxes`, `tax_reservations`

**Year-end (M5):**

- `year_end_packages`, `ne_fields`

**Settings (M6):**

- `workspace_settings` — `locale`, `updater_enabled`, `default_export_directory`, `default_backup_directory`, `dashboard_tour_completed` (M8), `simple_mode` (M8)

**Additional contracts:**

- `invoice_issue_snapshots` retains immutable issue-time provenance; `invoice_snapshot_recovery_requirements` records legacy evidence requirements. Historical recovery attestation is recorded in audit metadata.
- `vat_profiles.vat_filing_deadline_regime` selects a rule-backed schedule; tax tasks are derived, not stored as task rows.
- `vat_returns` carries nullable `review_revision`, `review_snapshot_json` and `review_snapshot_hash`; `vat-returns` owns versioning, stale rejection and legacy compatibility.
- Year-end packages carry artifact hashes; their ledger fingerprints are retained in audit metadata.
- Backup manifests are files, not database tables; calculation traces are assembled from existing records.

## Constraints

- Money columns use integer minor units; posting services use integer VAT basis points for stored invoice rates.
- Database triggers reject UPDATE/DELETE of posted vouchers and their existing journal lines, and reject all audit-event UPDATE/DELETE.
- Journal-line checks require nonnegative amounts with exactly one positive debit or credit side.
- Services verify balanced voucher totals. There is no database balance trigger or blanket prohibition on inserting another line into a posted voucher; immutability is not full database sealing.
- Invoice sequences are unique per workspace/fiscal year; posted invoice identity and its issue snapshot are separately protected.
- Posting/approval services enforce fiscal-year and date-range period locks; this is not a universal SQL authorization layer.
- Keyed `local_jobs` rows are unique by `(workspace_id, job_type, idempotency_key)` when the key is non-null. The table stores both synchronous replay results and queued PDF work, not a general scheduler.

## Indexes

Ledger read indexes (M7) plus domain indexes from M2–M6 migrations cover:

- `workspaces(active_rule_year)`
- `rule_versions(tax_year, status)`
- `tax_rules(rule_version_id, family, key)`
- `accounts(workspace_id, number)`
- `vouchers(workspace_id, status, created_at)` and accounting-date variants
- `journal_lines(voucher_id)`
- `audit_events(workspace_id, created_at)`
- `local_jobs(status, created_at)`
- `invoices(workspace_id, status, issued_at)`
- `documents(workspace_id, content_hash)`
- `staged_transactions(workspace_id, status)`
- `vat_returns(workspace_id, fiscal_period_id)`
- `year_end_packages(workspace_id, fiscal_year_id)`

## Migration Rules

- Migrations are forward-only.
- Never rewrite historical accounting rows in migrations.
- Data backfills must record an `audit_events` row.
- Rule changes create new `rule_versions`; they do not mutate old rule versions.
- Migration tests must run against an empty database and a seeded prior-version database. This is a verification requirement, not a claim that this documentation import ran them.
