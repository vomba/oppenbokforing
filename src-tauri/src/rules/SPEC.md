---
id: tax-rules
type: submodule-design
status: active
title: Versioned tax-rule lookup
parent: rust-backend
references: [product-goal, prepare-only-tax]
---

# Versioned tax-rule lookup

## Responsibility

Resolve persisted rule-version metadata and typed tax-rule values so domain
services can attach source/year provenance and fail closed when required
configuration is absent. Rule storage is local; this module does not fetch or
publish regulatory updates.

## Required rule applicability

`product-goal` declares the required accounting years and business cases.
Accounting-year rules and filing-calendar schedules have separate applicability.
A latest-active lookup must not substitute another year's rules.

Missing or invalid applicable rules block the affected rule-dependent action.
Historical records remain inspectable. Provisional calculation/export follows
`prepare-only-tax` and cannot imply supported completed preparation.

Provisional calculations cannot fill missing rules with another year or invented
values. Unknown required values remain unknown, not silently zero.

A new year requires authoritative material, a new version, and the same validation
gate. Published documents alone do not establish support. New versions must not
rewrite historical rules or silently change approved artifacts.

## Calculation validation gate

Eligibility is specific to the case, method, accounting year, filing calendar,
and rule version. It requires the following evidence:

- Authoritative sources with citations and recorded applicability.
- Expected results derived independently of the implementation under review.
- Reproducible checks of recognition, amounts, rounding, deadlines, and relevant boundaries.
- A recorded review of coverage, assumptions, and results.

A fixture derived only from implementation output is not an independent check.
Qualified professional review is optional, not a prerequisite. A source URL,
active status, or passing regression suite alone does not establish eligibility.

## Current boundary and gaps

This module supplies typed data access, not a universal policy engine. Each domain
owns its transition guards. `rust-backend` records sibling calls.

Latest-active and active-for-year APIs are distinct. Issuance uses issue-year
lookup. Optional typed lookups treat malformed data as absent, and required integer
lookup returns a validation error.

The repository seeds 2026 rules. It does not establish independently validated
coverage for the required years or the eligibility gate described here.
