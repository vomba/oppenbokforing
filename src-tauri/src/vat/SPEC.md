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

## Reviewed snapshot identity

Each successful generation or regeneration stores a version-1 canonical snapshot,
its SHA-256 digest, and a fresh opaque review revision, even if all figures are
unchanged. The revision identifies one review, not merely a return or its totals.
The canonical representation uses fixed object fields, sorted map keys and ordered
row collections. Its version must change when review semantics change.

The snapshot binds:

- Workspace identity and active rule year; return identity and assigned rule
  version; period identity, workspace, fiscal year, key and bounds; fiscal-year
  identity, workspace and bounds.
- Every posted voucher in the inclusive accounting-date range, including those
  without lines: voucher/workspace/fiscal-year identity, status, source type and
  source ID, accounting date and effective accounting date. The latter uses the
  posted date only when accounting date is absent.
- Every associated journal line, including unmapped accounts: line, voucher and
  account IDs, debit, credit and nullable VAT code. Referenced accounts bind their
  identity, workspace, number, name, type and normal balance. Source additions or
  replacements therefore invalidate review even when totals are unchanged.
- All semantic business, tax and VAT profile fields: names, residence, SNI, tax
  status, expected profit/salary, active rule year, VAT status, frequency,
  accounting method, voluntary-registration date and filing-deadline regime.
  Missing profiles are represented explicitly. Profile row IDs and timestamps
  are not semantic review state.
- The active rule version for the period year, including identity, effective
  bounds, provenance, checksum and status; all its ordered rule IDs, families,
  keys and exact stored JSON values; workspace VAT-code configuration.
- Required user inputs, explicitly an empty map today, and the displayed stored
  box codes, integer amounts, query hashes and snapshot trace (rule provenance,
  year, revision, box amounts and source voucher IDs).

Profile/rule/configuration binding is deliberately conservative: changes outside
current box arithmetic can still require regeneration. This is not evidence of
rule-driven mappings or validated eligibility. Incidental timestamps, paths,
locale/settings, approval/lock state and generated box-row IDs are excluded.
Future guided inputs must enter the versioned identity before approval can use them.

## Approval, replay and trace

Approval requires the renderer's displayed `reviewedRevision`. An immediate SQLite
transaction checks that revision, snapshot version/digest, stored boxes and current
canonical source/configuration state before approving the stored figures. Approval
never replaces boxes with recalculated values. Status, checked period lock, approval
audit and replay binding commit together. Rejection leaves all of them unchanged.
Missing, unsupported or stale snapshots require explicit regeneration and fresh
review; a previous revision cannot approve the regenerated draft.

Every successful approval key is bound to workspace, return and reviewed revision,
including a fresh key presented for an already-approved return. Such a call records
only its replay binding, without another approval audit or history mutation.
Matching retries return the approved history even after configuration changes;
reuse for another return or revision is rejected.

Generation retries retain the same revision while it remains current. A superseded
request returns `vat_draft_superseded`, not a newer revision under the old key.
The renderer discards that definitively unusable key on the next explicit generation
attempt, but retains keys after uncertain transport failures.

Summary reads are transaction-coherent. Snapshotted traces retain reviewed figures
and source references rather than querying current ledger figures. The renderer
uses stored boxes and only attaches a trace matching the displayed revision.
`ui-workflows` owns confirmation and selection behavior.

Legacy drafts are not backfilled from unreviewed sources: they require regeneration.
Legacy approved returns remain readable and are not rewritten; their fallback trace
is a live reconstruction without a review revision, not a historical snapshot.

## Current boundary and gaps

Period shape follows the stored frequency. Returns cannot overlap, and profile
frequency cannot change once returns exist.

Posting guards reject dates in locked ranges. `rust-backend` records shared fiscal
helpers and revenue reads. There is no standalone VAT repository.

Export writes the approved preparation artifact; neither trace nor export is
evidence that a return has been submitted. Existing-return retrieval and UI revisit behavior are distinct;
`ui-workflows` owns the UI gap.

Aggregation recognizes accounts 3041/2611/2641 and reconstructs some bases from
rounded VAT. It maps VAT12/VAT6 bases to boxes 06/07 and residual base to 05.
These mappings lack independent tax-law validation.

Method-specific treatment, independent calculation validation, eligibility gating,
and guided amendments remain implementation gaps. Existing JSON exports are not proof of a
completed readable filing handoff.
