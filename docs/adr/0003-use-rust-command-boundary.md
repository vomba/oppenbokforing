# ADR 0003: Use a Rust Command Boundary for Mutations

Status: accepted

## Context

Accounting data must be immutable, auditable, and protected from accidental UI-side mutation. The desktop renderer should not directly write to SQLite.

## Decision

Expose a narrow set of Tauri commands. All accounting mutations go through Rust services and SQLite transactions.

## Consequences

- The renderer uses generated TypeScript bindings for command DTOs.
- Rust validates active workspace, rule version, idempotency key, and period lock state.
- Command-level tests become the main integration safety net.
- UI code cannot bypass posting, reversal, and audit rules.

