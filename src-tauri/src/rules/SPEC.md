---
id: tax-rules
type: submodule-design
status: draft
title: Versioned tax-rule lookup
parent: rust-backend
---

# Versioned tax-rule lookup

## Responsibility

Resolve persisted rule-version metadata and typed tax-rule values so domain
services can attach source/year provenance and fail closed when required
configuration is absent. Rule storage is local; this module does not fetch or
publish regulatory updates.

## Boundary

Latest-active lookup and active-for-a-specific-year lookup are different contracts.
Issuance uses the latter for its issue year; the two APIs are not interchangeable
when a caller needs date-specific rules. Typed optional lookups treat malformed values as
absent; required integer lookup returns a validation error on absence or invalidity.

This is a data-access surface, not a universal policy engine. Each consuming domain
owns which rule is required and which transition it protects; sibling edges live
in `rust-backend`.

Observed scope (acceptance unconfirmed): the repository seeds 2026 rules. A year
field, active-version identifier, or stored source URL does not establish complete
coverage for other years or independent legal validation of those rules.
