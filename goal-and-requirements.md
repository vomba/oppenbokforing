---
id: product-goal
type: goal-and-requirements
status: draft
title: ÖppenBokföring goals and requirements
tags: [product]
references: [desktop-platform, local-sqlite, rust-command-boundary, prepare-only-tax, retained-evidence, cash-planning, ui-workflows, security-privacy, tax-rules]
---

# ÖppenBokföring

## Goal and audience

Help Swedish enskild firma owners keep local books and report business taxes
without mandatory professional assistance. The audience is solo service businesses,
including owners with employment income under FA-skatt.

The app is independent open-source software, not a government service or professional
tax advice. Professional review is an optional recommendation, not a completion requirement.

The user approved this product contract through decision review. All nodes remain
draft for review of the revised text. Required capabilities are not claims that
the current implementation satisfies them.

## Supported business cases

- Simple solo domestic service businesses, without employees or trading inventory.
- Ordinary taxable services and low-turnover VAT exemption, not sector-specific exemptions.
- Both cash-method and invoice-method accounting, with method-specific recognition and closing treatment.
- Ordinary equipment, depreciation, and justified business-use splits for shared phone, internet, and equipment costs.
- New businesses and established businesses with reviewed opening balances and unpaid invoices.
- Accounting years 2025 and 2026, with their applicable filing calendars. `tax-rules` governs validated support and new-year extension.
- SEK invoices, full or partial payments, and full credits that do not require an unsupported refund.

## Required capabilities

- Guided business/tax/VAT setup, Swedish-first and English UI, and review before material accounting actions.
- Double-entry books, retained invoice identity, protected posted history, and source traces.
- Retained evidence, validated bank CSV staging, reconciliation, and guided expense/asset treatment.
- VAT preparation and complete annual-account/NE drafts for supported cases, including guided answers for facts absent from the ledger.
- Human-readable, box-by-box external-filing guidance, source references, and remaining user actions. Structured files supplement this handoff.
- Explicit provisional exports, validated approval, audited corrections, and user-reported external submission under `prepare-only-tax`.
- Reviewed opening positions, accounting-method transitions, and remediation of affected legacy books without silent historical rewrites.
- Verified encrypted backup/restore, exclusive workspace ownership, and preserved export copies under the owning architecture and module contracts.

The cash projection remains a **provisional approximation**, not a tax calculation
or bank-funds guarantee. Its formula belongs to `cash-planning`.

## Explicit exclusions

Cross-border trade, sector-specific VAT exemptions, complex assets, vehicle/property-specific
cases, employees, and inventory are outside the supported set.

Partial credits, overpayments/refunds, and foreign-currency invoices are also outside
scope. An operation that requires an excluded case must stop, not disguise it as a
supported invoice, expense, or credit.

Unsupported or unresolved transactions retain their evidence/staging but cannot
enter the ordinary posting workflow. Relevant unresolved activity blocks a claim
that the affected books or tax preparation are complete.

The product covers business accounts and the NE annex, not the entire personal
income-tax return. Salary assumptions do not imply support for unrelated personal
or capital-income sections.

Direct filing and payment initiation are excluded by `prepare-only-tax`. Cloud
sync, hosted collaboration, and hosted OCR are outside the local-first scope.
Future scope expansion requires an explicit contract change, not a roadmap here.

## Implementation honesty

`tax-rules` owns the evidence required for calculation eligibility. Golden fixtures
alone are not independent tax validation. Disclaimers do not replace that gate.

Module specs separate required behavior from observed implementation gaps.
`ui-workflows` owns interaction gaps, and `security-privacy` owns known control gaps.
This documentation change implements none of the missing capabilities.
