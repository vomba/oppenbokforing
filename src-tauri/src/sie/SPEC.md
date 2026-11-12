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

Newly rendered exports, including regeneration of a missing cached file, emit
`#RAR 0` from the selected fiscal year's real `fiscal_years.starts_on` and
`ends_on`, formatted as canonical `YYYYMMDD`. The fiscal row must belong to the
requested workspace; a deterministic year ID alone is not scope authority.
Stored bounds must be valid, canonical ISO `YYYY-MM-DD` dates in non-reversed
order. Empty, malformed, impossible or reversed dates fail closed, without a
calendar-year fallback. Thus an approved June 1–December 31 first year emits
`#RAR 0 20260601 20261231`; ordinary full-calendar years remain unchanged.
This changes only header date provenance, not voucher dates, source identities,
account amounts, fiscal eligibility or ledger/rule/origin records. Public export
input and summary DTOs are unchanged.

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

Observed format choices (intent and acceptance unconfirmed): zero opening
balances, export-local voucher numbering, and `#FORMAT PC8` despite
writing UTF-8 string bytes. These are current implementation, not certification of
compatibility with every SIE consumer.

An observed account-balance risk remains unverified by execution: `load_sie_data`
sums journal lines before a LEFT JOIN whose year/status conditions do not filter
unmatched amounts. This may make closing balances include other years/statuses;
do not treat the query as a proven fiscal-year balance contract.

Replay checks artifact existence and counts, not content hash. When the existing
file and cached voucher/account counts match, retry preserves the file's exact
bytes, including an older January-start header even if today's valid fiscal
metadata is narrower. The date correction does not rewrite that historical
artifact. Missing-file regeneration follows the existing policy: it reads the
selected actual fiscal metadata and current ledger, not a saved snapshot. All
keys for a workspace/year still share one filename. Neither regeneration nor
this header correction establishes a new immutability guarantee. Broader
publication/audit gaps and the governing requirements are recorded in
`security-privacy` and `local-sqlite`.
