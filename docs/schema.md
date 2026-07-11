# SQLite Schema Plan

Last updated: 2026-07-11. The current schema is defined by the 15 forward-only SQLx migrations listed below.

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

**Deferred (v2):**

- `due_dates`, `calculation_traces`, `backup_manifests` (backup flows exist without dedicated tables)

## Constraints

- All money values are integer minor units.
- Posted accounting rows are immutable.
- Journal lines must have either debit or credit, never both.
- Voucher posting must verify debit total equals credit total.
- Invoice numbers are unique per workspace and fiscal year.
- Locked periods reject new postings, reversals, invoice issuance, and VAT changes.

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
- Migration tests must run against an empty database and a seeded prior-version database.
