# ÖppenBokföring Codemaps

Last updated: 2026-07-11. These navigation guides describe the standalone public product repository.

| Map | Use it for |
|-----|------------|
| [Architecture](./ARCHITECTURE.md) | System boundaries, data flow, invariants, and testing layers |
| [Modules](./MODULES.md) | Rust and React responsibilities, command groups, and dependencies |
| [Files](./FILES.md) | Repository layout, migrations, test suites, scripts, and starting points |

## Quick orientation

The local-first application flows from React pages through typed `invoke()` wrappers to Rust Tauri commands, then into domain modules and the workspace-local SQLite database, document store, and exports. The renderer never writes SQLite directly.

For behavioral requirements, start with the product plans, then [`docs/rfc-overview.md`](../rfc-overview.md), ADRs, schema/migrations, executable fixtures, and UI flows.

## Current scope

The product provides local-first bookkeeping and prepare-only tax workflows. Run `npm run tauri:dev` for local use or `npm run tauri:build` for an unsigned desktop bundle. See [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for verification and contribution expectations.
