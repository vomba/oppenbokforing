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
scheduler. `security-privacy` owns broader snapshot/path/size gaps.

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

## Current boundary and gaps

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

Current creation does not enforce all restore size/count limits.
File copies are not part of one atomic database snapshot.

`security-privacy` records additional manifest/path gaps.
Existing encryption does not satisfy the full recovery contract.
