# Desktop UI Flow Specification

## Design Direction

Purpose: daily accounting and tax preparation for Swedish sole traders.  
Audience: solo freelancers who need to invoice, reconcile, prepare VAT, and close the year without an accountant-first interface.  
Tone: calm, guided, and review-focused — plain Swedish labels with optional field help; technical paths live in Settings, not the dashboard.  
Memorable detail: every calculated amount has a nearby trace affordance that shows source ledger lines and rule version.  
Constraint: first screen is the app workspace, not a landing page.

## Primary Navigation

- Dashboard
- Invoices
- Ledger
- Documents
- VAT
- Year end
- Settings

## Workspace Picker

States:

- No workspace exists.
- Recent workspaces exist.
- Workspace open failed.
- Backup restore available.

Required actions:

- Create workspace.
- Open workspace.
- Restore backup.
- Reveal workspace location.

## Dashboard

Primary content:

- **Nästa steg** action checklist (ordered by urgency).
- Spendable cash summary metric.
- Rule year / source trace (compliance panel).

Checklist sources: compliance status, VAT threshold, staged imports, open invoices, year-end readiness.

Required interactions:

- Jump to unresolved work from checklist links.
- Create backup (header).
- Open calculation trace from VAT/cash surfaces.

Technical workspace paths (`databasePath`, `dataDir`) are shown in Settings only.

## Onboarding

Four-step wizard: Business → Tax → VAT → Review.

- SEK decimal inputs in the UI (not minor units).
- Functional step navigation in sidebar.
- Human-readable compliance messages on review step.
- Default locale `sv` for new workspaces.

## Invoices

Views:

- Invoice list with status filters.
- Draft editor.
- Issued invoice detail.
- Credit-note flow.

Required states:

- Draft.
- Issued.
- Paid.
- Overdue.
- Credited.

Controls:

- Create draft.
- Issue invoice.
- Preview PDF.
- Mark paid.
- Credit invoice.
- Open ledger trace.

## Ledger

Views:

- Voucher list.
- Voucher detail.
- Account ledger.
- Period locks.

Rules:

- This read-only workbench lists posted vouchers and their journal lines.
- Posted vouchers cannot be edited.
- Corrections use reversal.
- Locked periods reject posting.

## Documents and Imports

Views:

- Evidence inbox.
- Staged transaction review.
- Matched-transaction history.
- Expense posting form with optional evidence selection.

Required interactions:

- Import document.
- Import CSV.
- Match a staged transaction to an invoice payment.
- Post a staged transaction as an expense, with evidence or an explicit reason.

## VAT Workbench

Views:

- Period selector.
- VAT return draft.
- VAT box trace.
- Threshold monitor.
- Export package.

Required interactions:

- Generate draft.
- Review source lines.
- Approve return.
- Export filing package.
- Lock period.

## Year-End Workbench

Views:

- Fiscal year selector.
- Closing checklist.
- Simplified annual accounts draft.
- `NE` draft.
- Ledger-source mapping for generated NE fields.

Required interactions:

- Generate package.
- Approve package.
- Export package.
- Lock fiscal year.

## Settings

Sections:

- Locale and simple mode.
- Workspace database and data-directory paths.
- Default export and backup directories.
- SIE and accountant-package export.
- Accountant-package validation.
- Integration availability (feature-flag stubs).

Required interactions:

- Switch Swedish/English locale.
- Enable or disable simple mode.
- Choose or clear default export and backup directories.
- Create SIE or accountant packages.
- Validate a selected accountant package without importing it.

