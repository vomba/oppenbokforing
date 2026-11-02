---
id: expense-posting
type: submodule-design
status: draft
title: Expense and input-VAT posting
parent: rust-backend
depends-on: [evidence-archive, vat-returns, workspace-defaults]
references: [storage-contract]
---

# Expense and input-VAT posting

## Responsibility

Post a business expense, applicable input VAT, and bank outflow while retaining
its explanation/evidence and optional bank-staging link. Owner withdrawals are
not business expenses; this service must not classify them as costs.

## Boundary

The caller supplies amount, expense/payment accounts, replay key, and evidence or
an explicit reason for its absence. An omitted date defaults to today's UTC date. A linked staged row must be an unconsumed
negative payment exactly matching the gross expense. Evidence is verified from
retained bytes before posting, not accepted solely by identifier.

The service writes its own voucher and lines, verifies balance, consumes optional
staging, and records audit within the posting transaction. It checks both fiscal
year and period locks. `storage-contract` owns database guarantees; parent
`rust-backend` owns cross-service edges.

Observed UI restriction, confirmed unfinished coverage: the documents workbench
requires a staged transaction even though this backend input is optional. The
interaction contract and broader UI limits live in `ui-workflows`.

Observed policy-validation limit (acceptance unconfirmed): account existence and
supported VAT rates do not establish that a caller-selected cost is deductible
or that its VAT is recoverable under the workspace's tax posture.
