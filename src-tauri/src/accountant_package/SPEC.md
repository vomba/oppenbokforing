---
id: accountant-handoff
type: submodule-design
status: active
title: Accountant package assembly and validation
parent: rust-backend
depends-on: [sie-export, year-end-packages, workspace-defaults]
references: [security-privacy, system-architecture, prepare-only-tax, local-sqlite]
---

# Accountant package assembly and validation

## Responsibility

Assemble a reviewable handoff package from SIE, optional existing year-end artifacts,
an evidence index, and a hashed manifest. Validate a selected package without
importing its ledger data.

## Required package contract

`system-architecture` owns non-overwriting publication and verified identical
retries. A new package must not destroy an older handoff copy.

Provisional tax figures preserve the state, gaps, rules, and sources required by
`prepare-only-tax`. Packaging cannot promote them to completed preparation.
A package does not require professional review or replace the self-service handoff.

## Current boundary and gaps

The package contains `ledger.sie`, optional year-end JSON, the evidence index, and
manifest; it does not copy the retained receipt/document bytes. It is therefore
not a backup replacement. Package validation may append audit but does not merge
accounting records. Shared service edges are recorded in `rust-backend`.

Validation checks manifest version, workspace identity, manifest hash, listed-file
hashes, and canonical path confinement. It bounds the manifest to 1 MiB and each
hashed file to 50 MiB. These checks apply to validation, not automatically to every
export retry.

Current cached replay checks manifest existence rather than every entry.
File publication, succeeded-job storage, and audit do not form one transaction.

Nested SIE creation still honors saved export defaults.
`security-privacy` owns these gaps, and `local-sqlite` retains the transaction requirement.

A successful export is a local handoff, not proof an accountant accepted it or a
return was submitted.
