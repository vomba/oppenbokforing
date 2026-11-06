---
id: encrypted-backups
type: submodule-design
status: active
title: Encrypted backup publication and restore
parent: rust-backend
references: [local-sqlite, security-privacy, system-architecture]
---

# Encrypted backup publication and restore

## Responsibility

Preserve a recoverable workspace database, retained documents, exports, and rule
state in an encrypted `.skatbackup`, and restore into a fresh workspace directory.

This service owns claims, staging, and manifests. `invoice-pdf-jobs` is not its
scheduler. `security-privacy` owns broader filesystem and privacy controls.

## Required recovery contract

Restore succeeds only for a complete, verified package. A missing or corrupt retained
receipt must fail the restore, not produce editable partial books. Existing
workspaces remain unchanged, and the failure must identify the problem.

Required verification covers the manifest, included data/evidence, supported format,
paths, and integrity before publication. A successful backup must fit the supported
restore limits.

Partial salvage is outside the approved recovery contract.

Snapshot coordination must preserve a coherent recoverable state across the database
and retained files. `system-architecture` owns exclusive access, in-flight operation
completion, and artifact publication identity.

## Implemented verification and publication

A checkpoint and `VACUUM INTO` produce the database snapshot; copied files and a
hashed manifest form the package. The private crypto component owns the format-v2
header, random salt/nonce, Argon2 key derivation, AES-256-GCM encryption, and bounded
safe tar extraction. Its passphrase minimum measures trimmed UTF-8 bytes, despite
the user-facing wording referring to characters (observed mismatch, unconfirmed).

Create claims have five-minute token leases, renewed every 60 seconds by the
command adapter. Recovery/finalization must use the current unexpired token.
No-clobber publication precedes a fenced success/audit transaction; a stale owner
cannot finalize another owner's claim. Retry semantics include staged publication
recovery rather than blindly creating another package.

Restore requires explicit confirmation and uses a fresh UUID directory.
It decrypts/verifies in staging, preserves the logical workspace ID, and rewrites
physical paths. It rejects directory backups.

Restore does not activate the workspace or add a recent entry.
The UI opens the restored workspace afterward.

Creation, interrupted-publication recovery, and restore share one verifier over a
private extracted stage. Verification precedes successful publication/recovery or
copying data into a fresh restore directory. A manifest hash alone is insufficient:
the verifier also reconciles the complete physical inventory and database-referenced
retained evidence, including evidence omitted from both the manifest and archive.

Only manifest version 1 is supported. Required fields have strict JSON types;
unknown fields and duplicate JSON keys are rejected. Workspace ID/name must match
the single database workspace, and the creation timestamp must be RFC 3339.
Paths use the same confined relative grammar as extraction: no absolute paths,
empty/dot/parent components, backslashes, colons or NULs. Paths are unique across
entry types. `workspace.sqlite` is a required file; other files and directories
are confined to `documents/` and `exports/`. Files declare lowercase SHA-256 and
byte size; directories declare an empty hash, zero bytes, and a trailing slash.
The optional v1 `rule_versions/` virtual `database_table` entry declares an empty
hash and positive row count, verified against the database; no other virtual entry
is supported. Physical files/directories must match the declared inventory exactly.

The snapshot is opened read-only without migrations or trusting its physical paths.
SQLite integrity and foreign-key checks must pass. Every retained-document row must
belong to the workspace and resolve under the staged document root; the evidence
archive's retained-object verifier checks containment, links, and actual byte hashes.
Manifest hashes cannot substitute for these database evidence hashes.

Archive construction and extraction enforce at most 10,000 physical entries and
512 MiB of tar bytes; encrypted packages must also fit 512 MiB including their
header and authentication tag. Creation reuses the restore decrypt/extract checks
before publication, so a successful package fits these supported restore limits.
Extraction rejects duplicate paths, links, and special files. Failed private stages
are removed; no existing workspace is used as a restore target.

## Remaining gaps

File copies are not part of one atomic database-plus-files snapshot. Package
verification detects missing or mismatched evidence but does not coordinate concurrent
workspace operations or prove a coherent point-in-time snapshot. These lifecycle and
snapshot guarantees remain owned by `system-architecture`; encryption and successful
verification do not establish them.
