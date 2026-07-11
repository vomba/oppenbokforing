# ÖppenBokföring Codemaps

Last updated: 2026-07-11. These navigation guides describe the post-M8 `main` branch (PR #26 merged).

| Map | Use it for |
|-----|------------|
| [Architecture](./ARCHITECTURE.md) | System boundaries, data flow, invariants, and testing layers |
| [Modules](./MODULES.md) | Rust and React responsibilities, command groups, and dependencies |
| [Files](./FILES.md) | Repository layout, migrations, test suites, scripts, and starting points |

## Quick orientation

The local-first application flows from React pages through typed `invoke()` wrappers to Rust Tauri commands, then into domain modules and the workspace-local SQLite database, document store, and exports. The renderer never writes SQLite directly.

For behavioral requirements, follow the hierarchy in [`AGENTS.md`](../../AGENTS.md): product plans, RFC, ADRs, schema/migrations, executable fixtures, then UI flows.

## Current scope

M0–M8 are complete. Default distribution remains `npm run tauri:dev` for local use or `npm run tauri build` for an unsigned personal build. Post-M8 work is stabilization and release CI: #12, #16, #20–#22, and #27–#29.
