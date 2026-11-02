---
id: invoice-settlement
type: submodule-design
status: draft
title: Invoice payment settlement
parent: rust-backend
depends-on: [evidence-archive, vat-returns, workspace-defaults]
references: [storage-contract]
---

# Invoice payment settlement

## Responsibility

Convert a reviewed bank receipt into an invoice settlement and traceable payment
voucher. Own both matching imported staged receipts and recording a payment with
retained PDF bank evidence when no CSV receipt is available.

## Boundary

Settlement accepts an issued standard invoice, once, for its exact full total.
Partial payments and settlement of a credit note are not implemented (observed
limits; permanent product exclusion is unconfirmed). A staged receipt must remain
unconsumed and belong to the same workspace.

The direct payment route requires verified retained PDF evidence and an explicit
payment date, creates a synthetic staged transaction, and converges on the same
settlement rules. It does not initiate a bank payment or accept arbitrary files
as proof solely because their metadata says PDF.

Payment voucher/lines, match, staging transition, replay result, and audit are
committed together, with balance and accounting-date fiscal/period guards. This
service writes vouchers directly; the parent `rust-backend` owns sibling edges
and `storage-contract` owns database constraints.
