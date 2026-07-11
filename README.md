# ÖppenBokföring

Local-first open-source bookkeeping for Swedish **enskild firma** (sole traders). Prepare invoices, ledger entries, VAT worksheets, and year-end exports on your machine — nothing is filed or paid automatically.

**License:** [AGPL-3.0-or-later](LICENSE) · **Repository:** [vomba/oppenbokforing](https://github.com/vomba/oppenbokforing)

## What it does today

ÖppenBokföring is a **desktop app** (Tauri + React + Rust) that keeps your books and tax prep data **on your computer**. It is built for solo freelancers in Sweden, including those with salary income who need **FA-skatt**.

| Area | Capabilities |
|------|----------------|
| **Onboarding** | Guided setup for business profile, tax status (F-skatt / FA-skatt), VAT posture, and compliance checklist |
| **Invoicing** | Issue invoices with correct VAT, VAT-exempt, or F-tax wording; credit notes and reversals |
| **Ledger** | Double-entry bookkeeping with immutable posted vouchers and traceable journal lines |
| **Documents** | Content-addressed receipt and invoice PDF archive under your workspace |
| **Imports** | CSV bank import with invoice payment matching and reconciliation |
| **Expenses** | Manual expenses with input VAT where applicable; owner withdrawals kept separate from costs |
| **VAT** | Momsdeklaration worksheets for registered, exempt, and voluntary-registration cases; period lock after approval |
| **Cash planning** | Reserved VAT and tax estimates; FA-skatt salary kept out of the business ledger |
| **Year-end** | K1-style simplified annual accounts and NE-bilaga preparation (prepare-only) |
| **Export** | SIE type 4 ledger export, accountant packages, encrypted `.skatbackup` backups |
| **UX** | Swedish-first UI, simple mode, dashboard checklist, guided tour, contextual help |

**Prepare-only by design** — the app produces filing-ready figures and exports. It does **not** submit returns to Skatteverket or initiate payments. See [ADR-0004](docs/adr/0004-prepare-only-tax-workflows.md).

Behaviour is defined by executable **golden scenarios** in [`fixtures/golden-scenarios/`](fixtures/golden-scenarios/README.md) — change a rule, update a fixture, make tests pass.

## Roadmap

| Horizon | Focus |
|---------|--------|
| **Now (beta)** | `v0.1.0-beta.2` — public OSS + review fixes; unsigned macOS builds via release workflow or `tauri build` |
| **Next** | Optional Apple notarization for smoother Gatekeeper installs; community beta feedback |
| **Soon** | Brand wordmark and app icons; contributor onboarding polish |
| **Planned** | Bank sync (open banking) after manual entry and CSV import are solid; Windows/Linux packaging |
| **Not planned (v1)** | Direct Skatteverket filing; cloud sync; hosted OCR |

Track work on [GitHub Issues](https://github.com/vomba/oppenbokforing/issues). Architecture and module maps: [`docs/CODEMAPS/`](docs/CODEMAPS/README.md).

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
git clone https://github.com/vomba/oppenbokforing.git
cd oppenbokforing
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

## Documentation

| Topic | Location |
|-------|----------|
| Architecture & modules | [`docs/CODEMAPS/`](docs/CODEMAPS/README.md) |
| ADRs | [`docs/adr/`](docs/adr/) |
| Database schema | [`docs/schema.md`](docs/schema.md) |
| UI flows | [`docs/ui-flows.md`](docs/ui-flows.md) |
| Privacy model | [`docs/security-privacy.md`](docs/security-privacy.md) |
| macOS packaging | [`docs/packaging-release.md`](docs/packaging-release.md) |

## Security

Report vulnerabilities per [SECURITY.md](SECURITY.md).

## Releases

macOS beta builds: see [`docs/packaging-release.md`](docs/packaging-release.md). Apple signing is optional.

---

*UI copy is Swedish-first; contributor docs are English-first.*
