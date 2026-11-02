---
id: renderer
type: module-design
status: draft
title: Desktop renderer boundary
parent: system-architecture
references: [rust-command-boundary, ui-workflows, prepare-only-tax]
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

Workspace context holds an in-memory summary, not a second source of accounting
truth. Locale and simple-mode contexts hydrate workspace settings; pages own their
read/refresh state rather than a global accounting cache. Simple mode changes
visibility, not permission. Review-dialog mechanics are shared; each page owns the
facts shown and the command dispatched after confirmation.

The UI boundary is enforced by convention and command exposure, not a runtime DTO
validator. Observed limitation (rationale unconfirmed): not every asynchronous
read is protected against a stale workspace/selection response; see `ui-workflows`.

## Verification surface

Vitest covers presentation helpers and mocked-command component behavior. Fixture
schema checks and those mocks do not execute native accounting or prove desktop
end-to-end behavior; Rust integration tests own the posting contract.
