---
id: evidence-archive
type: submodule-design
status: draft
title: Retained evidence archive
parent: rust-backend
depends-on: [workspace-defaults]
references: [retained-evidence, security-privacy]
---

# Retained evidence archive

## Responsibility

Turn external files and generated invoice bytes into workspace-owned evidence.
`retained-evidence` owns the storage decision and its rationale; this module owns
import validation, deduplication, retained-object verification, listing, and reveal.

## Boundary

Imports are capped at 10 MiB, identify PDF/PNG/JPEG by content, and admit declared
text/CSV through their separate path. Files are copied into the content-addressed
archive with metadata and seven-year retention metadata; a hash duplicate reuses
the retained object. Import replay claims belong to this service.

`verify_retained_document` and its transaction variant check workspace association,
path containment, symlinks, and content hash. Payment/expense callers must verify
actual retained bytes, not trust a document ID or metadata row alone. Filesystem
checks and broader unresolved controls are distinguished in `security-privacy`.

Reveal stages verified content for the operating-system viewer; it is not a generic
path-opening API. Generated-byte storage supports invoice PDFs. This module does
not post transactions, determine business tax treatment, or implement document
deletion; retention/deletion requirements remain in `security-privacy`.
