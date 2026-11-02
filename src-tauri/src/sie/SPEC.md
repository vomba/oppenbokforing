---
id: sie-export
type: submodule-design
status: active
title: SIE ledger export
parent: rust-backend
depends-on: [workspace-defaults]
references: [security-privacy, system-architecture, accounting-ledger, local-sqlite]
---

# SIE ledger export

## Responsibility

Serialize a workspace fiscal year's ledger into a SIE type-4-style handoff artifact
with export metadata and a replay result. It does not import ledger entries or
submit figures to an authority.

## Required export contract

SIE content must represent the selected ledger scope and obey its declared format/encoding.

Opening positions and method-specific entries follow `accounting-ledger`, not a
fictitious zero position for established businesses.

`system-architecture` owns distinct export identity, preserved older copies,
and verified identical retries. Regeneration cannot silently replace an approved
historical artifact from today's ledger.

## Current boundary and gaps

The exporter queries shared profiles/accounts/vouchers directly and uses workspace
fiscal helpers plus shared destination publication. Voucher/detail queries select
posted entries in the requested year. Fiscal-helper calls can bootstrap missing
defaults, so export is not guaranteed read-only with respect to SQLite.

Observed format choices (intent and acceptance unconfirmed): calendar-year ranges,
zero opening balances, export-local voucher numbering, and `#FORMAT PC8` despite
writing UTF-8 string bytes. These are current implementation, not certification of
compatibility with every SIE consumer.

An observed account-balance risk remains unverified by execution: `load_sie_data`
sums journal lines before a LEFT JOIN whose year/status conditions do not filter
unmatched amounts. This may make closing balances include other years/statuses;
do not treat the query as a proven fiscal-year balance contract.

Replay checks artifact existence and counts, not content hash; regeneration reads
the current ledger and all keys for a workspace/year share one filename. This is
not immutable snapshot replay. Broader publication/audit gaps and the governing
requirements are recorded in `security-privacy` and `local-sqlite`.
