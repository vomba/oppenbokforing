# ÖppenBokföring

Local-first open-source bookkeeping for Swedish **enskild firma** (sole traders). Prepare invoices, ledger entries, VAT worksheets, and year-end exports on your machine.

**License:** [AGPL-3.0-or-later](LICENSE) · **Maintainer:** [vomba/oppenbokforing](https://github.com/vomba/oppenbokforing)

## Legal notice (English)

> **ÖppenBokföring** is independent open-source software for local bookkeeping and **prepare-only** tax workflows. It is **not** affiliated with Skatteverket or any government authority. It does **not** provide tax, legal, or accounting advice, and it does **not** file returns or initiate payments on your behalf. You are responsible for verifying all figures before submitting anything to Skatteverket, an accountant, or other parties.

## Juridiskt meddelande (Svenska)

> **ÖppenBokföring** är oberoende öppen källkod för lokal bokföring och **förberedande** skatte- och deklarationsunderlag. Programmet är **inte** knutet till Skatteverket eller någon myndighet. Det ger **inte** skatte-, juridisk eller redovisningsrådgivning och **lämnar inte in** deklarationer eller betalningar för din räkning.

Full legal text: [`docs/legal/`](docs/legal/README.md) · Trademark: [`docs/TRADEMARK.md`](docs/TRADEMARK.md)

## Beta warning

**Beta software** — keep encrypted backups and test restore before compliance deadlines. Unsigned macOS builds may require extra steps (Gatekeeper). See [`docs/legal/en.md`](docs/legal/en.md).

## Stack

- Tauri v2 · React · TypeScript · Rust · SQLite
- All accounting mutations via Rust commands (renderer does not write the database)
- Content-addressed document archive under your workspace

## Quick start

```sh
npm ci
npm run icons:dev
npm run tauri:dev
```

**Requirements:** Node.js 22+, Rust stable, platform Tauri dependencies ([Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)).

Optional reproducible shell: `direnv allow` (Nix flake in repo).

## Verify changes

```sh
npm run ci:public          # fixtures + golden + tests + build
npm run tauri:build        # unsigned desktop bundle
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contributor workflow.

## Domain rules

Regulatory behaviour is defined by **golden scenarios** in [`fixtures/golden-scenarios/`](fixtures/golden-scenarios/README.md). Change a rule → add or update a fixture → make tests pass.

Architecture: [`docs/CODEMAPS/README.md`](docs/CODEMAPS/README.md) · ADRs: [`docs/adr/`](docs/adr/) · Schema: [`docs/schema.md`](docs/schema.md)

## Security

Report vulnerabilities per [SECURITY.md](SECURITY.md). Privacy model: [`docs/security-privacy.md`](docs/security-privacy.md).

## Releases

macOS beta builds: see [`docs/packaging-release.md`](docs/packaging-release.md). Apple signing is optional.

---

*UI copy is Swedish-first; contributor docs are English-first.*
