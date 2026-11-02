---
id: vat-returns
type: submodule-design
status: active
title: VAT snapshots traces and period locks
parent: rust-backend
depends-on: [business-profiles, tax-rules, accounting-ledger, workspace-defaults]
references: [prepare-only-tax, product-goal]
---

# VAT snapshots, traces, and period locks

## Responsibility

Build reviewable VAT-return snapshots from posted accounting dates, expose box-to-
voucher traces and threshold monitoring, and approve/lock periods. `prepare-only-tax`
owns the external-submission boundary; approval is a local accounting event.

## Required preparation contract

Returns cover only `product-goal`'s supported transactions and methods.
`tax-rules` determines calculation/year eligibility. Accepted rate codes alone
cannot establish supported VAT treatment.

`prepare-only-tax` owns provisional exports, required inputs, snapshot approval,
audited amendments, and external-submission attestations. Relevant source changes
require a new reviewed draft, not silent recomputation on approval.

Period locks protect approved history. Correction workflows preserve that history
and use the legally appropriate period/treatment. A locked mistake is not permission
to overwrite the original return.

## Current boundary and gaps

Period shape follows the stored frequency. Returns cannot overlap, and profile
frequency cannot change once returns exist.

Approval currently recomputes boxes from current source rows. It atomically saves
approved values, replay result, audit, and the period lock.

Posting guards reject dates in locked ranges. `rust-backend` records shared fiscal
helpers and revenue reads. There is no standalone VAT repository.

Export writes the approved preparation artifact. Trace output binds boxes to
source vouchers and a query fingerprint; it is not evidence that a return has
been submitted. Existing-return retrieval and UI revisit behavior are distinct;
`ui-workflows` owns the UI gap.

Aggregation recognizes accounts 3041/2611/2641 and reconstructs some bases from
rounded VAT. It maps VAT12/VAT6 bases to boxes 06/07 and residual base to 05.
These mappings lack independent tax-law validation.

Method-specific treatment, eligibility gating, stale-review rejection, and guided
amendments remain implementation gaps. Existing JSON exports are not proof of a
completed readable filing handoff.
