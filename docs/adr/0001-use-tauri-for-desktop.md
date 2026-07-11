# ADR 0001: Use Tauri for the Desktop App

Status: accepted

## Context

The product is a desktop accounting and tax preparation app. It needs local file access, local database access, packaged installers, update support, and a secure bridge between UI and native capabilities.

## Decision

Use Tauri v2 with a React/Vite TypeScript renderer and Rust native layer.

## Consequences

- Rust owns local persistence, file IO, migrations, exports, and accounting mutations.
- The renderer stays focused on UI and workflow state.
- The binary footprint should be smaller than a comparable Electron app.
- The team must maintain Rust and TypeScript code.
- Native integrations such as updater, dialogs, and scoped file access are handled through Tauri plugins and permissions.

