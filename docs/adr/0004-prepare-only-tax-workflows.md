---
id: prepare-only-tax
type: architecture-design
title: Prepare Filing Outputs Without Direct Submission
status: active
parent: system-architecture
references: [tax-rules, product-goal]
---

# ADR 0004: Prepare Filing Outputs, Do Not Submit Directly in V1

Decision status: accepted in the source ADR. The user approved adoption into the spec graph.

## Context

Direct filing to Skatteverket requires technical access, signing, legal responsibility,
and a higher support burden. Users still need usable preparation outputs and clear guidance.

## Decision

The app prepares VAT drafts, year-end packages, NE values, PDFs, and export bundles.
It does not submit returns or initiate payments. The user completes external filing.

## Preparation states and approval

A provisional artifact preserves its draft status, coverage gaps, rule identity,
and source references in every exported copy. A draft export is allowed, but it
cannot imply completed preparation or submission readiness.

Completed preparation requires the calculation eligibility defined by `tax-rules`,
complete required user inputs, and resolution of relevant unsupported activity.
A warning or disclaimer cannot bypass these conditions.

Approval binds the reviewed source snapshot, inputs, rules, and figures. If relevant
sources change, approval must fail until regeneration and fresh review occur.
Silent approval of recomputed figures is forbidden.

Approved snapshots remain immutable. A guided correction or amendment preserves
the original approval and records the legally appropriate compensating action
and revised output. It does not unlock and overwrite history.

## Filing handoff and completion

The completed handoff provides readable box-by-box values, source explanations,
and remaining external actions. Machine-readable artifacts alone do not satisfy
`product-goal`'s self-service requirement.

External submission is a separate user attestation with date/reference and an
optional retained receipt. It is not authority verification. Export or local
approval must never automatically mark a task submitted.

Changes to a submission attestation retain its audit history. Professional review
can be recommended, but neither preparation nor recorded submission requires a
qualified-person sign-off.

## Current implementation gaps

`vat-returns` implements revision-bound canonical source/configuration snapshots
and approval of stored reviewed figures. Legacy VAT drafts require regeneration;
legacy approved history is preserved without claiming a canonical review snapshot.
Year-end aggregate ledger fingerprints are not equivalent to that source-record
identity and do not establish the same guarantee for equal-total source changes.

VAT/year-end artifacts are JSON preparation files. Their existence does not prove
validated coverage, a readable filing handoff, or authority-format compatibility.
Guided amendments and recorded external submission are not implemented.

## Consequences

- The product can launch without direct authority integration risk.
- All outputs must be traceable and reviewable.
- Export package quality is a core product requirement.
- Any change to the prepare-only boundary requires a separate compliance review.

