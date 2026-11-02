---
id: workspace-defaults
type: submodule-design
status: draft
title: Workspace bootstrap and fiscal guards
parent: rust-backend
depends-on: [vat-returns, workspace-settings]
---

# Workspace bootstrap and fiscal guards

## Responsibility

Make an opened workspace usable by ensuring accounting defaults, VAT-code seeds,
and presentation settings. Supply fiscal-year identity and open-year guards to
posting and preparation services. Physical create/open/close orchestration belongs
to the command facade in `rust-backend`, not this directory alone.

## Boundary

Bootstrap seeds six BAS accounts, calendar-year fiscal years, and yearly invoice
sequences. `ensure_workspace_ready` uses the current UTC year. Fiscal-year lookup
may create a missing year and its defaults; callers must not assume every helper
with a lookup-like name is read-only.

The module also supplies lexical path joins and explicit canonical containment
checks for workspace exports. A safe lexical join is not a substitute for canonical
containment; the broader security requirements live in `security-privacy`.
Cross-service calls and cycles are recorded by `rust-backend`.

Observed scope (product acceptance unconfirmed): fiscal years are calendar years
and the seeded chart is deliberately small in code. Neither configurable fiscal
calendars nor a complete BAS account catalog is established by this bootstrap.
