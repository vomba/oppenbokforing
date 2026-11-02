---
id: expense-posting
type: submodule-design
status: draft
title: Expense and input-VAT posting
parent: rust-backend
depends-on: [evidence-archive, vat-returns, workspace-defaults]
references: [storage-contract, product-goal, tax-rules, year-end-packages]
---

# Expense and input-VAT posting

## Responsibility

Post a business expense, applicable input VAT, and bank outflow while retaining
its explanation/evidence and optional bank-staging link. `accounting-ledger` owns
the distinction between business costs and owner transfers.

## Required expense and equipment contract

Guided inputs determine whether a purchase is an ordinary expense, business asset,
or unsupported case within `product-goal`. Account existence and a VAT rate alone
cannot establish deductible cost or recoverable input VAT.

Ordinary equipment needs recognition, depreciation, and closing treatment.
Shared phone, internet, and equipment costs require a justified business-use split
with retained rationale/evidence. Full business use must not be assumed.

Missing required facts or unsupported treatment blocks posting while evidence and
staging remain available. `tax-rules` governs validated treatment.
`year-end-packages` consumes the supported accounting and adjustment results.

## Current boundary and gaps

The caller supplies amount, expense/payment accounts, replay key, and evidence or
an explicit reason for its absence. An omitted date defaults to today's UTC date.

A linked staged row must be an unconsumed negative payment matching the gross expense.
The service verifies retained bytes, not an identifier alone.

The service writes its own voucher and lines, verifies balance, consumes optional
staging, and records audit within the posting transaction. It checks both fiscal
year and period locks. `storage-contract` owns database guarantees; parent
`rust-backend` owns cross-service edges.

Observed UI restriction, confirmed unfinished coverage: the documents workbench
requires a staged transaction even though this backend input is optional. The
interaction contract and broader UI limits live in `ui-workflows`.

Guided equipment, business-use allocation, and complete tax-posture eligibility
are implementation gaps. Existing account/rate validation does not satisfy the
required policy contract.
