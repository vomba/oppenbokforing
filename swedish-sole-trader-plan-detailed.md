# Swedish Sole-Trader Taxes and Accounting Desktop App - Detailed Plan

## Summary

This service is a greenfield Swedish finance and tax desktop app for `enskild firma`. It covers invoicing, bookkeeping, VAT, cash planning, and year-end tax preparation, with explicit support for the common case where the business owner also has salary income and therefore needs `FA-skatt`.

The implementation should be local-first, rule-versioned, auditable, and export-oriented. The app should keep the primary accounting record on the user's machine, generate filing-ready outputs, and preserve a clean path toward later optional integrations with banks, accountants, and Skatteverket.

## Product Boundaries

In scope for v1:

- Desktop workspace setup for one Swedish-resident sole trader.
- Sole trader setup and tax-profile capture.
- Invoicing with VAT, no-VAT, and `F-skatt` wording.
- Bookkeeping with immutable local voucher history.
- Local receipt and invoice evidence archive.
- Cashflow and tax reserve estimates.
- VAT tracking and draft return preparation.
- Year-end support for simplified annual accounts and `NE-bilaga`.
- Local backup and filing package export.

Out of scope for v1:

- Employee payroll.
- Limited companies or partnerships.
- Direct Skatteverket submission.
- Bank feed integrations.
- Multi-device cloud sync.
- Hosted accountant portal.
- Advanced cross-border VAT and OSS flows unless later added behind feature flags.

## Regulatory Model

The product should encode tax and accounting rules as versioned data, not UI assumptions.

Key rule families:

- `F-skatt` and `FA-skatt` behavior for sole traders.
- Preliminary tax estimates based on total expected income.
- VAT registration threshold and voluntary VAT registration.
- VAT-exempt workflows for low-turnover businesses.
- Annual income tax filing for sole traders using `INK1` plus `NE`.
- Accounting retention, locking, and audit traceability.

The UI should surface the rule outcome, but the source of truth should be a Rust rule engine with tax-year metadata and source references stored in SQLite.

## Desktop Architecture

Use Tauri v2 as the desktop shell, React/Vite TypeScript for the renderer, and Rust for command handlers, domain services, local persistence, file IO, and packaging-sensitive behavior.

### Runtime modules

- `app-shell`: Tauri configuration, windows, permissions, menus, updater, app paths.
- `renderer`: React desktop UI, routing, forms, tables, review screens.
- `commands`: narrow Tauri command interface exposed to the renderer.
- `workspace`: local workspace lifecycle, database location, backups, restore.
- `compliance-rules`: versioned rule sets, thresholds, source citations, validation checks.
- `invoicing`: customers, invoice sequences, invoices, invoice lines, credit notes, PDFs.
- `ledger`: accounts, vouchers, journal entries, journal lines, posting, reversals, locks, SIE export.
- `documents`: local receipt and invoice evidence, metadata, hashes, retention deadlines.
- `imports`: CSV upload, transaction staging, import validation.
- `cashflow`: tax reservations, VAT reservations, owner withdrawals, due-date forecast.
- `vat-engine`: VAT code mapping, VAT return drafts, zero returns, threshold monitoring.
- `year-end-workbench`: simplified annual accounts, NE draft, review checklist, export package.
- `audit-and-retention`: audit events, calculation traces, local retention and backup checks.

### Core entities

- `Workspace`
- `BusinessProfile`
- `TaxProfile`
- `VatProfile`
- `FiscalYear`
- `FiscalPeriod`
- `RuleSet`
- `Invoice`
- `LedgerEntry`
- `Voucher`
- `EvidenceRecord`
- `VatReturnDraft`
- `AnnualAccountsDraft`
- `NEDraft`
- `BackupManifest`

### Key invariants

- Ledger entries are immutable after posting.
- Corrections happen through reversal entries.
- Closed periods cannot be mutated.
- Every calculated tax amount must be traceable to source data and rule version.
- Source documents must be retained for seven years.
- The renderer cannot write directly to accounting tables; all accounting mutations go through Rust commands.
- Local backups must include the SQLite database, evidence files, export files, rule versions, and a manifest hash.

## User Flows

### Onboarding

- Create or open a local workspace.
- Capture business type, residency, accounting year, and expected turnover.
- Capture whether the owner also has salary income.
- Select `F-skatt`, `FA-skatt`, or planning state.
- Select VAT status:
  - VAT-registered.
  - VAT-exempt under the low-turnover threshold.
  - Voluntary VAT registration.

### Invoicing

- Issue invoices with unique numbering.
- Add `Godkänd för F-skatt` where required.
- Include VAT or VAT-exemption text based on profile.
- Generate and archive invoice PDFs locally.
- Support credit invoices for corrections.

### Bookkeeping

- Create ledger entries from invoices, expenses, CSV imports, and manual journals.
- Attach local evidence files to vouchers.
- Support manual review and reconciliation.
- Show tax reserves separately from spendable cash.

### VAT

- Compute VAT by period from local ledger data.
- Support zero returns for VAT-registered users.
- Trigger alerts when turnover approaches or crosses the exemption threshold.
- Switch behavior when the threshold is exceeded.

### Year End

- Prepare simplified annual accounts for K1 users.
- Build the `NE` appendix draft from the ledger and adjustments.
- Surface unresolved items before export.
- Export a filing package and evidence bundle to a user-selected location.

## Implementation Phases

### Phase 0 - Desktop Foundation

- Scaffold Tauri v2, React/Vite, TypeScript, Rust, SQLite, and local app-data paths.
- Define command response and error conventions.
- Add migrations, seed rules, local document directory setup, and logging.
- Build packaged-app smoke test for macOS first, then Windows and Linux.

### Phase 1 - Workspace, Rules, and Onboarding

- Build local workspace create/open/backup/restore.
- Finalize domain model and rule registry.
- Build profile capture.
- Build golden test corpus of Swedish tax scenarios.
- Define source citation requirements for every rule.

### Phase 2 - Invoicing and Ledger

- Build customer records, invoice creation, PDF generation, and credit notes.
- Build voucher posting, balancing, reversals, and period locks.
- Ensure every issued invoice posts into the ledger transactionally.

### Phase 3 - Documents, Imports, and Cash Planning

- Build local evidence archive.
- Build CSV import and staged transaction review.
- Build manual journals, reconciliation, and cash reserve views.

### Phase 4 - VAT and Threshold Logic

- Build VAT classification and return drafts.
- Support VAT-exempt and VAT-registered flows.
- Add zero returns, threshold alerts, and state transitions.

### Phase 5 - Year-End Workbench

- Build simplified annual accounts.
- Build `NE` draft generation.
- Build final review, export package, and evidence manifest.

### Phase 6 - Integrations

- Add SIE export.
- Add accountant export/import package.
- Add bank feeds only after local permission, consent, and provider boundaries are designed.
- Add direct filing only after legal, signing, and technical access are resolved.

## Acceptance Scenarios

- The app installs, launches, creates a workspace, closes, and reopens the same local data offline.
- A sole trader with salary income and business income gets `FA-skatt` guidance and separate tax planning.
- A VAT-exempt sole trader can invoice without VAT and is warned before threshold breach.
- A VAT-registered user can generate a zero VAT return for an empty period.
- A user can close the year, review annual accounts, and export `NE` draft data.
- A corrected invoice preserves the audit chain through reversal entries.
- A backup can be created and restored into a fresh local app install.

## Assumptions

- The first target is a Swedish-resident solo freelancer.
- The product is advisory and preparatory in v1, not a direct filing tool.
- Data is local-first and single-device in v1.
- Banking is manual/CSV first.
- K1 simplified annual accounts are the first accounting mode.
- Advanced VAT or cross-border rules are later-phase additions.

## Source Material

- Tauri v2 architecture documentation: https://v2.tauri.app/concept/architecture/
- Tauri v2 SQL plugin documentation: https://v2.tauri.app/plugin/sql/
- Tauri v2 file-system plugin documentation: https://v2.tauri.app/plugin/file-system/
- Skatteverket F-tax / FA-tax guidance: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/registeringabusiness/approvalforftax.4.676f4884175c97df4192308.html
- Skatteverket VAT registration guidance: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/registeringabusiness/registeryourbusinessforvat.4.6e1dd38d196873bc1e1376.html
- Skatteverket VAT exemption guidance: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/registeringabusiness/incertaincasesyoudonotneedtoregisteryourbusinessforvat.4.6e1dd38d196873bc1e1cff.html
- Skatteverket income tax for sole traders: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/declaringtaxesbusinesses/incometax/incometaxreturnsforsoletraders.4.676f4884175c97df41913f3.html
- Skatteverket VAT declarations: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/declaringtaxesbusinesses/vatdeclarations.4.12815e4f14a62bc048f52be.html
- Skatteverket paying taxes: https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/payingtaxesbusinesses.4.12815e4f14a62bc048f5395.html
- Bokföringsnämnden enskilda näringsidkare: https://www.bfn.se/redovisningsregler/vad-galler-for/enskilda-naringsidkare/
