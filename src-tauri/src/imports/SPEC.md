---
id: bank-csv-staging
type: submodule-design
status: draft
title: Bank CSV staging
parent: rust-backend
depends-on: [evidence-archive]
references: [product-goal, local-sqlite, security-privacy]
---

# Bank CSV staging

## Responsibility

Parse bank-import files into workspace-scoped staged transactions for explicit
review. Staging is not posting: imported rows must be consumed by a settlement or
expense action before they affect the ledger.

## Required compatibility and failure contract

Required bank targets are SEB, Swedbank/Sparbankerna, Handelsbanken, and Nordea.
These are compatibility requirements, not claims of existing support.

A compatibility claim identifies the actual export format/version and representative
fixtures. Checks cover column/date/amount conventions, quoting, encoding, duplicates,
and invalid rows. A bank name alone is not a format definition.

Unsupported formats must be rejected clearly. Malformed rows must not be silently
skipped or turned into partial successful imports. No universal CSV or guided
column-mapping capability is implied.

Imported evidence and unresolved rows remain available for review.
Unsupported accounting cases cannot become ordinary domestic postings.
`product-goal` owns that scope boundary.

## Current boundary and gaps

The service handles CSV shape, duplicate/import replay state, staged rows, and
source-document linkage. Expense and reconciliation services access these shared
rows directly; there is no exclusive staging repository boundary. Their function
and SQL relationships are recorded in `rust-backend`.

The parser supports a narrow line/comma format with integer `amount_minor`.
No named-bank format/version compatibility is established by that parser.
Quoted/multiline parsing and bank-specific validation remain gaps.

Retries still need the source file. Staging commits before evidence archival/linking,
so archival failure can leave rows. `local-sqlite` retains the transaction requirement,
and `security-privacy` records the gap.
