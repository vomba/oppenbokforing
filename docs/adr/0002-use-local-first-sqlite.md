---
id: local-sqlite
type: architecture-design
title: Use Local-First SQLite
status: active
parent: system-architecture
---

# ADR 0002: Use Local-First SQLite

Decision status: accepted in the source ADR. The user approved adoption into the spec graph.

## Context

The app must work offline and preserve the user's accounting records locally. V1 does not include cloud sync or hosted collaboration.

## Decision

Use SQLite per workspace in the OS app data directory as the primary system of record, with WAL mode and forward-only migrations. `src-tauri/src/db.rs` enables WAL and foreign keys and limits each pool to one connection. The reason for the single-connection limit is unconfirmed.

## Consequences

- Users can work without network access.
- Backup and restore become product-critical features.
- All accounting writes must be transactional.
- Multi-device sync is deferred.
- The app must test migrations against empty databases and seeded prior versions.

## Observed implementation gaps

Some profile, draft, audit, and artifact paths do not share a transaction, as
recorded in `security-privacy`. Their acceptance as product limitations is
unconfirmed; they are implementation gaps, not exceptions to the transaction
requirement above or the mutation boundary in `rust-command-boundary`.

