# M8 — Guided UX for non-accountants

## Scope

M8.1 and M8.2 shipped first; M8.3–M8.5 and `simple_mode` completed 2026-07-11.

## Principles

1. **Progressive disclosure by default** — show the next action, not infrastructure.
2. **Swedish-first** — new workspaces default to `sv` locale; English remains available in Settings.
3. **SEK in the UI** — users type kronor (e.g. `48 000,50`); renderer converts to minor units before `invoke`.
4. **Human-readable errors** — compliance and validation failures use i18n prose, never raw JSON.
5. **Calculation traces stay** — dashboard checklist links to work surfaces; rule year/source remain visible.

## M8.1 — Onboarding wizard

Four steps: Business → Tax → VAT → Review.

| Step | Fields | Help |
|------|--------|------|
| Business | name, owner, SNI | Short popover per field |
| Tax | F/FA-skatt, salary SEK, profit SEK | Explain FA-skatt vs salary |
| VAT | status, period, method | Explain momsbefriad threshold |
| Review | summary + compliance | Human compliance messages |

Sidebar step nav is functional (not inert links). Completing onboarding navigates to dashboard when compliance passes.

## M8.2 — Dashboard checklist

Replace milestone/infra panels with ordered **Nästa steg** items built from read commands:

- `complianceCheckRun` — profile review required
- `vatThresholdStatusGet` — approaching/breached
- `stagedTransactionsList` (status `staged`) — unmatched imports
- `invoiceOpenCount` — open invoices
- `yearEndReadinessGet` — year-end blockers
- `cashflowOverviewGet` — spendable cash summary metric (kept)

Technical paths (`databasePath`, `dataDir`) move to Settings only (advanced mode).

## M8.3 — First-run dashboard tour

After onboarding, the dashboard shows a lightweight guided tour (5 steps):

1. Checklist — nästa steg
2. Sidebar navigation
3. Spendable cash metric
4. Encrypted backup
5. Active rules panel

Persistence: `workspace_settings.dashboard_tour_completed` via `dashboard_tour_mark_complete` command.

Spec IDs: `M8.3-TOUR` (`src/lib/dashboardTour.ts`, `src/components/GuidedTour.tsx`)

## M8.4 — Workbench help rollout

`HelpTip` + `helpTopics` registry on ledger, documents, year-end, invoices, and VAT pages.

Spec IDs: `M8.4-HELP` (`src/lib/helpTopics.ts`)

## M8.5 — Polish + simple mode

- SQL error redaction in `src-tauri/src/error.rs`
- Branded dev icon script (`scripts/generate-dev-icons.sh`)
- Invoices, VAT, workspace picker, and year-end i18n
- Invoice status filters, PDF preview (`invoice_pdf_status`, `document_reveal`), payment/ledger trace links
- `workspace_settings.simple_mode` — default **on**; hides ledger nav + advanced settings panels

Spec IDs: `M8.5-SIMPLE` (`src/lib/workbenchNav.ts`, `src/context/SimpleModeContext.tsx`)

## Spec IDs (core)

| ID | Artifact |
|----|----------|
| M8.1 | This doc § Onboarding wizard |
| M8.2 | This doc § Dashboard checklist |
| M8.1-SEK | `parseSekToMinorUnits` in `src/lib/money.ts` |
| M8.1-ERR | `compliancePresentation.ts` |
| M8.2-CHK | `buildDashboardChecklist` in `src/lib/dashboardChecklist.ts` |

## Deferred

- Topic registry expansion beyond core workbench surfaces
- Apple Developer ID signing / notarized `.dmg` (see `docs/apple-signing-setup.md` — optional future)
