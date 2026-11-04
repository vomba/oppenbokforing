---
id: renderer
type: module-design
status: active
title: Desktop renderer boundary
parent: system-architecture
references: [rust-command-boundary, ui-workflows, prepare-only-tax, invoice-lifecycle, tax-rules]
depends-on: [rust-backend]
---

# Desktop renderer

## Responsibility

Present guided bookkeeping workbenches and translate user intent into typed
command calls. `ui-workflows` owns the interaction contract and current UI gaps;
this module owns presentation, routing, selection, and in-memory workflow state,
not accounting decisions.

## Boundary

Pages and contexts call the handwritten `lib/commands.ts` facade. It invokes
registered Rust commands and unwraps `{ data: T }`; `lib/bindings.ts` supplies
Specta-generated DTOs and must not become a separately maintained domain model.
Native dialogs select paths. No renderer code writes SQLite or accounting files.

Shared helpers may parse SEK into integer minor units, format dates/statuses,
order the next-action checklist, and map backend task targets to routes. They do
not determine tax deadlines, VAT treatment, posting eligibility, or filing status.
Calculation and mutation authority remains across `rust-command-boundary`.

Workspace ownership, operation completion, and cross-switch result identity must
obey `system-architecture`. Renderer visibility alone does not enforce those rules.

Workspace context holds an in-memory summary, not a second source of accounting
truth. Locale and simple-mode contexts hydrate workspace settings; pages own their
read/refresh state rather than a global accounting cache. Simple mode changes
visibility, not permission. Review-dialog mechanics are shared; each page owns the
facts shown and the command dispatched after confirmation.

The UI boundary uses convention and command exposure, not a runtime DTO validator.

Not every asynchronous read guards against stale workspace/selection responses.
This remains a gap against `ui-workflows`, not a guarantee established by context providers.

## Bounded new-invoice review

The bounded `invoice-lifecycle` recognition gate uses a local composed dialog,
not a change to the generic material-action review or credit workflow. New
standard issuance collects explicit, initially unanswered transaction and
2026-wide annual/no-EU/registration/expected-taxable-base/timely-invoicing facts,
completion date, and an explicit invoice/sending date before backend preflight.
Unknown or contradictory answers cannot reach final issuance. The supported
case is only 2026 invoice-method ordinary completed domestic SEK services at
25%, no previous payments, annual VAT/no-EU under `annual_may_12`, with exact
whole-öre VAT on every line. Rust remains the eligibility authority.

Final review uses the fingerprint-bound invoice snapshot returned by Rust, not
cached list amounts: customer, due date, lines and totals are shown alongside the
same facts, completion date, narrow coverage, backend validation/digest/rule identity
and returned deadline. A missing or unrelated snapshot cannot reach confirmation.
Confirmation
submits exactly those facts, date and review fingerprint. The owner sends the
invoice on that date; the app does not email it. Editing facts/date discards the
preflight, and workspace/invoice changes discard the review. Failed preflight
and freshness rejection keep the draft and answers available in the facts
step, rather than closing this slice's dialog. No support boolean is submitted.
Cash-method guards and existing credit/PDF/recovery behavior remain unchanged.

Issued history can read immutable recognition provenance through
`invoice_issue_validation_get`; absence is displayed without guessing current
facts or invoking any mutation. This is not approval of historical invoices,
full legal PDF compliance, books, VAT returns or year-end accounts. Bundled source
citations are inspectable only for their matching validation identity; unknown
retained identities do not inherit another case's references. Source clicks prevent
webview navigation and invoke the native ID-only opener; failures remain visible
without accounting mutation. `tax-rules` owns the
evidence and verification record; UI checks alone do not establish eligibility.

## Verification surface

Vitest covers presentation helpers and mocked-command component behavior. Fixture
schema checks and those mocks do not execute native accounting or prove desktop
end-to-end behavior; Rust integration tests own the posting contract.
