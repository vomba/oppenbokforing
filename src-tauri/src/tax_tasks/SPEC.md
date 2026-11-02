---
id: tax-task-calendar
type: submodule-design
status: draft
title: Derived tax-task calendar
parent: rust-backend
depends-on: [business-profiles, tax-rules]
references: [prepare-only-tax]
---

# Derived tax-task calendar

## Responsibility

Tell the UI what VAT/year-end preparation or profile-review action is needed,
with dates, status, route target, rule-version identity, and official source URL.
No task table is maintained; tasks are derived from profiles, rules, and current
return/package state.

## Boundary

Rust owns deadline regime matching, task necessity, status, and ordering. Missing
profile/regime/schedule data yields an undated profile-review task rather than a
guessed deadline; invalid configured schedules produce validation errors.
Registered and voluntarily registered VAT use their compatible regime, while
low-turnover-exempt profiles do not generate VAT-return tasks.

The service preserves overdue tasks and the nearest future VAT deadline and sorts
by status/date/ID. Approved-plus-export-path state becomes
`prepared_external_submission_required`: metadata does not prove submission or
that an exported file still exists. `prepare-only-tax` owns that product boundary.

Observed year scope (acceptance unconfirmed): rule selection uses the latest
active rule year, not the caller's as-of year. The year-end task targets the
preceding fiscal year. Consumers must not reinterpret as-of as arbitrary-year
rule selection or infer full multi-year calendar coverage.
