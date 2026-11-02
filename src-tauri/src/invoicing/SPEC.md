---
id: invoice-lifecycle
type: submodule-design
status: draft
title: Invoice issuance credits and historical identity
parent: rust-backend
depends-on: [accounting-ledger, tax-rules, business-profiles, evidence-archive, workspace-defaults]
references: [storage-contract, product-goal, invoice-settlement, local-sqlite]
---

# Invoice lifecycle and historical identity

## Responsibility

Own drafts, issuance, numbering, compensating credits, and issue-time provenance.
Issuance and accounting recognition must remain distinct for method-specific
treatment. `invoice-pdf-jobs` owns PDF production.

## Required invoice contract

`product-goal` owns supported currency and payment/credit cases. `tax-rules` governs
applicable calculation eligibility. Issuance cannot silently choose another method,
year, or unsupported VAT treatment.

`invoice-settlement` owns partial payments and outstanding balances.
Full credits preserve the source invoice and cannot disguise a partial credit or refund.

A credit requiring an excluded refund must stop with its evidence/history intact.

Method changes, legacy timing problems, and reviewed opening invoices follow
`business-profiles` and `accounting-ledger`. A PDF is not proof of correct
recognition or safe completion.

## Current boundary and gaps

Issuance rechecks tax/VAT posture, issue-year rules, fiscal locks, and replay state
under an immediate transaction. Number allocation, posting, issue snapshot,
PDF-job enqueue, and audit commit with the transition. A preflight is guidance,
not permission to skip revalidation at issue time.

Planning tax status cannot issue. VAT-exempt invoices cannot charge VAT; a sale
that crosses the exemption threshold is blocked for VAT-treatment review rather
than silently choosing a tax treatment. The UI's narrower draft capabilities
belong to `ui-workflows`, not to the backend's line-input contract.

Only drafts are editable through draft APIs. A source can receive one full credit.
The credit date cannot precede issue but can use a later open period/year.
Original VAT and historical identity remain intact.

These guards do not establish method-specific recognition, partial settlement,
or guided opening/remediation support.

Issue-time identity, tax posture, and rule source are snapshotted so later profile
changes cannot rewrite the issued PDF's meaning. Legacy invoices without that
snapshot require explicit historical evidence/attestation recovery, not guessed
values from current profiles. Snapshot storage protection lives in `storage-contract`.

Draft creation is not keyed, and some draft/audit writes are separate.
The transaction requirement remains `local-sqlite`. `accounting-ledger` owns the
fixed-account posting limitation.
