---
id: invoice-settlement
type: submodule-design
status: active
title: Invoice payment settlement
parent: rust-backend
depends-on: [evidence-archive, vat-returns, workspace-defaults]
references: [storage-contract, product-goal, tax-rules, accounting-ledger]
---

# Invoice payment settlement

## Responsibility

Convert a reviewed bank receipt into an invoice settlement and traceable payment
voucher. Own both matching imported staged receipts and recording a payment with
retained PDF bank evidence when no CSV receipt is available.

## Required settlement contract

An invoice can receive several partial payments. Each receipt has its own source,
date, amount, replay identity, and audit trail. The remaining balance determines
whether the invoice is unpaid, partly paid, or settled.

A receipt cannot exceed the outstanding amount through this workflow.
`product-goal` excludes overpayments/refunds and defines the supported credit cases.
Unsupported receipts remain retained/staged rather than ordinary revenue.

Recognition follows the selected method and applicable rules under `tax-rules`.
An opening invoice must not create duplicate revenue or receivables on settlement.
`accounting-ledger` owns opening-position reconciliation.

## Current boundary and gaps

Settlement accepts an issued standard invoice once, for its exact full total.
Partial settlement and its read-model balances are implementation gaps.
A staged receipt must remain unconsumed and belong to the same workspace.

The direct payment route requires verified retained PDF evidence and an explicit
payment date, creates a synthetic staged transaction, and converges on the same
settlement rules. It does not initiate a bank payment or accept arbitrary files
as proof solely because their metadata says PDF.

Payment voucher/lines, match, staging transition, replay result, and audit are
committed together, with balance and accounting-date fiscal/period guards. This
service writes vouchers directly; the parent `rust-backend` owns sibling edges
and `storage-contract` owns database constraints.
