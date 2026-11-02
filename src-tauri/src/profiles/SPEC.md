---
id: business-profiles
type: submodule-design
status: draft
title: Business tax and VAT profiles
parent: rust-backend
depends-on: [profile-compliance]
references: [local-sqlite, product-goal, tax-rules, security-privacy]
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

`cash_method` is accepted and stored, but posting/VAT services do not branch on it.
Method-transition, legacy-remediation, and full eligibility screening contracts
are implementation gaps, not accepted exceptions.
