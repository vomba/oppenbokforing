# Module Codemap

Rust domain modules and React frontend modules. Command wrappers live in `src/lib/commands.ts`; generated types live in `src/lib/bindings.ts`.

Last updated: 2026-07-11.

## Rust domain modules (`src-tauri/src/`)

### Core infrastructure

| Module | Purpose | Key exports |
|--------|---------|-------------|
| `commands` | Tauri command handlers; workspace gate | Registered `#[tauri::command]` functions |
| `bindings` | Specta/TS type export | `CommandResponse<T>`, input types |
| `error` | Structured `AppError`; SQL redaction + unique-violation flag | `AppError`, `is_sqlite_unique_violation` |
| `db` | SQLite connect + migrate | `connect_workspace`, `open_existing_workspace` |
| `state` | In-memory open workspace | `AppState`, `WorkspaceContext` |
| `workspace` | Bootstrap seeds, readiness | `ensure_workspace_ready` |
| `paths` | User path validation | absolute paths, reject `..` |
| `audit` | Audit event recording | `record_event` |
| `jobs` | `local_jobs` queue (PDF, etc.) | `process_pending_invoice_pdf_jobs`, `invoice_pdf_status` |

### Profiles, rules, compliance

| Module | Purpose |
|--------|---------|
| `profiles` | Business, tax, VAT profile CRUD |
| `rules` | Active `rule_versions`, `tax_rules` lookups |
| `compliance` | Golden-scenario evaluation engine |

### Accounting domains

| Module | Purpose | Idempotency |
|--------|---------|-------------|
| `counterparties` | Customers/suppliers | — |
| `invoicing` | Draft/issue/credit invoices; open count | issue, credit |
| `invoicing/pdf` | PDF render bytes | — |
| `ledger` | Vouchers, accounts, balances | — |
| `documents` | Content-addressed import, reveal, list | import |
| `imports` | CSV import + staged transactions | csv create |
| `reconciliation` | Match payments to invoices | match create |
| `expenses` | Manual expense posting | post |
| `vat` | Returns, approval, threshold | approve |
| `cashflow` | Spendable cash overview | — |
| `year_end` | K1/NE packages, readiness | create, approve |
| `sie` | SIE type 4 export | export create |
| `accountant_package` | Package export/validate | export create |

### Workspace services

| Module | Purpose |
|--------|---------|
| `backup` | Encrypted `.skatbackup` create/restore |
| `backup/crypto` | Argon2id + AES-256-GCM |
| `settings` | Locale, paths, tour, simple mode |
| `recent` | Recent workspace list (app data) |
| `integrations` | Open Banking / BankID stubs |

## Tauri commands by domain

Commands are registered in `src-tauri/src/lib.rs` `generate_handler![…]`.

| Domain | Commands |
|--------|----------|
| Workspace | `workspace_create`, `workspace_open`, `workspace_close`, `recent_workspaces_list`, `workspace_backup_create`, `workspace_backup_restore` |
| Profiles | `business_profile_*`, `tax_profile_*`, `vat_profile_*` |
| Compliance | `compliance_check_run`, `rule_version_get` |
| Invoicing | `counterparty_*`, `invoice_*` (list, create/update draft, issue, credit, open count, PDF status) |
| Documents and imports | `document_import`, `document_reveal`, `document_list`, `csv_import_create`, `staged_transactions_list`, `reconciliation_match_create`, `expense_post` |
| VAT | `vat_return_*`, `vat_threshold_status_get`, `cashflow_overview_get` |
| Year-end | `year_end_package_*`, `year_end_readiness_get` |
| Settings (M8) | `workspace_settings_get`, `workspace_settings_save`, `dashboard_tour_mark_complete` |
| Export | `sie_export_create`, `accountant_package_*`, `integration_status_get` |
| Ledger reads (M7) | `voucher_list`, `voucher_count`, `voucher_get`, `account_list`, `fiscal_period_list` |

## React frontend (`src/`)

### Pages (`src/pages/`)

| Page | Route | Primary commands |
|------|-------|----------------|
| `WorkspacePickerPage` | `/` | `workspace_open`, `workspace_create`, backup restore |
| `OnboardingPage` | `/onboarding` | profile saves, `compliance_check_run` |
| `DashboardPage` | `/dashboard` | checklist reads, tour, backup |
| `InvoicesPage` | `/invoices` | invoice CRUD, PDF preview, payment link |
| `LedgerPage` | `/ledger` | `voucher_list`, `account_list` |
| `DocumentsPage` | `/documents` | import, reconcile, expenses |
| `VatPage` | `/vat` | VAT return draft/approve/export |
| `YearEndPage` | `/year-end` | year-end package flow |
| `SettingsPage` | `/settings` | locale, simple mode, exports |

### Context (`src/context/`)

| Context | Role |
|---------|------|
| `WorkspaceContext` | Open workspace summary |
| `LocaleContext` | `en` / `sv` UI locale |
| `SimpleModeContext` | M8 progressive disclosure; hides ledger nav when on |
| `WorkspaceLocaleHydrator` | Sync locale from `workspace_settings` on open |

### Shared UI (`src/components/`)

| Component | Role |
|-----------|------|
| `AppSidebar` | Workbench nav; respects `simple_mode` via `workbenchNav` |
| `GuidedTour` | M8 first-run dashboard tour (focus trap, Escape) |
| `HelpTip` / `PageHelpHeader` | Contextual help popovers |
| `VoucherTraceLink` | Deep link to ledger voucher |

### Libraries (`src/lib/`)

| Module | Role |
|--------|------|
| `commands.ts` | All `invoke()` wrappers + `appErrorMessage` |
| `bindings.ts` | Generated TS types (from Specta) |
| `errorPresentation.ts` | UI presentation for `AppError` values |
| `dashboardChecklist.ts` | Ordered “next steps” checklist |
| `dashboardChecklistDetail.ts` | Per-item checklist presentation detail |
| `dashboardTour.ts` | Tour step definitions |
| `invoiceStatus.ts` | Paid/overdue display status (local calendar date) |
| `workbenchNav.ts` | Nav items filtered by simple mode |
| `helpTopics.ts` | Help topic keys per page |
| `money.ts` | SEK ↔ minor units |
| `dialogs.ts` | Native file/directory dialogs |
| `exportDirectory.ts` | Resolve an export directory from settings or a native picker |
| `workbenchSelection.ts` | Shared selection state helpers for workbench lists |

### i18n (`src/i18n/`)

- `sv.ts` — Swedish (default for new workspaces)
- `en.ts` — English
- `index.ts` — `t()`, `tVars()`, `Locale` type

## Module dependency rules

```
commands.rs → domain modules only (never UI)
domain modules → db, error, rules, audit (no commands)
renderer → commands.ts only (never sqlx / fs direct writes)
```

The UI may use Tauri dialog and scoped filesystem plugins for user-selected paths, but it never writes the workspace SQLite database directly.

## Tests co-located with modules

| Area | Test files |
|------|------------|
| Rust milestones | `src-tauri/tests/m*_milestone.rs`, `golden_scenarios.rs` |
| M8 settings/tour | `src-tauri/tests/m8_milestone.rs` |
| Frontend lib | `src/lib/*.test.ts` |
| Frontend pages | `src/pages/*.test.tsx` |
| M8 fixture integration | `src/test/m8_simple_ux.test.ts` |
