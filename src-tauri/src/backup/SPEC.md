---
id: encrypted-backups
type: submodule-design
status: draft
title: Encrypted backup publication and restore
parent: rust-backend
references: [local-sqlite, security-privacy]
---

# Encrypted backup publication and restore

## Responsibility

Preserve a recoverable workspace database, retained documents, exports, and rule
state in an encrypted `.skatbackup`, and restore into a fresh workspace directory.
This service owns backup claims/staging/manifests; `invoice-pdf-jobs` is not its
scheduler. Broader snapshot/path/size gaps are owned by `security-privacy`.

## Boundary

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

Restore requires explicit confirmation but never overwrites: it verifies/decrypts
into staging, retains the logical workspace ID, rewrites physical paths, and
publishes into a new UUID directory. Directory backups are rejected. Restore does
not itself activate the workspace or add a recent entry; the UI opens it afterward.
The format's restore limits do not certify that creation enforces equivalent limits;
see the unconfirmed implementation gaps in `security-privacy`.
