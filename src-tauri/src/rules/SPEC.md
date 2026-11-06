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

The repository seeds engineering rules. It does not establish validated coverage
for all required years or the eligibility gate described here.

### Bounded method-recognition coverage

The [2026 exact-VAT domestic-service dossier](../../../docs/accounting-validation/2026-domestic-service25-exact.md)
records authoritative passages, independently derived expectations, AI coverage
review and execution-verification status. It is not a professional or authority
attestation. Its companion manifest pins the source references and expected cases.

The invoice-owned gate covers only new 2026 invoice-method recognition of ordinary
completed domestic SEK services at 25%, without prior receipts, under annual VAT
without EU trade (`annual_may_12`). Each line must have exact VAT in integer öre;
fractional-öre rounding remains uncovered, even when fractions cancel in the total.
The applicable accounting period is 2026 and the filing date is 2027-05-12. Explicit
annual-registration, expected taxable-base and year-wide no-EU facts are required;
profit, residency defaults and stored regime selections do not establish them.

The separate [cash recognition dossier](../../../docs/accounting-validation/2026-cash-pdf-full.md)
records independently derived zero-at-issue and full-receipt expectations, first-year
and civil-date applicability, source review and execution-verification status. Its
cash case covers only new businesses started in 2026 with a first year ending
December 31, explicit zero-opening/outside-history review, and one full same-year
receipt backed by retained PDF and reviewed actual facts. Short first years require
review of adjusted normal annual net turnover. The SEK 3 million cash-method test
is distinct from the SEK 1 million tax-year VAT-base test. Neither a small workspace
balance nor an annual-registration setting proves these facts.

Migration 22 installs the sole active 2026 version `rv-2026-method-recognition-v2`.
Live invoice evidence is `invoice-recognition-se-service25-2026-exact-v2`, with
unchanged inherited expectations and explicit lineage to published invoice-v1;
cash evidence is `cash-recognition-se-service25-2026-pdf-full-v1`. The immutable
compiled registry resolves each case separately. The gate requires the exact
manifest digest, active applicability, actual case-policy payload and annual
schedule; a descriptive checksum alone cannot authorize a transition.

Published invoice-v1 bytes, its retired rule payloads, seven-field facts, historical
context/fingerprint reader and source URLs remain intact. Version retirement and
copied unrelated engineering values do not rewrite history, backfill eligibility
or validate those engineering calculations. Historical reads select the recorded
case rather than require it to remain active. `invoice-lifecycle` owns transactional
case enforcement and retained declarations. Cash CSV recognition, credits, partials,
over/refunds, unpaid closing and all cash VAT/year-end approvals remain outside
this coverage. Complete legal PDFs, books and returns are not established; full
required-year/case eligibility remains incomplete.
