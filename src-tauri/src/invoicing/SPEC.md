---
id: invoice-lifecycle
type: submodule-design
status: active
title: Invoice issuance credits and historical identity
parent: rust-backend
depends-on: [accounting-ledger, tax-rules, business-profiles, evidence-archive, workspace-defaults]
references: [storage-contract, product-goal, invoice-settlement, local-sqlite]
---

# Invoice lifecycle and historical identity

## Responsibility

Own drafts, numbered issuance, method-specific recognition authority, compensating
credits and immutable case provenance. The bounded cash flow also owns its receipt
transition while reusing settlement storage. Issuance and accounting recognition
remain distinct. `invoice-pdf-jobs` owns PDF production.

## Required invoice contract

`product-goal` owns supported currency and payment/credit cases. `tax-rules` governs
applicable calculation eligibility. Issuance cannot silently choose another method,
year, or unsupported VAT treatment.

`invoice-settlement` owns partial payments and outstanding balances.
Full credits preserve the source invoice and cannot disguise a partial credit or refund.

A credit requiring an excluded refund must stop with its evidence/history intact.

Method changes, legacy timing problems, and reviewed opening invoices follow
`business-profiles` and `accounting-ledger`. A PDF is not proof of correct
recognition or safe completion.

## Current boundary and gaps

Issuance rechecks tax/VAT posture, accounting method, issue-year rules, fiscal locks, and replay state
under an immediate transaction. Number allocation, case provenance, issue snapshot,
PDF-job enqueue, audit and any method-specific posting commit with the transition.
A preflight is guidance, not permission to skip transactional revalidation.
Invoice-timed entrypoints and credits retain `business-profiles`' restrictive timing
policy; separate cash entrypoints enforce their own reviewed case. Completed-result
recovery follows the preservation exceptions.

Planning tax status cannot issue. New standard invoices additionally require
`tax-rules`' bounded recognition case. Exempt, reduced/mixed-rate and other
uncovered cases now stop at eligibility, not a warning-only threshold permission.
Historical threshold calculations remain engineering regression coverage. The UI's
narrower draft capabilities belong to `ui-workflows`, not to the backend's line-input
contract.

For invoice-method issuance, guided, initially unanswered facts identify the ordinary domestic service,
completion date, prior receipts, year-wide no-EU circumstances, registered annual
VAT, expected taxable base within SEK 1 million, and ordinary timely invoicing.
The last confirmation includes consistent invoice-date treatment for incoming and
outgoing invoices throughout the year, and invoice date as the owner's sending
date. These are retained user declarations, not independently observed facts.

Preflight and issuance share an evaluator over actual workspace-scoped profiles,
lines, amounts, year and reviewed rules. Checked multiplication/addition and stored
header comparisons reject saturation, overflow or inconsistent amounts. A context
fingerprint binds the reviewed facts/date, relevant profile and rule payloads,
line identities and evidence digest. Preflight returns the actual reviewed invoice
snapshot (customer display identity, due date, checked line data and totals) from
that same transaction. The complete snapshot is fingerprint-bound and retained in
the immutable context; a stale list row is not the final review's data source.
It is a freshness check, not authorization: issuance recomputes all predicates
under its immediate transaction.

Invoice-method issuance atomically saves immutable recognition provenance with the
issue snapshot, number, voucher, PDF job, audit and replay result. `storage-contract` owns
row protection. Completed replay/already-issued recovery precedes new-case checks;
legacy invoices are not backfilled as eligible. Workspace-scoped provenance reads
use retained facts and identity, not current profile substitutions. `ui-workflows`
owns fact collection, stale-review behavior and source inspection.

Source-opening requests carry only compiled validation/source IDs. The native
handler requires an open workspace and resolves exact reviewed HTTPS sources;
renderer-supplied URLs, files and commands are not accepted. On macOS, a blocking
task invokes the system browser opener and reaps/checks its result rather than
relying on an unhandled webview new-window request. Unsupported platforms report
an error. Opening a source does not mutate accounting or validation records.

## Bounded cash identity and full receipt

`tax-rules` owns the case/year/calendar envelope and evidence lineage. Dedicated
cash commands do not extend the old invoice facts, assessment or settlement replay
shapes. First issuance requires affirmative owner review of a new 2026 business,
real start and December-ending first year, zero opening positions, no unpaid or
prior unrecorded activity, including outside the workspace. Actual posted/issued/
payment history must also be empty; a new file or zero balance is not proof.
Registered bokslutsmetoden, consistent incoming/outgoing cash treatment and both
separate turnover limits require explicit facts. Missing, uncertain or contradictory
required facts stop the transition. A shortened year needs its turnover-adjustment
review, not an inference from a low short-period total.

Preflight's deterministic prospective origin and actual invoice snapshot are
fingerprint-bound. The first successful issue atomically anchors the origin and
configures/audits the still-empty open fiscal year's real dates. It preserves
number, snapshot, PDF work and audit but creates no voucher, journal line, 1510,
revenue or output VAT; its accounting date is null. Proven later cash activity and
ordinary expenses are continuation, not automatically legacy. Every expense voucher
must match the immutable origin/year dates, original scoped expense-job provenance
and balanced ordinary expense lines. Optional bank staging adds link checks, not an
exemption from that common predicate. Contradictory unstaged history blocks both
new issue and receipt, just as contradictory staged history does. Unknown or mixed
income history cannot be grandfathered. `business-profiles` owns method freeze.

A receipt requires a verified retained external PDF plus explicit actual amount,
receipt date, SEK/received status, stable bank/account/transaction identity, source
locator, payer/invoice/account review and confirmation of unchanged case/no other
receipts or adjustments. Invoice total is comparison data, never proof of amount
received. Known generated invoice PDFs, including retained older job results,
cannot serve as bank evidence. The bounded structural PDF check and retained-byte
integrity do not authenticate a bank, extract facts by OCR or validate bank formats.
Raw owner additions remain separate from received bytes. `security-privacy` owns
isolated parser limits: parsing occurs before the accounting writer, then the same
source/hash is rechecked inside it; no unbounded parser runs in the app process.

The receipt writer rechecks original issue/origin, actual snapshot and checked
amounts, current relevant profiles/rule payloads, supported continuation, annual
commitments and actual receipt-date locks. Unpaid commitments count toward annual
base; payment replaces that commitment rather than counting a second sale. Private
UTC time and the source-backed 2026 civil-day data reject future events without a
renderer clock or generalized timezone rule.

One receipt reserves both its PDF content hash and bank tuple. A different entry
in the same PDF still cannot consume it again; changing the file or reference is
not a workaround. The proof/reservations, actual-date voucher, 1930 debit and
3041/2611 credits, staging/match, audit and completed replay commit together. No
1510 is created or cleared. Original issue identity, amounts and PDF are unchanged.
Exact-key matching recovery precedes current rules, profile, clock and evidence
checks; changed inputs or new-key source reuse do not receive cached success.
Read-only history verifies retained proof/link consistency without claiming fresh
byte verification when the PDF is unavailable.

Immutable cash-issued identity prevents invoice-timed settlement or credits even
if a current profile is externally edited. Cash CSV matching, partials, over/refunds,
credits, affected legacy/opening workflows, unpaid closing and cash VAT/year-end
approvals remain blocked. A paid invoice is not completed cash preparation.

## Historical identity and remaining gaps

Only drafts are editable through draft APIs. A source can receive one full credit.
The credit date cannot precede issue but can use a later open period/year.
Original VAT and historical identity remain intact.

The bounded flows do not establish general cash-method support, partial settlement
or guided opening/remediation. Their evidence does not establish all legal
invoice/PDF particulars; seller VAT-number/address and other document-completeness
coverage remain separate gaps.

Issue-time identity, tax posture, and rule source are snapshotted so later profile
changes cannot rewrite the issued PDF's meaning. Legacy invoices without that
snapshot require explicit historical evidence/attestation recovery, not guessed
values from current profiles. Snapshot storage protection lives in `storage-contract`.

Draft creation is not keyed, and some draft/audit writes are separate.
The transaction requirement remains `local-sqlite`. `accounting-ledger` owns the
fixed-account posting limitation.
