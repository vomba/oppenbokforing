---
id: tax-task-calendar
type: submodule-design
status: active
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

## Required task semantics

The projection distinguishes accounting year, filing calendar, preparation state,
and user-reported external submission. `tax-rules` governs applicable schedules,
not an unconditional latest-active year.

`prepare-only-tax` owns submission attestations and their evidence/audit contract.
Only that explicit user report can indicate external completion. Approval, export,
or a path string alone cannot mean filed.

Relevant unresolved activity or missing guided answers prevents misleading complete
status. The renderer presents backend states rather than invents deadlines or
submission claims.

## Current boundary and gaps

Rust owns deadline regime matching, task necessity, status, and ordering. Missing
profile/regime/schedule data yields an undated profile-review task rather than a
guessed deadline; invalid configured schedules produce validation errors.
Registered and voluntarily registered VAT use their compatible regime, while
low-turnover-exempt profiles do not generate VAT-return tasks.

The service preserves overdue tasks and the nearest future VAT deadline and sorts
by status/date/ID. Approved-plus-export-path state becomes
`prepared_external_submission_required`: metadata does not prove submission or
that an exported file still exists. `prepare-only-tax` owns that product boundary.

Current selection uses the latest active rule year, not the caller's as-of year.
The year-end task targets the preceding fiscal year. This does not establish the
required accounting-year/filing-calendar coverage.

Submission attestations and the corresponding completion state are not implemented.
The existing prepared status is not that new state.
