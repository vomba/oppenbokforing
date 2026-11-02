---
id: accountant-handoff
type: submodule-design
status: draft
title: Accountant package assembly and validation
parent: rust-backend
depends-on: [sie-export, year-end-packages, workspace-defaults]
references: [security-privacy]
---

# Accountant package assembly and validation

## Responsibility

Assemble a reviewable handoff package from SIE, optional existing year-end artifacts,
an evidence index, and a hashed manifest. Validate a selected package without
importing its ledger data.

## Boundary

The package contains `ledger.sie`, optional year-end JSON, the evidence index, and
manifest; it does not copy the retained receipt/document bytes. It is therefore
not a backup replacement. Package validation may append audit but does not merge
accounting records. Shared service edges are recorded in `rust-backend`.

Validation checks manifest version, workspace identity, manifest hash, listed-file
hashes, and canonical path confinement. It bounds the manifest to 1 MiB and each
hashed file to 50 MiB. These checks apply to validation, not automatically to every
export retry.

Observed limitations (acceptance unconfirmed): cached export replay checks manifest
existence rather than revalidating every entry; file publication, succeeded-job
storage, and audit do not form one common transaction. Nested SIE creation still
honors saved export defaults. `security-privacy` owns these broader control gaps;
`local-sqlite` retains the transaction requirement. A successful export is a local
handoff, not proof an accountant accepted it or a return was submitted.
