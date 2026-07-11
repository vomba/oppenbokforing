# Security and Privacy Plan

## Scope

The app stores sensitive accounting records, personal tax assumptions, invoices, receipts, and export packages locally. V1 is local-first and does not include cloud sync, direct filing, or payment initiation.

## Security Principles

- Keep accounting mutations in Rust commands.
- Validate every command input.
- Use parameterized SQL only.
- Scope file access to app data or explicit user-selected paths.
- Avoid broad Tauri permissions.
- Do not log personal identity numbers, invoice contents, salary assumptions, receipt text, tokens, or file contents.
- Treat backups and exports as sensitive files.

## Local Data Controls

- SQLite database lives in the workspace data directory.
- Evidence files are copied into the workspace document archive.
- Imported files are stored by content hash.
- Backups include database, documents, exports, rules, and manifest hash.
- Backup encryption is required before beta.
- Restore must not overwrite an existing workspace without explicit confirmation.

## Tauri Command Controls

- Commands are allowlisted in Rust.
- Commands return structured errors without stack traces.
- Commands touching files must use app paths or user-selected paths.
- Long-running commands use local jobs and are safe to retry.
- Accounting commands require idempotency keys.

## Import Validation

Validate before import:

- File size.
- MIME type where available.
- File extension.
- CSV column shape.
- UTF-8 handling.
- Duplicate content hash.

Rejected imports should leave no partial ledger state.

## Privacy Controls

- No telemetry in v1 unless explicitly added later with opt-in consent.
- No cloud sync in v1.
- No external OCR service in v1 unless added behind a separate privacy review.
- Rule-source updates must not upload accounting data.
- Crash logs must redact workspace path and user-entered financial data.

## Retention and Deletion

- Accounting evidence must be retained according to Swedish bookkeeping requirements.
- The app should warn before deleting documents within the retention period.
- Deleting a workspace is allowed only after explicit confirmation and backup recommendation.
- Audit events should record deletion and export actions.

## Release Blocking Checklist

- No broad file-system permissions.
- No hardcoded secrets.
- No sensitive values in logs.
- SQLite queries are parameterized.
- Backup restore cannot silently overwrite data.
- Import validation covers file size, type, extension, and parser errors.
- Error messages do not expose stack traces or SQL internals.
- Packaged app uses production Tauri permissions.

