---
id: vat-returns
type: submodule-design
status: draft
title: VAT snapshots traces and period locks
parent: rust-backend
depends-on: [business-profiles, tax-rules, accounting-ledger, workspace-defaults]
references: [prepare-only-tax]
---

# VAT snapshots, traces, and period locks

## Responsibility

Build reviewable VAT-return snapshots from posted accounting dates, expose box-to-
voucher traces and threshold monitoring, and approve/lock periods. `prepare-only-tax`
owns the external-submission boundary; approval is a local accounting event.

## Boundary

Period shape follows the stored reporting frequency. Returns cannot overlap, and
profile frequency cannot change once returns exist. Drafts are not final numbers:
approval recomputes boxes from current source rows and atomically saves approved
values, replay result, audit, and the period lock. Posting guards reject dates in
locked ranges. Fiscal-year helpers and revenue reads are shared as described by
`rust-backend`; there is no standalone VAT repository.

Export writes the approved preparation artifact. Trace output binds boxes to
source vouchers and a query fingerprint; it is not evidence that a return has
been submitted. Existing-return retrieval and UI revisit behavior are distinct;
`ui-workflows` owns the UI gap.

Observed calculation limits (acceptance unconfirmed): aggregation recognizes
accounts 3041/2611/2641, reconstructs some taxable bases from rounded VAT, and
maps VAT12/VAT6 bases to boxes 06/07 with residual base in 05. These are current
engineering behavior, not independently validated Swedish tax-law mappings.
Do not infer complete transaction/VAT coverage from the supported rate codes.
