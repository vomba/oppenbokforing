---
id: desktop-platform
type: architecture-design
title: Use Tauri for the Desktop App
status: draft
parent: system-architecture
---

# ADR 0001: Use Tauri for the Desktop App

Decision status: accepted in the source ADR; graph adoption awaits review.

## Context

The product is a desktop accounting and tax preparation app. It needs local file access, local database access, packaged installers, update support, and a secure bridge between UI and native capabilities.

## Decision

Use Tauri v2 with a React/Vite TypeScript renderer and Rust native layer.

## Consequences

- Rust owns local persistence, file IO, migrations, exports, and accounting mutations.
- The renderer stays focused on UI and workflow state.
- The binary footprint should be smaller than a comparable Electron app.
- The team must maintain Rust and TypeScript code.
- Native integrations are mediated by Tauri plugins and permissions. Dialogs are installed and permitted. The fs plugin is installed without renderer fs grants; updater configuration is present but no updater plugin is installed in the runtime.

