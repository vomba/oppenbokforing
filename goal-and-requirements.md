---
id: product-goal
type: goal-and-requirements
status: draft
title: ÖppenBokföring goals and requirements
tags: [product]
---

# ÖppenBokföring

## Goal and audience

Help Swedish enskild firma owners keep local books and prepare reviewable tax
and accountant handoffs without an accountant-first interface. The audience is
solo freelancers, including people combining employment and business under FA-skatt.
The app is independent open-source software, not tax advice or a government service.

This draft imports the purpose from `README.md` and the accepted decisions in
`desktop-platform`, `local-sqlite`, `rust-command-boundary`, `prepare-only-tax`, and
`retained-evidence`. Source-observed capabilities below are not a claim of legal
or accounting completeness; all imported nodes await user review.

## Capabilities

- Create and reopen local workspaces; preserve them with encrypted backup and restore.
- Guide business, tax, and VAT profile setup, with Swedish-first and English UI,
  simple navigation, contextual help, and review before material accounting actions.
- Create invoice drafts, issue invoices with tax/VAT safeguards and retained PDFs,
  record full payments, and issue compensating credit notes.
- Keep a double-entry ledger with protected posted records and auditable source links.
- Retain documents by content hash, stage bank CSV transactions, reconcile receipts,
  and post expenses with evidence or an explicit explanation.
- Prepare VAT worksheets with source-line traces and period locking on approval;
  show rule-backed tax tasks and an approximate cash-planning projection.
- Prepare simplified annual-account and NE draft artifacts, review readiness,
  approve year-end packages, and export SIE and accountant handoff packages.

The cash projection is a **provisional approximation**, confirmed by the user;
its formula and limits belong to `cash-planning`. The user also confirmed that
narrow daily-entry flows are unfinished coverage, not intended product exclusions;
`ui-workflows` distinguishes current UI support from its requirements.

## Product boundary

The product prepares information for review and external submission. Direct tax
filing and payment initiation are excluded by `prepare-only-tax`. Cloud sync,
hosted collaboration, and hosted OCR are outside the documented local-first scope;
this is not an assertion that every unimplemented feature is forbidden.

Sensitive records remain local under the controls and explicit implementation gaps
in `security-privacy`. Golden fixtures define regression examples, not legal
certification. Users remain responsible for checking figures and submitting them
outside the app.
