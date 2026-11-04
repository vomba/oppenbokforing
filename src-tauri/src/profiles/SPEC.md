---
id: business-profiles
type: submodule-design
status: active
title: Business tax and VAT profiles
parent: rust-backend
depends-on: [profile-compliance]
references: [local-sqlite, product-goal, tax-rules, security-privacy, invoice-lifecycle]
---

# Business, tax, and VAT profiles

## Responsibility

Persist the workspace's business identity, F/FA-skatt posture, salary/business
assumptions, VAT registration, reporting frequency, accounting-method selection,
and filing-deadline regime. These are inputs to other services, not ledger entries.

## Required profile contract

Setup determines eligibility for `product-goal`'s supported cases. An unresolved
answer does not authorize ordinary posting. Required facts must come from guided
answers, not silent defaults that imply tax eligibility.

Both accounting methods require their own recognition and closing behavior.
After the first posting, an ordinary settings change cannot change the method.
A dedicated, legally valid transition must preserve unpaid items and audit history.

Legacy cash-method books with invoice-method timing require reviewed remediation
before further affected posting. Inspection and evidence retention remain possible.
No transition can silently rewrite posted history.

`tax-rules` governs year/case eligibility. A stored method or rule year is not proof
of support. Guided review must not require a qualified professional.

## Current boundary and gaps

Composite onboarding validates all three profiles, runs compliance, and saves
profiles plus audit events in one immediate transaction. Load failures must not
be treated as absent profiles. Individual save APIs remain distinct.

FA-skatt requires positive salary assumptions. Registered VAT requires a deadline
regime compatible with frequency. Any existing VAT return prevents frequency
changes. The save serializes this check.

Voluntary-registration date validation differs between composite and individual
saves. The transaction requirement remains in `local-sqlite`.

### Interim accounting-method safety

`cash_method` remains selectable before posting. Both individual VAT-profile saves
and composite onboarding saves serialize a method freeze with their writes: after
any posted voucher, the stored method must remain unchanged. A missing method in
posted books also requires review, not ordinary profile replacement.

The interim timing policy permits only a stored `invoice_method` for new invoice
issuance, full credits, invoice settlements, and VAT/year-end approval. Missing
profiles fail closed. A rejection identifies `accountingMethod` and explains the
unsupported timing/remediation boundary rather than substituting another method.
The posting/approval transaction rechecks the policy; preflight is guidance only.

Completed-result replays and existing-result recovery remain available without
new posting. An already-issued retry may cache a new replay key. Historical reads,
PDF/evidence retention, imports/staging, drafts and historical exports retain their
existing safeguards. Expense posting and VAT/year-end draft creation/regeneration
are not newly gated by this policy; availability does not establish safe treatment
or complete preparation.

This guard is not cash-method recognition or closing support. Reviewed method
transitions, legacy remediation, and full eligibility screening remain gaps.
Allowing invoice-method timing does not satisfy `tax-rules`' validation gate;
`invoice-lifecycle` separately enforces its bounded recognition case on new issue.
