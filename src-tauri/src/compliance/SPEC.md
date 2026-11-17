---
id: profile-compliance
type: submodule-design
status: active
title: Profile compliance evaluation
parent: rust-backend
depends-on: [tax-rules]
---

# Profile compliance evaluation

## Responsibility

Evaluate supported tax/VAT profile scenarios and return reviewable outcomes for
onboarding and compliance commands. This is profile guidance, not authorization
for every posting or a certificate of tax compliance.

## Boundary

Profile posture selects scenario IDs in Rust. The live scenario command accepts
four allowlisted scenarios: F-skatt business guidance (`f-skatt-business`),
FA-skatt salary and business guidance, low-turnover VAT exemption, and its
threshold breach. All four are embedded for packaged builds; broader fixture
loading is a source-test facility. Other golden scenarios test their own domain
implementations rather than expanding this evaluator automatically.

F-skatt selects its own business guidance, not the FA-skatt scenario. Its outcomes
require F-skatt business posture, business income in the business ledger, and
F-skatt approval text on invoices. F-skatt does not require salary assumptions;
FA-skatt still requires a positive salary assumption. Exempt profiles additionally
select the low-turnover exemption checks, not a replacement for tax guidance.
Planning/unknown posture selection and explicit unsupported-scenario failures are
unchanged by the F-skatt onboarding fix.

Unknown evaluator scenarios return an explicit unsupported result. Required rule
values come from `tax-rules`; posting and approval services enforce their own
state/rule/lock guards, independent of a successful profile result.

Observed scope: this evaluator covers the four implemented guidance scenarios,
not all profile combinations or tax obligations. The F-skatt fixture is an
engineering regression for onboarding, not independent legal validation or new
calculation/invoice eligibility coverage. Passing profile guidance does not imply
that an accounting transaction is supported or certify full tax compliance.
