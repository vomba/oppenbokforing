---
id: local-sqlite
type: architecture-design
title: Use Local-First SQLite
status: draft
parent: system-architecture
---

# ADR 0002: Use Local-First SQLite

Decision status: accepted in the source ADR; graph adoption awaits review.

## Context

The app must work offline and preserve the user's accounting records locally. V1 does not include cloud sync or hosted collaboration.

## Decision

Use SQLite per workspace in the OS app data directory as the primary system of record, with WAL mode and forward-only migrations. `src-tauri/src/db.rs` enables WAL and foreign keys and limits each pool to one connection. The reason for the single-connection limit is unconfirmed.

## Consequences

- Users can work without network access.
- Backup and restore become product-critical features.
- Accounting posting must be transactional. This is not a blanket claim that every profile, draft, audit, or artifact write shares a transaction; `security-privacy` distinguishes those limits.
- Multi-device sync is deferred.
- The app must test migrations against empty databases and seeded prior versions.

