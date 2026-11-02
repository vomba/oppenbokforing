---
id: business-profiles
type: submodule-design
status: draft
title: Business tax and VAT profiles
parent: rust-backend
depends-on: [profile-compliance]
references: [local-sqlite]
---

# Business, tax, and VAT profiles

## Responsibility

Persist the workspace's business identity, F/FA-skatt posture, salary/business
assumptions, VAT registration, reporting frequency, accounting-method selection,
and filing-deadline regime. These are inputs to other services, not ledger entries.

## Boundary

Composite onboarding validates all three profiles, runs profile compliance, and
saves profiles plus audit events in one immediate transaction. Load failure must
not be interpreted as absence by its caller. Individual save APIs remain distinct;
the transaction requirement and observed gaps are owned by `local-sqlite` and
`security-privacy`.

FA-skatt requires positive salary assumptions. Registered VAT requires a deadline
regime compatible with reporting frequency. Frequency cannot change once any VAT
return exists, including a draft; the service serializes this check with its save.
Voluntary-registration date validation differs between composite and individual
save paths (observed gap, acceptance unconfirmed).

Observed limitation (acceptance unconfirmed): `cash_method` is accepted and stored,
but posting/VAT services do not branch on it. A selectable profile value is not
proof of cash-method accounting support. Rule-year defaults also do not establish
that valid rules exist for an arbitrary selected year.
