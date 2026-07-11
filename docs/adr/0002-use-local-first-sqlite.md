# ADR 0002: Use Local-First SQLite

Status: accepted

## Context

The app must work offline and preserve the user's accounting records locally. V1 does not include cloud sync or hosted collaboration.

## Decision

Use SQLite in the OS app data directory as the primary system of record. Enable WAL mode during implementation and use forward-only migrations.

## Consequences

- Users can work without network access.
- Backup and restore become product-critical features.
- All accounting writes must be transactional.
- Multi-device sync is deferred.
- The app must test migrations against empty databases and seeded prior versions.

