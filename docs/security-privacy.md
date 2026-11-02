---
id: security-privacy
type: architecture-design
title: Security and privacy contract
status: active
parent: system-architecture
references: [rust-command-boundary, retained-evidence, storage-contract, encrypted-backups, prepare-only-tax]
---

# Security and Privacy Contract

Required controls are separate from current implementation gaps. The gaps are not
permitted exceptions to the approved product or transaction contract.
Approval of this contract does not establish comprehensive enforcement.

## Scope

The app stores sensitive accounting records, personal tax assumptions, invoices, receipts, and export packages locally. V1 is local-first and does not include cloud sync, direct filing, or payment initiation.

## Security Principles

- Keep accounting mutations in Rust commands.
- Validate every command input.
- Parameterize data queries. Snapshot creation uses an explicitly escaped path in
  `VACUUM INTO`, rather than a bound parameter (`src-tauri/src/db.rs`).
- Scope file access to app data or explicit user-selected paths.
- Avoid broad Tauri permissions.
- Do not log personal identity numbers, invoice contents, salary assumptions, receipt text, tokens, or file contents.
- Treat backups and exports as sensitive files.

## Local Data Controls

- SQLite database lives in the workspace data directory.
- Evidence files are copied into the workspace document archive.
- Imported files are stored by content hash.
- Backups include database, documents, exports, rules, and manifest hash.
- Backup encryption is implemented for `.skatbackup` files.
- Restore requires explicit confirmation and creates a fresh directory; it never
  overwrites an existing workspace despite the input name `confirmOverwrite`.

## Tauri Command Controls

- Commands are allowlisted in Rust.
- Commands return structured errors without stack traces.
- Commands touching files must use app paths or user-selected paths.
- Retry safety is domain-specific. Invoice PDFs use a local queue; many export
  operations run synchronously and use `local_jobs` only as a result cache.
- Keyed posting and approval flows require idempotency keys; this does not cover
  all draft, profile, settings, or lifecycle writes.

## Import Validation

Validate before import:

- File size.
- MIME type where available.
- File extension.
- CSV column shape.
- UTF-8 handling.
- Duplicate content hash.

Rejected imports should leave no partial ledger state. CSV staging is not ledger
posting: staged rows commit before source archival, so archival failure can leave
staged rows behind.

## Privacy Controls

- No telemetry in v1 unless explicitly added later with opt-in consent.
- No cloud sync in v1.
- No external OCR service in v1 unless added behind a separate privacy review.
- Rule-source updates must not upload accounting data.
- Crash logs must redact workspace path and user-entered financial data.

## Retention and Deletion

- Accounting evidence must be retained according to Swedish bookkeeping requirements.
- The app should warn before deleting documents within the retention period.
  Retention metadata exists, but there is no document-delete command or warning flow.
- Deleting a workspace is allowed only after explicit confirmation and backup recommendation.
  No workspace-delete command is currently exposed.
- Audit events should record deletion and export actions.

## Required consistency boundaries

`system-architecture` owns exclusive workspace access, completion before switch,
and preservation of older export copies. These controls apply to commands and
jobs, not only renderer visibility.

`encrypted-backups` owns strict, verified recovery. Successful decryption alone
cannot authorize an incomplete restore. Creation must preserve recoverability,
not produce packages the restore contract refuses.

`prepare-only-tax` owns provisional/approved output identity and correction history.
An export path or existing file does not prove the artifact matches its approved
sources and figures.

## Current control gaps

- Tauri grants only core and dialog permissions. Filesystem IO occurs in Rust;
  accepting an absolute path in a command is not proof it came from a native picker.
- Retained evidence verification checks canonical containment, symlinks, and hashes.
  This does not establish universal filesystem safety for every export/backup path.
- Backup archive extraction rejects traversal, links, and special files. The separate
  manifest-entry join has weaker validation; completeness and manifest-version
  rejection are not comprehensive. Creation does not enforce all restore size/count
  limits. These observed gaps must not be described as resolved by secure extraction.
- Backup database snapshots and subsequent document/export copies are not one
  encompassing atomic snapshot.
- Generic export publication may overwrite destinations. Export replay validation
  differs by domain and does not universally verify artifact hashes.
- Audit records deliberately retain profile and financial metadata locally. They
  are not diagnostic logs and are not redacted by the generic error layer.
- Public storage errors are redacted, but debug builds log underlying errors.
  The no-sensitive-diagnostic-logging requirement is not comprehensively verified.
- Posting, VAT approval, and composite onboarding include transactional audit;
  some draft/profile/export writes and their audit happen separately.

These are documented gaps, not completed fixes. This spec revision changes no
application controls and runs no security certification.

## Release Blocking Checklist

- No broad file-system permissions.
- No hardcoded secrets.
- No sensitive values in logs.
- SQLite queries are parameterized.
- Backup restore cannot silently overwrite data.
- Import validation covers file size, type, extension, and parser errors.
- Error messages do not expose stack traces or SQL internals.
- Packaged app uses production Tauri permissions.

