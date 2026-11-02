---
id: accounting-ledger
type: submodule-design
status: draft
title: Posting primitives and ledger reads
parent: rust-backend
depends-on: [vat-returns]
references: [storage-contract, business-profiles, expense-posting, product-goal, prepare-only-tax, local-sqlite]
---

# Posting primitives and ledger reads

## Responsibility

Provide balanced invoice posting and compensating reversal primitives, plus
workspace-scoped voucher, account, and balance reads. Preserve source links so
workbenches and exports can trace posted amounts.

## Required accounting contract

Recognition and closing must use the selected method and applicable validated rules.
Cash-method invoices cannot silently receive invoice-method timing.

`business-profiles` owns reviewed method transitions and legacy remediation.

Established businesses need a reviewed opening position with balances, unpaid
items, provenance, and reconciliation checks. Opening items must not double-count
subsequent receipts, costs, or revenue.

Owner transfers are equity flows, not business expenses or salary. Equipment and
business-use allocations must preserve their source and year-end effects under
`expense-posting`. Required support is bounded by `product-goal`.

Corrections add traceable compensating records. `prepare-only-tax` owns corrections
to approved periods/years, which cannot become an unlock-and-overwrite shortcut.
`local-sqlite` retains the all-accounting-writes transaction requirement.

## Current boundary and gaps

Invoice posting primitives use the caller's transaction. Expense/payment services
write vouchers directly. This is not an exclusive ledger repository.
`rust-backend` owns the service graph.

`storage-contract` owns balance, row protection, and replay constraints.
Period guards use accounting dates.

Invoice posting uses fixed accounts 1510/3041/2611 rather than line-account metadata.
Method-specific posting, reviewed opening positions, and equipment/owner-flow
coverage are not established by those primitives.

`net_revenue_minor_for_fiscal_year` means net credits on revenue account 3041,
not profit after expenses. Consumers must not reinterpret it as net income.
