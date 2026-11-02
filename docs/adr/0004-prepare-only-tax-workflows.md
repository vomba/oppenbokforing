---
id: prepare-only-tax
type: architecture-design
title: Prepare Filing Outputs Without Direct Submission
status: draft
parent: system-architecture
---

# ADR 0004: Prepare Filing Outputs, Do Not Submit Directly in V1

Decision status: accepted in the source ADR; graph adoption awaits review.

## Context

Direct filing to Skatteverket requires technical access, signing, legal responsibility, and a higher support burden. The user still needs filing-ready outputs and clear guidance.

## Decision

The app prepares VAT drafts, year-end packages, `NE` draft values, PDFs, and accountant/export bundles. It does not submit returns or initiate payments. Current VAT/year-end preparation artifacts are JSON, not proof of an authority-compatible submission format; export does not mean filed.

## Consequences

- The product can launch without direct authority integration risk.
- All outputs must be traceable and reviewable.
- Export package quality is a core product requirement.
- Direct filing can be added later behind a separate compliance review.

