---
id: workspace-defaults
type: submodule-design
status: draft
title: Workspace bootstrap and fiscal guards
parent: rust-backend
depends-on: [vat-returns, workspace-settings]
references: [accounting-ledger, tax-rules, security-privacy]
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

Fiscal years are calendar years, and the seeded chart is small. Bootstrap does
not establish a complete BAS catalog or the reviewed opening-position support
required by `accounting-ledger`.

Creating defaults for today's UTC year does not establish tax support for that
year. `tax-rules` governs applicability and validated year extension.
