---
id: bank-csv-staging
type: submodule-design
status: draft
title: Bank CSV staging
parent: rust-backend
depends-on: [evidence-archive]
---

# Bank CSV staging

## Responsibility

Parse bank-import files into workspace-scoped staged transactions for explicit
review. Staging is not posting: imported rows must be consumed by a settlement or
expense action before they affect the ledger.

## Boundary

The service handles CSV shape, duplicate/import replay state, staged rows, and
source-document linkage. Expense and reconciliation services access these shared
rows directly; there is no exclusive staging repository boundary. Their function
and SQL relationships are recorded in `rust-backend`.

Observed limits (acceptance unconfirmed): the parser handles a narrow line/comma
format with integer `amount_minor`, not general quoted or multiline bank CSV.
An import retry still needs the source file. Staged-row commit precedes evidence
archival/linking, so archival failure may leave staged rows; the transactional
requirement and control gap are recorded in `local-sqlite` and `security-privacy`.
