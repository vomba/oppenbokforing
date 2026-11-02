---
id: invoice-lifecycle
type: submodule-design
status: draft
title: Invoice issuance credits and historical identity
parent: rust-backend
depends-on: [accounting-ledger, tax-rules, business-profiles, evidence-archive, workspace-defaults]
references: [storage-contract]
---

# Invoice lifecycle and historical identity

## Responsibility

Own drafts, issuance, invoice numbering, compensating credits, and issue-time
provenance. Issuing an invoice is an accounting transition, not merely generating
a PDF; PDF production belongs to `invoice-pdf-jobs`.

## Boundary and invariants

Issuance rechecks tax/VAT posture, issue-year rules, fiscal locks, and replay state
under an immediate transaction. Number allocation, posting, issue snapshot,
PDF-job enqueue, and audit commit with the transition. A preflight is guidance,
not permission to skip revalidation at issue time.

Planning tax status cannot issue. VAT-exempt invoices cannot charge VAT; a sale
that crosses the exemption threshold is blocked for VAT-treatment review rather
than silently choosing a tax treatment. The UI's narrower draft capabilities
belong to `ui-workflows`, not to the backend's line-input contract.

Only drafts are editable through draft APIs. A source invoice can receive one full
credit, not an in-place ledger edit. Credit dates cannot precede issue but may use
a later open period/year; original VAT and historical identity are preserved.
Payment eligibility and settlement belong to `invoice-settlement`.

Issue-time identity, tax posture, and rule source are snapshotted so later profile
changes cannot rewrite the issued PDF's meaning. Legacy invoices without that
snapshot require explicit historical evidence/attestation recovery, not guessed
values from current profiles. Snapshot storage protection lives in `storage-contract`.

Observed limitation (acceptance unconfirmed): draft creation is not keyed, and
some draft/audit writes are separate; the transaction requirement remains
`local-sqlite`. Supported line inputs do not establish arbitrary account posting;
`accounting-ledger` owns that limitation.
