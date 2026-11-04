# 2026 domestic service invoice recognition — exact VAT

Validation ID: `invoice-recognition-se-service25-2026-exact-v1`.
Candidate rule version: `rv-2026-domestic-service25-exact-v1`.
Status: **implementation mechanically verified; final independent implementation review pending**.
Source retrieval: 2026-11-04. This is not a professional opinion or a claim that complete books, returns, settlements, credits, or generated PDFs are legally validated.

## Applicability and declarations

New standard SEK invoices only: an ordinary taxable service supplied domestically in Sweden, completed during 2026 no later than issuance, no advance or previous receipt, no special VAT treatment, invoice-method accounting. The seller is VAT registered (including voluntary registration only when effective for the supply). The invoice is issued in the ordinary agreed time, not a late invoice requiring earlier recognition. The invoice-date method is used consistently throughout the accounting year for both incoming and outgoing invoices, and the selected invoice date is the date the owner sends the invoice. The whole accounting year's circumstances permit annual VAT reporting without EU trade under `annual_may_12`. Annual taxable base is no more than SEK 1,000,000; the user's registration/expected-base answer cannot be inferred from profit or residency defaults.

Transaction classification, completion date, prior-payment answer, ordinary/consistent invoice timing and sending date, and year-specific annual/no-EU circumstances require explicit answers. Omitted/uncertain answers are unknown, not favorable defaults. The backend must separately verify stored method, profile, year, actual lines/accounts, amounts, rule values and evidence identity.

Only exact VAT in minor units is covered. Each line's net öre must be divisible by four. No policy for rounding fractional öre is validated. This deliberately excludes even lines whose fractions cancel only in the invoice total. The user approved this restriction rather than relying on inaccessible current rounding guidance. Do not suggest altering a commercial price to bypass it.

## Retained authoritative passages

The following short extracts were obtained from the named official publications, not from application output. PDF extracts were read from the retrieval tool's local extraction; they are not claimed to be hashes of original PDF bytes. Full URLs and pinpoint references identify the source for rechecking.

1. **Bookkeeping timing and split.** Bokföringsnämnden, *Vägledning Bokföring*, BFNAR 2013:2, updated 2024-09-16, paragraph 3.4, printed page 34; effective provisions printed page 98. <https://www.bfn.se/wp-content/uploads/vl13-2-bokforing.pdf>
   - “Bokföring av fordringar och skulder får anstå till dess faktura eller motsvarande handling har utfärdats eller mottagits eller sådan handling enligt god affärssed borde ha utfärdats eller mottagits.”
   - “En faktura anses utfärdad när den skickas från företaget och mottagen när den kommit till företaget. Fakturadatum anses sammanfalla med den dag fakturan skickas.”
   - At printed page 21: taxable sales are split between income and output VAT when recorded.
   - Applicability: general bookkeeping guidance effective before 2026; supports invoice-date receivable/revenue/VAT recognition for the bounded ordinary, non-late case. It does not justify deferring an overdue invoice into a later period.

2. **2026 invoice-method example and annual reporting limit.** Skatteverket, *Bokföring, bokslut och deklaration*, SKV 282 edition 9, published February 2026 (printed page 56), pages 20 and 54. <https://www.skatteverket.se/download/18.9567cda19bf6c0027216f6/1770379535285/bokforing-bokslut-och-deklaration-skv282utgava09.pdf>
   - Page 20: “Om du väljer att använda faktureringsmetoden ska du redovisa moms i din momsdeklaration i den period du har fått eller har upprättat en faktura.”
   - Page 54: a SEK 10,500 invoice consists of SEK 8,400 sale plus SEK 2,100 VAT and is booked as a receivable. Receipt later removes the receivable; it is not a second sale.
   - Page 20 permits annual reporting for a small business with an estimated taxable base no more than SEK 1 million.
   - Applicability: expressly published in 2026. The source's illustrative account numbers are not a requirement to use this application's exact BAS subaccounts. The verified economic roles are receivable, service revenue and output VAT; existing chart mappings 1510/3041/2611 implement those roles.

3. **2026 standard VAT rate.** Skatteverket, *Belopp och procentsatser för inkomståret 2026*, dated 2026-01-07, page 2. <https://www.skatteverket.se/download/18.1522bf3f19aea8075ba3285/1767885159120/belopp-och-procentsatser-for-inkomstaaret-2026.pdf>
   - “25 procent (20 procent av priset), generella skattesatsen för moms.”
   - Applicability: 2026 standard-rate sales only. This does not classify arbitrary services; the explicit ordinary domestic/no-special-treatment facts remain required.

4. **Annual no-EU deadline.** Skatteverket, *När ska jag deklarera moms*, section “Enskilda näringsidkare och dödsbon utan EU-handel”. <https://www.skatteverket.se/foretag/moms/deklareramoms/narskajagdeklareramoms.4.6d02084411db6e252fe80008988.html>
   - “En enskild näringsidkare eller ett dödsbo som ska lämna inkomstdeklaration och inte har handel med andra EU-länder ska redovisa och betala moms för beskattningsåret senast den 12 maj året efter beskattningsårets utgång.”
   - Applicability: annual VAT, no EU trade, no claimed extension. For accounting period 2026 this gives **2027-05-12**, not 2026-05-12. The latter is not validated by copying the existing engineering calendar seed. This date is a Wednesday, so no weekend adjustment is needed for this one case.

5. **Invoice facts and limitation of this gate.** Skatteverket, *Momslagens regler om fakturering*, “Fakturans innehåll”. <https://www.skatteverket.se/foretag/moms/saljavarorochtjanster/momslagensregleromfakturering.4.58d555751259e4d66168000403.html>
   - Required content includes supply/completion date when determinable and different from invoice date, seller VAT number, seller/buyer names and addresses, tax bases/rate/amounts.
   - Applicability: this gate must retain/display the confirmed completion date and cannot claim full PDF compliance. Address/VAT-number completeness is a separate existing gap, not waived by recognition eligibility.

Current legal rounding guidance returned “Request Rejected”, including through a separate browser. Older indexed snippets were not promoted to validated 2026 rounding coverage. No other rule/year/method is inferred from these sources.

## Independent derivation

The companion JSON was written from these sources and arithmetic before implementing the gate. It was not generated from the Rust posting or VAT services.

At 25%, VAT is one quarter of the net amount. In integer öre, net 1,000,000 / 4 = VAT 250,000, and gross = 1,250,000. The receivable debit equals gross; service revenue and output VAT credits sum to it. There is no bank movement at issuance. The same derivation gives net 4 / VAT 1 / gross 5; quantity 2 times unit 2 öre and quantity 4 times unit 1 öre each produce the same exact result. Two lines with net 8 and 12 produce VAT 2 and 3, respectively, totalling net 20 / VAT 5 / gross 25.

Net 1, 2 or 3 öre produces VAT 0.25, 0.5 or 0.75 öre and is **outside coverage**, not rounded. A 1-öre and 3-öre pair remains outside coverage despite summing to an exact total. Signed-64-bit overflow cannot authorize saturated values: both net multiplication and total/gross addition must be checked. The JSON contains explicit overflow examples.

For a service completed and invoiced on 2026-12-31, the recognized date is 2026-12-31 and the annual period is 2026. Accounting year and filing year are distinct: 2026 + 1 = 2027, due May 12. Completion after issuance, completion in 2025, or issuance in 2027 is not this case. Fractional rounding, advances, special regimes and other calendars are deliberately negative scenarios.

## Independent evidence review

On 2026-11-04, the read-only reviewer session `01a1a5ed-fc8c-7355-a36b-d6b6b61afbbb` performed an **AI evidence/coverage review**, not professional or tax-authority attestation. Verdict: conceptual evidence approved, conditional on implementation checks. The reviewer verified the three local official publication extracts, recomputed every JSON amount example independently with arbitrary-precision integers/rational fractions, and verified the filing date's weekday. The annual deadline review relied on the official fetched-text representation because its fresh retrieval reset; that limitation remains explicit.

The review requires explicit classification/registration/annual/no-EU/expected-base facts; ordinary timely and consistently applied invoice timing, with invoice date meaning sending date; per-line exactness and checked arithmetic; current profile/line/rule binding; atomic provenance; positive and negative execution tests, including earlier same-year completion. Those are implementation obligations, not completed execution verification. No broader case is approved.

## Activation and execution verification

Mechanical execution verification on 2026-11-04:

- The original-DTO missing-facts regression failed behaviorally against archived `44f897e`; its source/log and pre-gate posting shapes are retained under `fixtures/historical-regressions/`, separately from this independent evidence.
- The current public-boundary suite exercises the independent exact-VAT journals and date/calendar examples, each required fact, stale profiles/lines/rules/facts/dates, unsupported cases, line/aggregate overflow, annual limits, atomic issuance and immutable provenance, low-level bypasses and writer-lock races.
- A focused security audit found that lexicographic comparison could accept signed future registration date `+2027-01-01`. A public profile/preflight/issuance regression failed before the fix; parsed-date comparison made it pass. No broader profile-format policy was inferred.
- A real first-20-migrations database upgrades normally to migration 21 without changes to historical posting/audit/replay data or a fabricated eligibility backfill. Separate close/reopen and encrypted backup/restore checks preserve new immutable provenance and retained PDF bytes. Both persistence tests pass.
- The public CI invocation passed fixtures, golden/M8 gates, **255 Rust tests**, **146 frontend tests**, and the TypeScript/production build. Its final HEAD-diff binding check currently reports the legitimate uncommitted DTO change; generator output was independently regenerated twice and is byte-for-byte stable. The complete gate will be rerun after the implementation commit.
- Desktop compilation and integration-mode smoke pass. New-file Rust formatting and spec validation pass. Packaged GUI install/launch UAT is not verified.

Final independent implementation review: **pending**. Its disposition and the post-commit public gate must be recorded before handoff. These results do not establish excluded fractional rounding or whole-PDF/books/return validation.

A source citation, machine digest, active rule row or passing regression alone does not activate coverage. Before enabling this case, a fresh independent reviewer must inspect these sources/extracts, derivations, exclusions and test results; record its findings and disposition here. Do not invent a human/professional sign-off. Record the actual review identity and scope.

The implementation must bind the exact validation-package digest and relevant typed rule values to the current transaction's inputs and persist that provenance atomically with issuance. New rule versions must preserve historical identities. Existing unsupported invoices are historical engineering regression fixtures, not retrospectively validated cases.
