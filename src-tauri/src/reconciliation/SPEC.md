---
id: invoice-settlement
type: submodule-design
status: active
title: Invoice payment settlement
parent: rust-backend
depends-on: [evidence-archive, vat-returns, workspace-defaults, business-profiles]
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

Both existing routes are invoice-timed 1930/1510 settlement, not cash revenue
recognition. They enforce `business-profiles`' restrictive timing policy and reject
immutable cash-issued sources inside their immediate transaction, even after an
external current-profile edit. Checks precede voucher/synthetic-row creation or
imported-receipt consumption. Completed replays retain preservation exceptions.

The separate bounded full-PDF cash transition is owned by `invoice-lifecycle`.
It uses this module's staging/match schema but recognizes original revenue/VAT on
receipt date, without a receivable. Cash CSV matching remains blocked; import,
staging, evidence retention and inspection remain available. Historical invoices
without a current validation record are not retroactively validated: under an
explicit invoice-method profile they retain the existing backend settlement checks.

Payment voucher/lines, match, staging transition, replay result, and audit are
committed together, with balance and accounting-date fiscal/period guards. This
service writes vouchers directly; the parent `rust-backend` owns sibling edges
and `storage-contract` owns database constraints.
