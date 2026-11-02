---
id: retained-evidence
type: architecture-design
title: Use Local Content-Addressed Document Storage
status: active
parent: system-architecture
---

# ADR 0005: Use Local Content-Addressed Document Storage

Decision status: accepted in the source ADR. The user approved adoption into the spec graph.

## Context

Receipts, invoice PDFs, imports, and filing packages are accounting evidence. They must remain linked to ledger records and survive backups.

## Decision

Copy imported evidence into a local workspace document archive and store files by content hash. Keep metadata in SQLite and include files in backup manifests.

## Consequences

- Duplicate files can be detected by hash.
- Backups can verify integrity.
- File imports must validate size, type, and extension.
- Users must be warned before deleting evidence still covered by retention obligations. This remains a requirement, not a shipped deletion flow: documents have retention metadata but there is no document-delete command.

