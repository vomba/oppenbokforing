---
id: year-end-packages
type: submodule-design
status: draft
title: Year-end preparation and artifact integrity
parent: rust-backend
depends-on: [tax-rules, workspace-defaults, vat-returns]
references: [prepare-only-tax]
---

# Year-end preparation and artifact integrity

## Responsibility

Create, regenerate, review, approve, and export local annual-account/NE draft
packages. Own readiness checks, source-ledger snapshots, artifact integrity, and
the fiscal-year closing transition; `prepare-only-tax` owns filing semantics.

## Boundary

Creation/regeneration captures ledger state and rule provenance. Approval requires
all configured VAT periods approved and an unchanged ledger fingerprint, then
atomically records approval, locks periods, and closes the year. A package missing
its fingerprint must be regenerated before approval; viewing a draft is not proof
it is current enough to approve.

Draft regeneration refreshes the source snapshot; incomplete draft artifacts can
also be repaired. Approved artifacts must pass integrity checks and are not
silently replaced from today's ledger. Legacy approved packages require semantic verification before hashes can
be adopted. The package and its artifact files are one review unit even though
SQLite and filesystem publication cannot be assumed to share a transaction.

Observed coverage limits (acceptance unconfirmed): generated NE fields are only
R1 from revenue and B14 from the computed business result, with source references;
this is not a complete or independently validated declaration mapping. Readiness
labels referring to VAT filing mean local approval, not authority submission.
The module aggregates shared journal/return tables directly; cross-service helper
calls are recorded in `rust-backend`.
