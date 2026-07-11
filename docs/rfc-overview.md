# RFC overview (public)

This is a short contributor-facing summary. The full internal RFC (`implementation-rfc.md`) stays in the private development repository.

## Product

Local-first desktop app for Swedish **enskild firma**: invoices, ledger, documents, VAT periods, year-end (K1 / NE preparation), SIE export, encrypted backup.

## Architecture

```
React UI → invoke() → Rust commands → rule engine + SQLite
```

- Renderer **never** writes SQLite directly ([ADR-0003](adr/0003-use-rust-command-boundary.md)).
- Rules in `tax_rules` / `rule_versions`; outputs cite rule version and source URL.
- Golden scenarios in `fixtures/golden-scenarios/` are the executable behaviour contract.

## Commands (mutations)

Accounting mutations require an **idempotency key**. Errors use structured `AppError` codes (`validation_error`, `storage_error`, `locked_period`, etc.).

## Milestones (capability map)

| Area | Golden scenarios (examples) |
|------|---------------------------|
| M1 Compliance | FA-skatt, VAT exempt, backup |
| M2 Invoicing | Credit invoice reversal |
| M3 Documents | CSV import, owner withdrawal |
| M4 VAT | Threshold breach, period lock |
| M5 Year-end | K1 + NE |
| M6 Desktop | SIE export |

## Non-goals (v1)

- Direct filing to Skatteverket ([ADR-0004](adr/0004-prepare-only-tax-workflows.md))
- Cloud sync, external OCR, automatic payment initiation

## Verification

```sh
npm run ci:public
```

See [CONTRIBUTING.md](../../CONTRIBUTING.md).
