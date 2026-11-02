---
id: ui-workflows
type: module-design
title: Desktop UI workflows
status: active
parent: system-architecture
references: [rust-command-boundary, prepare-only-tax, product-goal, business-profiles, accounting-ledger, expense-posting, invoice-settlement, year-end-packages, tax-rules, system-architecture, bank-csv-staging]
---

# Desktop UI Flow Specification

The approved product contract lives in `product-goal`. This document separates
required interaction behavior from the observed page flows.

The user approved the revised text. Spec approval does not implement missing UI behavior.

## Required interaction contract

Setup screens for supported cases under `business-profiles`. Unsupported or
unresolved cases retain evidence but cannot enter ordinary posting or appear complete.

Historical timing problems and method changes require their reviewed workflows.

`accounting-ledger` owns reviewed opening positions. `expense-posting` owns guided
equipment and business-use allocation. `invoice-settlement` owns partial-payment
balances.

The UI must not hide these requirements behind narrower current forms.

Guided tax questions collect facts absent from the ledger. Unknown required values
cannot become zero. `year-end-packages` owns supported-case completeness.

`prepare-only-tax` owns provisional exports, reviewed snapshots, amendments,
box-by-box external-filing guidance, and user-reported submission. The UI must
keep those states distinct. Export never means submitted.

`tax-rules` owns calculation eligibility. A warning cannot enable completed
preparation without that evidence. Professional review remains optional under
`product-goal`.

`system-architecture` owns exclusive open, finish-before-switch, and preserved
export copies. Busy or failed operations must not appear silently cancelled
because the selected workspace changed.

`bank-csv-staging` owns named format compatibility. Unsupported formats or rows
cannot appear as silently successful imports.

## Observed UI record

The page sections record existing controls and specific unmet requirements.
They do not claim implementation of the required interaction contract.

## Design Direction

Purpose: daily accounting and tax preparation for Swedish sole traders.  
Audience: solo freelancers who need to invoice, reconcile, prepare VAT, and close the year without an accountant-first interface.  
Tone: calm, guided, and review-focused — plain Swedish labels with optional field help; technical paths live in Settings, not the dashboard.  
Traceability requirement: calculated amounts should expose their ledger/rule provenance. Current VAT traces, NE source references, and voucher links provide parts of this; the dashboard cash metric has no calculation-trace control.

Constraint: first screen is the app workspace, not a landing page.

## Primary Navigation

- Dashboard
- Invoices
- Ledger
- Documents
- VAT
- Year end
- Settings

### Simple mode

Task navigation is ordered as **Översikt** / Overview, **Fakturor** / Invoices, **Kvitton och utgifter** / Receipts & expenses, **Redovisa moms** / Report VAT, **Avsluta året** / Close the year, and **Inställningar** / Settings. It uses `/dashboard`, `/invoices`, `/documents`, `/vat`, `/year-end`, and `/settings`. Full mode retains the workbench navigation above. Ledger is not listed in simple mode, but remains reachable by its direct `/ledger` route.

### Material-action review

Issue invoice, credit invoice, payment/reconciliation, expense posting, VAT approval, and year-end approval require a localized review panel before their existing command. The panel explains user-visible facts, permanent consequence, a correction path where one exists, Cancel, and one confirm action. Cancel makes no mutation invoke. A backend validation or lock failure closes the panel and leaves its existing localized error visible.

## Workspace Picker

States:

- No workspace exists.
- Recent workspaces exist.
- Workspace open failed.
- Backup restore available.

Required actions:

- Create workspace.
- Open a workspace from the recent list.
- Restore encrypted backup with explicit confirmation, then open the restored workspace.

Unmet flow requirements: arbitrary database selection and a reveal-workspace action.
The recent list currently displays database paths.

## Dashboard

### Tax tasks

Below the unchanged checklist, a **Skatteuppgifter** / Tax tasks section lists backend-returned tax work ordered by status and due date. Each card gives the next plain-language action, covered period, status/date, a route to VAT, year-end, or onboarding, and an optional official rule source link. The app may prepare or export a package but must say that submission to Skatteverket happens outside the app. Calculations remain in the existing trace/help affordances; React never determines due dates or filing requirements.

Primary content:

- **Nästa steg** action checklist (ordered by urgency).
- Spendable cash summary metric.
- Rule year / source trace (compliance panel).

Checklist sources: compliance status, VAT threshold, staged imports, open invoices, year-end readiness.

Required interactions:

- Jump to unresolved work from checklist links.
- Create backup (header).
- Open calculation trace from the VAT surface; a cash trace remains unmet.

If a required dashboard read fails, show unavailable/retry instead of claiming the
workspace is caught up. Task dates and tax requirements come from Rust.

The dashboard does not show technical workspace paths (`databasePath`, `dataDir`).
Settings shows them; the workspace picker also shows recent database paths.

## Onboarding

Four-step wizard: Business → Tax → VAT → Review.

- SEK decimal inputs in the UI (not minor units).
- Functional step navigation in sidebar.
- Human-readable compliance messages on review step.
- Default locale `sv` for new workspaces.
- Save the three profiles through the composite onboarding command.
- A profile load failure must not appear as an empty profile that saving can overwrite.

## Invoices

Views:

- Invoice list with status filters.
- Single-line draft creation (quantity 1, account 3041, no due date).
- Issued invoice detail.
- Credit-note flow.

The broader draft-editor requirement is unmet: Rust exposes draft update, but the
UI does not call it. List filters are all/draft/issued; paid, overdue, credited, and
credit-note labels are derived display states, not separate list filters.

Required states:

- Draft.
- Issued.
- Paid.
- Overdue.
- Credited.

Controls:

- Create draft.
- Issue invoice after backend preflight; a VAT-treatment review blocks issue and directs setup correction.
- Preview/refresh PDF from historical issue data. Legacy invoices without snapshots require preserved evidence and explicit historical-value attestation before recovery.
- Mark paid.
- Credit invoice.
- Open ledger trace.

## Ledger

Views:

- Voucher list.
- Voucher detail.
- Account balances.
- Period lock status.

A per-account transaction workbench is not implemented. Lists show the first 100
posted vouchers without pagination.

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
- Match a staged transaction to an invoice payment (currently exact full payment only).
- Post a staged transaction as an expense, with evidence or an explicit reason.
- Record a full invoice payment without CSV by selecting retained PDF bank evidence
  and a payment date through the invoice-linked payment panel.

The UI requires staging for expenses even though the backend accepts an optional
staged transaction. Evidence/staged lists are capped at 100 and matched history at
20, without pagination.

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
- Approval locks the period; there is no separate lock command/button.

Changing the selected period clears the displayed return and trace.
Revisiting a period does not reload its existing return. This remains unfinished
UI coverage, separate from the new eligibility and stale-review requirements.

## Year-End Workbench

Views:

- Fiscal year selector.
- Closing checklist.
- Simplified annual accounts draft.
- `NE` draft.
- Ledger-source mapping for generated NE fields.

Required interactions:

- Generate or reload an existing package for the fiscal year.
- Regenerate a draft through review when its source snapshot is stale.
- Approve a ready package.
- Export package.
- Approval locks the fiscal year and its periods; there is no separate lock action.

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

Default directories, SIE/accountant exports, package validation, and integration
panels are hidden by simple mode; switching to full mode reveals them. This is
progressive disclosure, not an authorization boundary.

Observed limitations (product rationale unconfirmed): several async page refreshes
have no stale-response guard; frontend retry keys survive some failures but not
component remounts. Currency formatting remains Swedish even in English, some
validation text passes through from Rust, and page year/date defaults differ.
These are not inferred product guarantees.

