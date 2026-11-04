---
id: invoice-lifecycle
type: submodule-design
status: active
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

Issuance rechecks tax/VAT posture, accounting method, issue-year rules, fiscal locks, and replay state
under an immediate transaction. Number allocation, posting, issue snapshot,
PDF-job enqueue, and audit commit with the transition. A preflight is guidance,
not permission to skip revalidation at issue time. Direct preflight and new
issuance/credit transitions enforce `business-profiles`' interim timing policy;
completed-result recovery follows that policy's preservation exceptions.

Planning tax status cannot issue. New standard invoices additionally require
`tax-rules`' bounded recognition case. Exempt, reduced/mixed-rate and other
uncovered cases now stop at eligibility, not a warning-only threshold permission.
Historical threshold calculations remain engineering regression coverage. The UI's
narrower draft capabilities belong to `ui-workflows`, not to the backend's line-input
contract.

Guided, initially unanswered facts identify the ordinary domestic service,
completion date, prior receipts, year-wide no-EU circumstances, registered annual
VAT, expected taxable base within SEK 1 million, and ordinary timely invoicing.
The last confirmation includes consistent invoice-date treatment for incoming and
outgoing invoices throughout the year, and invoice date as the owner's sending
date. These are retained user declarations, not independently observed facts.

Preflight and issuance share an evaluator over actual workspace-scoped profiles,
lines, amounts, year and reviewed rules. Checked multiplication/addition and stored
header comparisons reject saturation, overflow or inconsistent amounts. A context
fingerprint binds the reviewed facts/date, relevant profile and rule payloads,
line identities and evidence digest. It is a freshness check, not authorization:
issuance recomputes all predicates under its immediate transaction.

New issuance atomically saves immutable recognition provenance with the issue
snapshot, number, voucher, PDF job, audit and replay result. `storage-contract` owns
row protection. Completed replay/already-issued recovery precedes new-case checks;
legacy invoices are not backfilled as eligible. Workspace-scoped provenance reads
use retained facts and identity, not current profile substitutions. `ui-workflows`
owns fact collection, stale-review behavior and source inspection.

Only drafts are editable through draft APIs. A source can receive one full credit.
The credit date cannot precede issue but can use a later open period/year.
Original VAT and historical identity remain intact.

These guards do not establish cash-method recognition, partial settlement,
or guided opening/remediation support. The bounded recognition evidence also does
not establish all legal invoice/PDF particulars; seller VAT-number/address and
other document-completeness coverage remain separate gaps.

Issue-time identity, tax posture, and rule source are snapshotted so later profile
changes cannot rewrite the issued PDF's meaning. Legacy invoices without that
snapshot require explicit historical evidence/attestation recovery, not guessed
values from current profiles. Snapshot storage protection lives in `storage-contract`.

Draft creation is not keyed, and some draft/audit writes are separate.
The transaction requirement remains `local-sqlite`. `accounting-ledger` owns the
fixed-account posting limitation.
