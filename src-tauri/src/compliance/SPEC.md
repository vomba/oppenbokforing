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
only three allowlisted scenarios: FA-skatt, low-turnover VAT exemption, and its
threshold breach. Those scenarios are embedded for packaged builds; broader
fixture loading is a source-test facility. Other golden scenarios test their
own domain implementations rather than expanding this evaluator automatically.

Unknown evaluator scenarios return an explicit unsupported result. Required rule
values come from `tax-rules`; posting and approval services enforce their own
state/rule/lock guards, independent of a successful profile result.

Observed limitation (acceptance unconfirmed): this evaluator covers only the
implemented scenario family. Passing it does not imply all profile combinations
or accounting transactions are supported.
