---
id: rust-command-boundary
type: architecture-design
title: Use a Rust Command Boundary for Mutations
status: draft
parent: system-architecture
---

# ADR 0003: Use a Rust Command Boundary for Mutations

Decision status: accepted in the source ADR; graph adoption awaits review.

## Context

Accounting data must be immutable, auditable, and protected from accidental UI-side mutation. The desktop renderer should not directly write to SQLite.

## Decision

Expose a narrow set of Tauri commands. All accounting mutations go through Rust services and SQLite transactions.

## Consequences

- The renderer uses generated TypeScript bindings for command DTOs.
- Workspace-bound commands obtain the active workspace. Posting/approval services validate applicable rules, replay keys, and locks; these checks are domain-specific, not common middleware for every command. Lifecycle/status commands can run without an open workspace, and draft/profile/settings writes do not universally take replay keys.
- Command-level tests become the main integration safety net.
- UI code cannot bypass posting, reversal, and audit rules.

