---
id: year-end-packages
type: submodule-design
status: active
title: Year-end preparation and artifact integrity
parent: rust-backend
depends-on: [tax-rules, workspace-defaults, vat-returns]
references: [prepare-only-tax, product-goal, accounting-ledger, expense-posting, business-profiles, tax-task-calendar]
---

# Year-end preparation and artifact integrity

## Responsibility

Create, regenerate, review, approve, and export local annual-account/NE draft
packages. Own readiness checks, source-ledger snapshots, artifact integrity, and
the fiscal-year closing transition; `prepare-only-tax` owns filing semantics.

## Required supported-case draft

The output must form a coherent annual-account/NE draft within `product-goal`'s
business and year coverage. Selected mapped fields alone do not satisfy this
contract. Unrelated personal-return sections remain outside scope.

Guided questions collect facts and adjustments absent from the ledger.
The draft retains answers, source references, and calculation traces.
Missing required facts cannot silently become zero.

Opening positions, unpaid items, ordinary equipment, business-use splits, and
method-specific closing treatment must contribute to the draft where applicable.
`accounting-ledger`, `expense-posting`, and `business-profiles` own those inputs.

`tax-rules` owns calculation eligibility. `prepare-only-tax` owns snapshot approval,
audited corrections, provisional exports, and the human-readable filing handoff.
Professional assistance is not a completion prerequisite.

## Current boundary and gaps

Creation/regeneration captures ledger state and rule provenance. Approval requires
all configured VAT periods approved and an unchanged ledger fingerprint, then
atomically records approval, locks periods, and closes the year. A package missing
its fingerprint must be regenerated before approval; viewing a draft is not proof
it is current enough to approve.

Draft regeneration refreshes the source snapshot and can repair incomplete artifacts.
Approved artifacts must pass integrity checks, not silent replacement from today's ledger.

Legacy approved packages require semantic verification before hash adoption.
The package and its files form one review unit. Current SQLite/filesystem publication
does not establish a shared transaction.

Current NE fields are only R1 from revenue and B14 from the computed business result.
This is incomplete, unvalidated mapping, not an accepted selected-field product.

Guided missing-fact inputs and complete supported-case treatment are absent.

Current readiness labels referring to VAT filing mean local approval.
They must not imply external submission. `tax-task-calendar` owns the required
completion projection.

The module queries shared journal/return tables directly. `rust-backend` records
its helper calls. These code boundaries do not prove the required draft is complete.
