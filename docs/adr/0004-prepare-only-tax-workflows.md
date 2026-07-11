# ADR 0004: Prepare Filing Outputs, Do Not Submit Directly in V1

Status: accepted

## Context

Direct filing to Skatteverket requires technical access, signing, legal responsibility, and a higher support burden. The user still needs filing-ready outputs and clear guidance.

## Decision

V1 prepares VAT drafts, year-end packages, `NE` draft values, PDFs, and accountant/export bundles. Direct submission is deferred.

## Consequences

- The product can launch without direct authority integration risk.
- All outputs must be traceable and reviewable.
- Export package quality is a core product requirement.
- Direct filing can be added later behind a separate compliance review.

