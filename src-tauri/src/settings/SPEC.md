---
id: workspace-settings
type: submodule-design
status: draft
title: Workspace presentation and path preferences
parent: rust-backend
---

# Workspace presentation and path preferences

## Responsibility

Persist locale, simple-mode/tour preferences, and default export/backup destinations
for a workspace. These preferences control presentation and destination selection,
not accounting policy, authorization, or installation of native integrations.

## Boundary

Defaults are Swedish, simple mode on, tour incomplete, and updater disabled.
Locale is limited to `sv`/`en`. Omitted optional save values preserve current
settings; empty directory values can clear configured defaults. Reading settings
may ensure the default row exists rather than being a pure query.

Directory normalization uses shared path validation; default destination resolution
also reads settings. That cycle is recorded in `rust-backend`, not an independent
filesystem permission boundary. Tour completion has its own persistence action.
The meaning of simple-mode visibility is owned by `ui-workflows`.

`updater_enabled` is stored configuration, not a working updater capability;
`rust-backend` owns the current integration support limits. Observed nontransactional
settings/audit paths do not change the accepted transaction requirements in
`local-sqlite` or the gaps recorded in `security-privacy`.
