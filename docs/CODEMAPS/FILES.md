# File Codemap

Directory structure and key files for the standalone public repository.

## Repository root

```
oppenbokforing/
├── README.md                 # Public quick start and project overview
├── CONTRIBUTING.md           # Public contributor workflow
├── CODE_OF_CONDUCT.md        # Community standards
├── SECURITY.md               # Vulnerability-reporting policy
├── swedish-sole-trader-plan*.md
├── package.json              # npm scripts (test, build, Tauri)
├── flake.nix / .envrc        # Nix dev shell
├── docs/                     # Architecture, schema, legal, and codemaps
├── fixtures/                 # Golden and UI scenario JSON specs
├── scripts/                  # Icons, packaged smoke, signing helper
├── src/                      # React renderer
├── src-tauri/                # Rust and Tauri backend
└── .github/workflows/        # Public CI and release workflows
```

## `src/` — React renderer

```
src/
├── main.tsx                  # App entry
├── App.tsx                   # Router + context providers
├── styles.css                # Global + M8 tour/help styles
├── pages/                    # One page per workbench route
│   ├── WorkspacePickerPage.tsx
│   ├── OnboardingPage.tsx
│   ├── DashboardPage.tsx
│   ├── InvoicesPage.tsx
│   ├── LedgerPage.tsx
│   ├── DocumentsPage.tsx
│   ├── VatPage.tsx
│   ├── YearEndPage.tsx
│   └── SettingsPage.tsx
├── components/
│   ├── AppSidebar.tsx
│   ├── GuidedTour.tsx        # M8
│   ├── HelpTip.tsx
│   ├── PageHelpHeader.tsx    # M8
│   └── VoucherTraceLink.tsx
├── context/
│   ├── WorkspaceContext.tsx
│   ├── LocaleContext.tsx
│   ├── SimpleModeContext.tsx # M8
│   └── WorkspaceLocaleHydrator.tsx
├── i18n/
│   ├── index.ts
│   ├── sv.ts
│   └── en.ts
├── lib/
│   ├── commands.ts           # invoke() wrappers — start here for API
│   ├── bindings.ts           # Generated types
│   ├── dashboardChecklist.ts
│   ├── dashboardTour.ts      # M8
│   ├── invoiceStatus.ts      # M8
│   ├── workbenchNav.ts       # M8
│   ├── helpTopics.ts         # M8
│   └── …                     # money, profile, dialogs, etc.
└── test/
    ├── setup.ts              # RTL cleanup
    └── m8_simple_ux.test.ts  # UI scenario fixture tests
```

## `src-tauri/` — Rust backend

```
src-tauri/
├── Cargo.toml
├── tauri.conf.json           # App id, CSP, plugins
├── capabilities/default.json # Scoped permissions
├── migrations/               # SQLx migrations (run on workspace connect)
│   ├── 202607050001_initial.sql
│   ├── 202607070001_seed_2026_rules.sql
│   ├── 202607080001_m2_invoicing.sql … 202607080007_m5_year_end.sql
│   ├── 202607100001_m6_workspace_settings.sql … 202607100004_workspace_path_settings.sql
│   ├── 202607110001_dashboard_tour.sql   # M8
│   └── 202607110002_simple_mode.sql      # M8
├── src/
│   ├── main.rs
│   ├── lib.rs                # Module tree + command registration
│   ├── commands.rs           # All Tauri command handlers
│   ├── bindings.rs           # Specta export types
│   ├── error.rs              # AppError + SQL redaction
│   ├── db.rs
│   ├── invoicing/            # mod.rs + pdf.rs
│   ├── documents/mod.rs
│   ├── jobs/mod.rs           # PDF job processor
│   ├── settings/mod.rs       # workspace_settings
│   └── …                     # vat, ledger, year_end, backup, etc.
└── tests/
    ├── golden_scenarios.rs
    ├── m3_milestone.rs … m8_milestone.rs
    ├── m7_read_commands.rs
    └── invoice_*.rs, backup_*.rs, profile_audit.rs, local_jobs_idempotency.rs
```

## `docs/` — Documentation

| Path | Purpose |
|------|---------|
| `docs/CODEMAPS/README.md` | Codemap index and contributor orientation |
| `docs/CODEMAPS/` | Architecture, module, and file navigation maps |
| `docs/schema.md` | SQLite tables + migrations |
| `docs/rfc-overview.md` | Public implementation scope and milestone overview |
| `docs/ui-flows.md` | UI state specs |
| `docs/ux-simple-mode.md` | M8 UX spec |
| `docs/security-privacy.md` | Local-data security and privacy model |
| `docs/packaging-release.md` | Release order (unsigned default) |
| `docs/apple-signing-setup.md` | Optional signing (deferred) |
| `docs/adr/` | Architecture decision records |
| `docs/legal/` | English and Swedish legal notices |

## `fixtures/` — Executable specs

```
fixtures/
├── golden-scenarios/         # Rust compliance/accounting scenarios (M1–M6)
│   └── schema.json
└── ui-scenarios/             # M8 guided UX scenarios
    ├── schema.json
    └── guided-ux-onboarding-checklist.json
```

## `scripts/` — Tooling

| Script | Purpose |
|--------|---------|
| `generate-dev-icons.sh` | Generate development icons |
| `setup-apple-secrets.sh` | Optional GitHub signing secrets |
| `packaged-smoke-test.sh` | `TAURI_SMOKE=1` smoke |

## Where to start

| Task | Start file |
|------|------------|
| New Tauri command | `src-tauri/src/commands.rs` + domain `mod.rs` |
| New UI page | `src/pages/` + route in `App.tsx` |
| New accounting rule | `tax_rules` seed + `compliance` engine |
| New golden behavior | `fixtures/golden-scenarios/*.json` + Rust test |
| New M8 UX behavior | `fixtures/ui-scenarios/*.json` + `test:m8` |
| Public contribution workflow | `CONTRIBUTING.md` and `docs/rfc-overview.md` |
