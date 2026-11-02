---
id: cash-planning
type: submodule-design
status: draft
title: Provisional cash-planning projection
parent: rust-backend
depends-on: [accounting-ledger, business-profiles, vat-returns]
---

# Provisional cash-planning projection

## Responsibility

Return a read-derived planning estimate for the dashboard without writing salary
assumptions into the business ledger. The user confirmed that the current formula
is a **provisional approximation**, not an approved definition of spendable bank
funds or tax liability.

## Boundary and current formula

The projection starts with bank account 1930 plus unpaid receivables on 1510,
then subtracts the estimated VAT reserve and preliminary tax reserve. The latter
is a hardcoded 30% of positive net revenue on account 3041 plus, for FA-skatt,
30% of a positive expected salary assumption. Expenses are not subtracted from
that revenue base. The expected-business-profit input is not the formula's base.
`accounting-ledger` defines the revenue helper's actual meaning.

The output is not a filing calculation, a bank balance, or proof that receivables
have been collected. VAT reserve/threshold values come from the existing VAT
service rather than a duplicate renderer calculation. This module does not use
the versioned tax rules to determine its 30% approximation.

Observed date limitation (acceptance unconfirmed): revenue uses the supplied rule
year while the VAT-monitoring period uses today's UTC date. A historical year
request is therefore not a wholly historical cash snapshot.
