# Contributing to ÖppenBokföring

Thank you for helping improve local-first bookkeeping for Swedish sole traders.

## Before you start

- Read the legal notice in [README.md](README.md) and [`docs/legal/en.md`](docs/legal/en.md).
- The app is **prepare-only** — it does not file with Skatteverket ([ADR-0004](docs/adr/0004-prepare-only-tax-workflows.md)).
- License: **AGPL-3.0-or-later** — contributions are under the same license unless otherwise agreed in writing.

## Development setup

```sh
git clone https://github.com/vomba/oppenbokforing.git
cd oppenbokforing
npm ci
npm run icons:dev
npm run tauri:dev
```

No Cursor, ECC skills, or private harness tooling is required.

## Making changes

### Accounting / tax behaviour

1. Find or add a golden scenario in `fixtures/golden-scenarios/`.
2. Write a failing test that loads the fixture `expected` output.
3. Implement the minimal Rust change (rules live in the database seed / rule engine — not in UI-only logic).
4. Run:

```sh
npm run test:fixtures
npm run test:golden
npm run test:all
```

### UI / React

- Match existing patterns in `src/pages/` and `src/components/`.
- Swedish UI strings in `src/i18n/sv.ts`; English in `src/i18n/en.ts`.
- Add Vitest tests for non-trivial UI logic.

### Rust commands

- All new mutations need an **idempotency key** where other accounting commands do.
- Register commands in `src-tauri/src/lib.rs` `generate_handler!`.
- Export TypeScript bindings: `npm run bindings:export` (commit `src/lib/bindings.ts` if changed).

## CI expectations

Pull requests should pass what runs in GitHub Actions:

```sh
npm run ci:public
npm run test:smoke    # optional locally; macOS packaging in release workflow
```

## Pull request checklist

- [ ] Focused change with a clear description
- [ ] Tests added or updated for behaviour changes
- [ ] Golden fixture updated if regulatory output changed
- [ ] No secrets, personal data, or local workspace files committed
- [ ] `bindings:check` clean if you touched Rust command types

## Code of conduct

Be respectful and constructive. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Questions

Open a [GitHub Discussion](https://github.com/vomba/oppenbokforing/discussions) or issue with the `question` label.
