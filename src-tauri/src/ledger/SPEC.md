---
id: accounting-ledger
type: submodule-design
status: draft
title: Posting primitives and ledger reads
parent: rust-backend
depends-on: [vat-returns]
references: [storage-contract]
---

# Posting primitives and ledger reads

## Responsibility

Provide balanced invoice posting and compensating reversal primitives, plus
workspace-scoped voucher, account, and balance reads. Preserve source links so
workbenches and exports can trace posted amounts.

## Boundary

Posting primitives use the caller's transaction; invoice services own the complete
business transition. Other domains can and do write vouchers directly, so this
module is not the sole database writer or a repository abstraction over all
accounting. The service graph is recorded in `rust-backend`.

Balance checks, debit/credit constraints, replay uniqueness, and the exact limits
of database immutability live in `storage-contract`. Corrections add compensating
records rather than editing posted rows. Period guards use accounting dates.

Observed limitation (acceptance unconfirmed): invoice posting resolves fixed BAS
accounts 1510/3041/2611; invoice-line account metadata does not select the posting
account. `net_revenue_minor_for_fiscal_year` measures net credits on revenue account
3041, not profit after expenses. Consumers must not reinterpret it as net income.
