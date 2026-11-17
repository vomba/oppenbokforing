---
id: accounting-ledger
type: submodule-design
status: active
title: Posting primitives and ledger reads
parent: rust-backend
depends-on: [vat-returns, business-profiles, invoice-lifecycle]
references: [storage-contract, expense-posting, product-goal, prepare-only-tax, local-sqlite]
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

Invoice posting and reversal primitives check `business-profiles`' interim timing
policy in the caller's transaction before creating vouchers or lines. Their
standalone wrappers use immediate transactions. Invoice posting also delegates to
`invoice-lifecycle`'s case evaluator: immutable pending issuance provenance must
match the actual unposted invoice, workspace, date/year and VAT buckets. Raw
buckets or a completed record cannot authorize another posting. These invoice primitives do not
extend recognition coverage to reversal, expense or payment workflows. The separate
bounded cash service in `invoice-lifecycle` owns its full-receipt authorization
and direct 1930/3041/2611 posting; it never calls the invoice primitive or clears 1510.
Expense/payment services write
vouchers directly and need their own transition guards. This is not an exclusive
ledger repository. `rust-backend` owns the service graph.

`storage-contract` owns balance, row protection, and replay constraints.
Period guards use accounting dates.

Invoice posting uses fixed accounts 1510/3041/2611 rather than line-account metadata.
Method-specific posting, reviewed opening positions, and equipment/owner-flow
coverage are not established by those primitives.

`net_revenue_minor_for_fiscal_year` means net credits on revenue account 3041,
not profit after expenses. Consumers must not reinterpret it as net income.
