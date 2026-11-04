# 2026 domestic service invoice recognition — exact VAT

Validation ID: `invoice-recognition-se-service25-2026-exact-v1`.
Candidate rule version: `rv-2026-domestic-service25-exact-v1`.
Status: **bounded recognition gate verified and independently reviewed** (implementation `5916918`).
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

The review requires explicit classification/registration/annual/no-EU/expected-base facts; ordinary timely and consistently applied invoice timing, with invoice date meaning sending date; per-line exactness and checked arithmetic; current profile/line/rule binding; atomic provenance; positive and negative execution tests, including earlier same-year completion. Execution verification was still pending at that evidence review; the implementation results below address those obligations. No broader case is approved.

## Activation and execution verification

Mechanical execution verification on 2026-11-04:

- The original-DTO missing-facts regression failed behaviorally against archived `44f897e`; its source, tracked `missing-facts-red.txt` transcript and pre-gate posting shapes are retained under `fixtures/historical-regressions/`, separately from this independent evidence. The transcript is present in commit `5916918` and is not lost to the generated-log ignore rule.
- The current public-boundary suite exercises the independent exact-VAT journals and date/calendar examples, each required fact, stale profiles/lines/rules/facts/dates, unsupported cases, line/aggregate overflow, annual limits, atomic issuance and immutable provenance, low-level bypasses and writer-lock races.
- A focused security audit found that lexicographic comparison could accept signed future registration date `+2027-01-01`. A public profile/preflight/issuance regression failed before the fix; parsed-date comparison made it pass. No broader profile-format policy was inferred.
- A real first-20-migrations database upgrades normally to migration 21 without changes to historical posting/audit/replay data or a fabricated eligibility backfill. Separate close/reopen and encrypted backup/restore checks preserve new immutable provenance and retained PDF bytes. Both persistence tests pass.
- Final post-commit `npm run ci:public` passed fixtures, golden/M8 gates, **262 Rust tests**, **152 frontend tests**, TypeScript/production build and the generated-binding HEAD-diff check. The earlier legitimate pending DTO diff is no longer a failed gate.
- The final review now displays the actual customer, due date, lines and checked totals returned by preflight, bound in its fingerprint and immutable context. Backend and component regressions reproduced missing/stale snapshots before the fix; changed customer identity also invalidates confirmation.
- Source actions dispatch only compiled validation/source IDs to the native macOS browser opener. Tests verify exact command construction, off-async-worker execution, denied arbitrary IDs/URLs, navigation prevention and visible launch failures. No browser was opened by automated tests; other platforms fail explicitly as unsupported.
- Desktop compilation and integration-mode smoke pass. New-file Rust formatting and spec validation pass. At the `5916918` handoff, packaged native and actual browser-launch UAT were **not yet verified**; the later bounded runtime addendum below records the completed subset and remaining exclusions.
- Strict Clippy remains blocked by pre-existing findings: 9 library diagnostics, 16 including library tests. One obsolete preflight-builder argument-count diagnostic disappeared during the gate refactor; no suppressions were added to hide the remaining debt.

## Independent implementation review and disposition

The independent implementation review of `5a0bf15` requested corrections for stale final-review amounts, nonfunctional webview source links and the ignored RED transcript. After those fixes, re-review of **`59169189bf6d` approved** the change and independently reran 262 Rust tests, 152 frontend tests, build, desktop, formatting and transcript checks. This is an AI implementation review, not a human/professional or tax-authority attestation. Its remaining manual browser/package UAT limitation is preserved above.

## Follow-up packaged native verification

The [2026-11-04 packaged macOS record](../verification/packaged-macos-2026-11-04.md)
retains an initial native failure, the test-first correction and a separately
identified corrected app/DMG. The real run exposed registered F-skatt incorrectly
selecting FA salary guidance. Revision `47369a1` fixes that profile-only
classification; FA salary checks and this invoice-recognition policy remain intact.
Independent review passed 267 Rust / 158 frontend tests for that fix.

The corrected unsigned app was copied from the read-only DMG into disposable
Applications/data directories and launched outside the repository. Native controls
verified ordinary F-skatt setup with zero salary, missing-fact stops, stale-review
rejection without posting/number consumption, the updated SEK 2,500 review,
2026-0001 issuance, retained PDF bytes and cold workspace reopening with the same
immutable facts/context/fingerprint. A pointer click on the annual/no-EU reference
was confirmed against that public Skatteverket document path in Firefox without
recording other URLs or page content.

This is bounded functional UAT, not signed/notarized distribution or
Finder/Gatekeeper admission. Visual PDF correctness and the BFN PDF's actual
viewer display were not established. A native sandbox-extension diagnostic is
retained rather than hidden; no OS security bypass or application test-mode hook
was used. Other methods/years/cases and full product/release readiness are not
established by that run.

The source review, independent expected arithmetic, public-boundary execution checks and recorded implementation review support only the declared exact-VAT case. A citation, digest, active row or regression pass alone is not the basis for eligibility. Excluded fractional rounding and whole-PDF/books/return validation remain unestablished.

The implementation binds the exact validation-package digest and relevant typed rule values to actual transaction inputs and persists the provenance atomically with issuance. New rule versions preserve historical identities. Existing unsupported invoices remain historical engineering regression fixtures, not retrospectively validated cases.
