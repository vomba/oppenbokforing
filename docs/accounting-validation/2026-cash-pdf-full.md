# 2026 cash-method identity and full PDF receipt evidence

Status: **source review approved; implementation under independent review; packaged cash verification pending**.
Source evidence prepared 2026-11-04. Automated checks and reproduced review defects
are recorded below; this is not a packaged-runtime or full-method completion claim.

- Cash validation ID: `cash-recognition-se-service25-2026-pdf-full-v1`.
- Shared schema-22 active rule version for the two live cases: `rv-2026-method-recognition-v2`.
- Invoice successor ID: `invoice-recognition-se-service25-2026-exact-v2`.
- Cash manifest: [2026-cash-pdf-full.json](../../fixtures/accounting-validation/2026-cash-pdf-full.json).
- Invoice compatibility manifest: [2026-domestic-service25-exact-v2.json](../../fixtures/accounting-validation/2026-domestic-service25-exact-v2.json).
- Reviewed cash manifest SHA-256: `08d21e5dc4a055698b6a180e665141770bdb7f2c7325eea4e6d5bee4d54e7f7b`.
- Reviewed invoice-v2 manifest SHA-256: `5dbd4e6bd327f01112179ce325f086d9717f738d976065fbf4ec2268edd58cec`.

The source expectations were independently prepared before the implementation;
they are not implementation output, professional advice, professional sign-off or
Skatteverket/BFN attestation. Runtime DTOs and policy enforcement are separate from
the immutable manifest's descriptive oracle vocabulary. An active schema row or
this dossier alone does not establish completed verification or broad support.

## Approved boundary and relation to current specifications

The accepted limits are retained in `invoice-lifecycle` and `tax-rules`:
**PDF receipts first, new businesses only**, including reviewed short first years
ending December2026. The temporary design task is not the durable authority.
Native invoice-v1 UAT is complete, the F-skatt guidance correction is recorded,
and the preceding 267 Rust / 158 frontend results are not evidence that cash works.
The shared invoice-timed posting/approval guard remains restrictive. The separate
cash commands implement only the bounded exception described by the owning specs;
they do not make old cash settlement or approval routes generally permissible.

Only a Swedish sole trader's ordinary completed domestic SEK service at 25% is
proposed, in calendar accounting year 2026, with exact VAT on every line, no advance
or prior receipt, timely ordinary issuance, and a full actual receipt in that same
year. VAT registration must be effective for the supply; the bookkeeping cash
method and registered VAT bokslutsmetoden must be explicit and consistent, not
inferred from an invoice-date declaration. Incoming and outgoing treatment must
be consistent. Ordinary issue date means the owner's sending date. Required
answers start unknown; omitted, uncertain or contradictory facts stop the action.

Annual VAT registration, estimated annual taxable base at most SEK 1,000,000 and
no EU trade for the whole accounting year are separate explicit facts. Annual
`annual_may_12` applies only to the stated income-tax-return/sole-trader case,
without an extension. Its reference date for accounting period 2026 is
**2027-05-12**, not 2026-05-12. No cash VAT-return or year-end approval is enabled,
even if every supported invoice is fully paid. New-business-only, PDF-first,
per-line exactness, full same-year receipt and approval blocking are conservative
user-approved validation limits, not claims that the sources forbid every other
cash-method business or lawful verification. This is not full cash-method,
closing, return or PDF-compliance validation. Required broader years/cases remain
implementation work, not new product-level exclusions.

Cash-method eligibility must separately establish normal annual **net turnover
at most SEK 3,000,000**. That is not the annual-VAT taxable-base test. Neither
answer, profit, bank balance, a default method nor a residency default proves the
other. Exactly SEK 3,000,000 and SEK 1,000,000 satisfy the respective source limits;
exceeding either or having an unknown relevant amount is outside this case.
SEK 3,000,000 = 300,000,000 öre; SEK 1,000,000 = 100,000,000 öre. A hypothetical
SEK 2,000,000 taxable base can satisfy the cash turnover limit but not this annual
VAT envelope; one eligibility permission cannot substitute for the other.
A short first year starting in2026 and ending December31 is included only with
explicit review of the **twelve-month-adjusted normal turnover** required by
BFN1.7. A low short-period total alone cannot satisfy this fact. The owner's
adjustment review starts unknown and is required for a shortened first year;
this is not an automatic annualization calculator or a professional attestation.
Extended years and other year-ends remain outside this case.

## Source-date clarification

The initial evidence preparation occurred on2026-11-04. The resumed first-year,
SFL and annual-deadline retry occurred on2026-11-06. Their manifest `retrievedOn`
annotations retained the preparation date rather than the later retrieval date.
The published manifest bytes and digest remain unchanged. This paragraph records
that metadata error openly. It does not claim a November4 fetch for those later
checks or change the cited editions, passages or numerical expectations.

## Retained authoritative passages and exact applicability

The provided local official-text extracts were read in bounded relevant sections,
not used as application outputs. Whitespace/line-break hyphenation is normalized
in quotations. These extracts identify publications and printed pages, **not
verified original PDF-byte digests**. The rate is inherited from invoice-v1.
Fresh retrieval is claimed only for the explicitly identified first-year,
annual-deadline, bounded SFL and civil-clock sources below; the historical v1
retrieval record and bytes remain unchanged.

### 1. Bookkeeping permission, timing and evidence order

Bokföringsnämnden, *Vägledning Bokföring*, BFNAR 2013:2, updated 2024-09-16:
<https://www.bfn.se/wp-content/uploads/vl13-2-bokforing.pdf>.
Local extract: `/Users/vomba/Downloads/vl13-2-bokforing.md`.

- Printed pages 32 and 40, reproduced **BFL 5 kap. 2 § third paragraph**:
  “Ett företag vars årliga nettoomsättning normalt uppgår till högst tre miljoner
  kronor får dröja med att bokföra affärshändelserna tills betalning sker. Vid
  räkenskapsårets utgång ska dock samtliga då obetalda fordringar och skulder bokföras.”
- Printed page 40, **paragraph 3.14**:
  “Ett företag, som med stöd av 5 kap. 2 § tredje stycket bokföringslagen (1999:1078)
  tillämpar kontantmetoden, ska i avvaktan på bokföring ordna verifikationerna i takt
  med att de mottas eller sammanställs.”
- Printed page 41, commentary: “Uppkomna fordringar och skulder behöver således
  inte bokföras löpande.” It immediately explains that supporting verifications
  must still exist and be kept in order. **Paragraph 3.15** measures permitted
  timing for postponed bookkeeping from payment, not invoice issuance.
- Printed page 98: BFNAR 2013:2 applies to fiscal years beginning next after
  2013-12-31; BFNAR 2024:1 applies from 2024-09-16. This is applicable pre-2026
  general bookkeeping guidance, not a publication limited to tax year 2024.
- Printed page 11 defines net turnover net of VAT and directly connected tax;
  paragraph **1.7** requires adjustment for a fiscal year other than twelve months.

**Applicability:** these passages support no running receivable/revenue/VAT journal
at cash invoice sending and recognition at actual receipt, with ordered retained
invoice/evidence meanwhile. They do **not** eliminate year-end unpaid obligations.
The source permits deferral of accounting; it does not call the numbered invoice
a bank receipt, prescribe an app's numbering/job architecture, or mandate these
specific BAS subaccounts. Identity-only issuance is the approved implementation
boundary to preserve traceability without inventing invoice-date recognition.
No claim about the maximum permitted bookkeeping delay is necessary: the oracle's
accounting date is the actual event date, not the date the owner later enters it.

### 2. Cash VAT, method contrast, annual base and publication year

Skatteverket, *Bokföring, bokslut och deklaration*, **SKV 282 edition 9**:
<https://www.skatteverket.se/download/18.9567cda19bf6c0027216f6/1770379535285/bokforing-bokslut-och-deklaration-skv282utgava09.pdf>.
Local extract: `/Users/vomba/Downloads/bokfring-bokslut-och-deklaration-skv-282-utgva-9.md`.
Printed page 56 states “SKV 282 utgåva 9. Utgiven i februari 2026.”

- Page 8, *Kontantmetoden*: “Under löpande år bokför du alltså inte när du skickar
  en faktura till en kund eller när du får en faktura från en leverantör utan först
  när det sker en betalning.” It expressly includes bank-account payments.
- Page 10 reproduces the normal annual net-turnover limit of SEK 3 million and
  describes retaining invoice and bank-payment supporting information.
- Pages 14–15: a SEK 10,500 invoice sent 2 November (net SEK 8,400 + VAT SEK 2,100)
  has **no posting at sending**; receipt on 30 November increases bank by 10,500,
  sales by 8,400 and output VAT by 2,100. The later daybook entry is the receipt.
- Page 20, *Boksluts- eller faktureringsmetoden*: “Bokslutsmetoden är enklare att
  tillämpa. Enligt denna ska du redovisa moms vid betalningstillfället. Du ska också
  redovisa moms på de obetalda kundfordringar och leverantörsskulder som du har vid
  årets utgång.” The text states that this corresponds to bookkeeping's cash method.
- Page 20, *Momsregistrering*: registration includes which VAT method and reporting
  period the business chooses. A stored bookkeeping method is not a substitute
  for confirming applicable VAT registration/method.
- Page 20, *Redovisningsperioder*: “Är du ett litet företag med ett beräknat
  beskattningsunderlag (försäljning) på högst en miljon kronor kan du välja att
  redovisa moms för varje kalendermånad, kalenderkvartal eller årsvis.”
- Page 20, *Bokslutsmetoden och årsmoms*: running entries use VAT when paid; closing
  adjusts for opening and closing receivables/payables to avoid double reporting.
- Page 54 expressly contrasts the cash example on pages 9–23 with invoice-method
  receivable at sending and clearance on payment, and states the SEK 3 million
  cash-method limit. That **1510-style clearance is not this cash receipt oracle**.

**Applicability and reconciliation:** edition 9 is explicitly published in 2026,
for sole traders' bookkeeping and simplified closing. Its account examples use
1940/3000/2610; the independently expected roles map to this app's bank/service
revenue/output VAT accounts **1930/3041/2611**, not a statutory account-number claim.
Page 14 also contains the explanatory phrase “När Berit skickar ut en faktura blir
det utgående moms.” Read in isolation, that could suggest invoice-date VAT.
It is not used as timing authority: the same example explicitly has no posting at
sending, and page 20 explicitly sets cash VAT at payment with a closing exception.
This wording limitation is recorded rather than silently choosing the sentence
that resembles implementation. Neither the example nor the year-end instructions
validate this app's cash closing; unpaid closing remains excluded and approvals
remain blocked.

### 3. Verification contents and PDF/source boundaries

BFN guidance above, printed pages **47–50**, BFL **5 kap. 6–7 §§**, paragraphs
**5.4–5.6**:

- Page 47: “Mottagna uppgifter ska alltid användas som verifikation.” It explains
  that electronic format, e.g. PDF, is part of electronically received information.
- Page 49: “Verifikationen skall innefatta uppgift om när den har sammanställts,
  när affärshändelsen har inträffat, vad denna avser, vilket belopp den gäller och
  vilken motpart den berör.”
- Page 49: “I verifikationen skall det ingå ett verifikationsnummer eller annat
  identifieringstecken samt sådana övriga uppgifter som är nödvändiga för att
  sambandet mellan verifikationen och den bokförda affärshändelsen utan svårighet
  skall kunna fastställas.” Paragraph 5.4 requires durable information.
- Page 50 explains that the required facts can be in multiple linked documents;
  paragraphs 5.5–5.6 require identification of the event, its meaning and counterparty.
- SKV 282 page 11 lists corresponding verification facts and linkage; page 16
  describes reconciling actual bank-statement entries with bookkeeping.

**Applicability:** source-backed event/date/amount/counterparty traceability
supports requiring an identifiable actual bank receipt, retained source and
explicit owner-provided facts. It does **not** prove that an arbitrary bank PDF
layout was parsed or authenticated. The chosen **external bank PDF only** gate
is a conservative user-approved product restriction, not a claim that the law
forbids other valid verifications (BFN 5.2 also discusses self-prepared ones).
The original retained source and additions must remain distinguishable.

The PDF may contain several transactions. Require a stable owner-provided **bank
reference/source identity**, business bank-account and payer context, plus an
entry locator where needed, so a reviewer can trace **one actual full receipt**
to **one issued invoice**. Filename, document ID, MIME type or file hash alone
cannot distinguish bank entries; an invoice number alone is not a bank receipt.
The sources do not forbid separate statement entries from supporting different
invoices. The user nevertheless selected **one cash receipt per retained PDF
content hash** for this first slice. Even another genuine entry with different
typed identity cannot consume that PDF again. A separate external bank PDF is
required for another receipt, and a stable bank/account/transaction identity also
prevents recognizing the same actual receipt from a newly exported/repackaged PDF.
Matching completed replay is result recovery, not another source consumption.
Do not suggest editing/cropping the received source or inventing references to
bypass either restriction.
Ambiguous/no stable identity or otherwise unsupported evidence must stop, not
receive a guessed transaction identifier. No named-bank format or bank-source-row
validation is established by these sources or the chosen PDF path.

Retained byte/workspace verification establishes integrity of the archived object.
**Source PDF byte integrity plus explicit owner data is NOT bank authentication,
OCR, automatic statement extraction or independent observation of receipt.**
Require explicit **actual amount, actual date, received confirmation, source SEK
currency confirmation and receipt identity**. Do not prefill invoice gross as
proof of money received. Scheduled transfers, the generated invoice PDF and a PDF
whose only significance is its filename are not actual bank evidence. Unknown or
foreign source currency stops even if the invoice itself is SEK. Missing/tampered
bytes or unsupported/contradictory source contents stop receipt recognition.

### 4. New-business opening review

SKV 282 page **6**, *Öppningsbalansräkning*:
“När du startar din verksamhet eller när du börjar tillämpa reglerna om förenklat
årsbokslut måste du göra en öppningsbalansräkning.” The adjacent *Nystartad firma*
text requires deciding which assets and liabilities belong to the business.

**Applicability:** new business does not mean no opening positions. This first
case requires explicit reviewed **zero opening assets/liabilities/equity**, no
opening unpaid receivables/payables, and no prior unrecorded business activity,
including outside the workspace (apart from the declared supply being issued).
If opening assets, debts, unpaid items or prior activity need treatment, this case
cannot invent zero, a journal, or retrospective coverage. It is not an implemented
opening-balance workflow. No mandatory professional review is imposed.

At the **first** cash issue, the actual accounting/issuance history must also be
empty of prior posted vouchers and issued invoices. Preexisting posted/issued
legacy origin is blocked **even with zero current balances or zero outstanding**.
A fresh database is not proof of business origin; positive owner facts alone
cannot override contradictory stored history. Drafts and retained unposted
sources alone neither establish nor disprove the facts.

After an eligible first issue, later invoices/receipts may rely on its immutable
qualifying origin plus uninterrupted eligible cash continuation. They are not
legacy merely because the workspace now has valid issues or receipts. Do not
require perpetual emptiness or a zero bank balance. Conversely, unknown or mixed
history is not continuation. Preserve method/case origin at issue; a settings
change before the first receipt/voucher must not turn identity-only cash invoices
into invoice-method receivables. Dedicated method transitions remain outside scope.

### 5. First financial year ending in December 2026

The user explicitly included businesses started during 2026 whose first financial
year ends **2026-12-31**, as well as January starts. Extended years and other
financial-year ends remain excluded. This corrects the candidate's initial blanket
shortened-year exclusion; it does not enable opening entries or closing/approvals.

Freshly retrieved Skatteverket *Allmänna uppgifter*, *Räkenskapsår* states:
“Du som har enskild näringsverksamhet ska ha kalenderår som räkenskapsår, det vill
säga från 1 januari till 31 december. Undantag gäller när näringsverksamheten
startas eller avslutas.” Its maximum 18-month startup allowance is not permission
for this case to cover an extended year. SKV282 edition9 page43 explicitly calls
its October-start example a shortened financial year.
<https://www.skatteverket.se/foretag/inkomstdeklaration/deklareraenskildnaringsverksamhet/fyllainebilagan/allmannauppgifter.4.361dc8c15312eff6fd2c77e.html>.

The freshly extracted SFL2011:1244, **26kap11§**, measures the estimated VAT
taxable base **for the tax year** and provides the one-million threshold for the
annual period. Sections10–13 also show why actual annual registration cannot be
inferred just from turnover: shorter reporting periods may apply. The tool warned
that it processed only the first part of the long statute; the quoted material
includes these sections. This is not a claim to have searched the whole law or
all exceptions. No old 2004 guidance/search hit is used as current governing law.
<https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/skatteforfarandelag-20111244_sfs-2011-1244/>.

Require the real start date in2026, no later than the declared completed service,
and explicit first-year end2026-12-31. Retain those facts immutably. At the first
successful issue, configure/audit the still-empty workspace financial-year row to
those dates atomically rather than export a fabricated January start. Never
rewrite a financial year containing posted history. Calendar metadata2026 and
May2027 are reference/reporting-year identity, not proof that every business
started January1. The normal-annual-turnover cash test and tax-year VAT base remain
distinct. Require explicit short-year turnover-adjustment review rather than
inference from the short-period total; BFN1.7 is authoritative for that review.
First-year examples cover full/short positive and incompatible, unreviewed or
unknown negative cases; they do not validate depreciation or closing calculations.

### 6. Official 2026 rate and annual/no-EU calendar evidence

These source passages were already retained in the published invoice-v1 dossier
and manifest, not inferred from engineering rule seeds. The rate is inherited;
the deadline page was freshly retrieved successfully for this cash case after a
retry. Neither the published v1 manifest nor its old retrieval record is changed:

- Skatteverket, *Belopp och procentsatser för inkomståret 2026*, dated 2026-01-07,
  page 2, *Mervärdesskatt (moms)*:
  <https://www.skatteverket.se/download/18.1522bf3f19aea8075ba3285/1767885159120/belopp-och-procentsatser-for-inkomstaaret-2026.pdf>.
  “25 procent (20 procent av priset), generella skattesatsen för moms.”
  Applies to 2026 standard-rate supplies; it does not classify all services as 25%.
- Skatteverket, *När ska jag deklarera moms*, *Enskilda näringsidkare och dödsbon
  utan EU-handel*:
  <https://www.skatteverket.se/foretag/moms/deklareramoms/narskajagdeklareramoms.4.6d02084411db6e252fe80008988.html>.
  “En enskild näringsidkare eller ett dödsbo som ska lämna inkomstdeklaration och
  inte har handel med andra EU-länder ska redovisa och betala moms för
  beskattningsåret senast den 12 maj året efter beskattningsårets utgång.”
  For the declared annual 2026 case: 2026 + 1 = 2027; May 12 is a Wednesday.
  The old v1 dossier's earlier retrieval-reset record remains historical. This
  cash task's successful fresh retrieval confirms the quoted passage. No
  extension/other calendar is inferred.

## Independent arithmetic and recognition oracle

These literal expectations were derived from the source rules and accounting
roles, not generated by Rust/frontend posting, an exported journal or a new cash
implementation. The compatibility invoice copy is explicitly different: lineage,
not a newly independent invoice validation.

For the ordinary fixture, service completion is **2026-01-10**, sending/issue is
**2026-01-15**, and the actual single full receipt is **2026-01-20**.

- Net: 1 × 1,000,000 öre = 1,000,000 öre (SEK 10,000).
- At 25%, VAT = 1,000,000 / 4 = 250,000 öre (SEK 2,500).
- Gross = 1,000,000 + 250,000 = 1,250,000 öre (SEK 12,500).
- At issue: numbered issued invoice, retained issued PDF and immutable audit/case
  provenance; **zero vouchers, zero journal lines, no 1510, no revenue posting,
  no 2611 VAT liability and no bank movement**. Retaining an issued invoice is not
  a receipt stage, bank proof, or journal verification number consumption.
- At actual full receipt: **one voucher / three lines dated 2026-01-20**:

  | Account / role | Debit öre | Credit öre |
  | --- | ---: | ---: |
  | 1930 bank | 1,250,000 | 0 |
  | 3041 domestic service revenue | 0 | 1,000,000 |
  | 2611 output VAT 25% | 0 | 250,000 |

  Debits = credits = 1,250,000. **No 1510 entry/clearance** exists in either stage.
  Receipt proof and bank identity/staging are separate from issued identity/proof.
  Never reprice the invoice, replace its immutable lines/amounts/method, or move
  recognition back to issue, completion, due date or later import-entry date.

For each line, calculate quantity × unit-price net using checked integer arithmetic;
at 25% the **line net** must be divisible by 4. Quantity 1 at 4 öre, quantity 2 at
2 öre and quantity 4 at 1 öre each give net 4 / VAT 1 / gross 5. Separate line nets
8 and 12 yield VAT 2 and 3: net 20 / VAT 5 / gross 25. Exactness concerns line net,
not unit price in isolation, and not just the aggregate invoice.

Line nets 1, 2, 3 öre give VAT 0.25, 0.5, 0.75 öre and are **blocked**, not rounded.
Separate 1-öre and 3-öre lines are still blocked despite exact aggregate VAT.
Unresolved rounding guidance is not filled by a convenient implementation rule;
do not recommend price changes to bypass the boundary.

Signed-64-bit minor-unit bounds must be checked on line multiplication, line VAT
and gross, aggregate net/VAT/gross and journal amounts. Quantity 2 ×
9,223,372,036,854,775,807 overflows net. Net 9,223,372,036,854,775,804 is divisible by
four but gross 11,529,215,046,068,469,755 exceeds the bound. Two individually exact
net lines of 4,000,000,000,000,000,000 each have individually in-range gross
5,000,000,000,000,000,000 but aggregate gross 10,000,000,000,000,000,000 overflows.
No float conversion, saturation or wrapped values can authorize posting. JSON
large integers must be checked losslessly (e.g. arbitrary-precision integers), not
silently rounded through JavaScript `Number`.

## Proposed public-boundary oracle scenarios (not execution results)

The cash manifest contains literal amount examples, stage journals and named
scenario variants. They are proposed inputs/observations for later regressions,
not a declaration that they ran or that the parent has frozen all typed policy.

1. **Positive stages and continuation:** issue Jan 15 has no journal; Jan 20 actual
   full receipt has the three literal lines above. A second eligible cash invoice
   has its own numbered identity and zero issue journal. A distinct actual bank
   receipt with a separately retained external PDF content hash gets its own receipt
   stage/proof/date/voucher. A genuine second statement entry in an already-used PDF
   remains outside this first slice. Proven continuation is not rejected as legacy.
2. **Required origin facts/history:** independently omit or mark unknown new
   business, zero opening, no unpaid items, no prior activity or outside-workspace
   review. Explicit nonzero/prior facts stop. Preexisting voucher/issued legacy
   invoice stops even with zero balances/outstanding; legacy invoice-date cash
   posting cannot be repaired with a checkbox.
3. **Applicability:** independently omit cash eligibility/turnover, effective VAT
   registration, registered bokslutsmetoden, consistent cash treatment, annual
   registration/base or no-EU facts. Exercise both independent limits, including
   equality, exceedance and unknown; never infer one limit from the other.
4. **Dates/calendar:** 2025/2027, cross-year completion or receipt, completion after
   issue, receipt before issue, advances/prior payments, scheduled not received,
   missing/invalid actual date, unsupported fiscal year/calendar/EU circumstances
   and wrong filing year stop. Short first years starting in2026 and ending
   December31 are included after explicit review; extended years and other ends
   stop. Keep calendar metadata distinct from approval.
5. **Actual bank facts:** partial 1,249,999, over 1,250,001, zero, negative or
   missing/unknown actual öre must stop. Require actual receipt, date, SEK currency
   confirmation and stable bank reference; invoice total alone cannot fill them.
6. **Source verification/identity:** missing/tampered/other-workspace bytes, MIME
   metadata only, generated invoice PDF, unknown/foreign currency, ambiguous
   statement entry or absent stable bank reference stops. Separate real entries are not legally the same receipt, but this first
   application case still blocks a second consumption of the same PDF hash even
   with a distinct typed bank identity.
7. **Duplicates/replay:** one actual receipt identity cannot fund another invoice
   or be posted again with a new replay key, renamed file or PDF wrapper. Exact
   durable replay returns the same result with **zero additional** vouchers/lines
   or consumption. Changed invoice/date/amount/currency/source-entry/facts cannot
   receive a cached success. Missing/tampered evidence must not be represented as
   freshly verified proof because a cache exists. Historical read-only recovery
   remains distinct from authorizing any new mutation; exact matching completed recovery remains ahead of current
   rule/profile/clock/evidence guards and cannot create new accounting. A changed
   request must fail. The immutable proof states integrity at posting time; a
   historical result is not a fresh claim that currently missing bytes verified.
8. **Atomicity/staleness/locks:** changed invoice snapshot, lines, profiles, rules,
   digest or origin invalidates reviewed proof. Concurrent duplicate attempts must
   not produce two recognitions. Check locks using **actual receipt date**.
   Evidence or write failure must not leave a partial voucher, consumed source or
   completed replay. Keep retained/staged source evidence available on a stop.
9. **No bypass expansion:** identity-only issuance must prevent ordinary method
   changes even before a first voucher. CSV posting stays blocked, while imports,
   staging and inspection remain available. Credits, partials, overpayments,
   refunds, legacy and unsupported raw invoice-date paths stay blocked. Cash
   VAT/year-end approvals stay blocked even when this invoice is fully paid.
10. **Unpaid closing:** a supported issue need not have a future guaranteed payment.
    If no supported full same-year receipt arrives, retain its unpaid status and
    evidence. Never synthesize zero unpaid items, a receipt, closing completion or
    return approval. The sources require unpaid closing treatment; this slice
    explicitly does not implement or validate it.

Every blocked new transition has no new recognition journals, source consumption
or successful transition replay. An issue rejected before transition consumes no
new invoice identity. A receipt rejected after a valid issue preserves that issued
identity, source evidence and historical inspection; it does not erase the invoice.
No restriction here licenses rewriting published/historical invoices.

## Swedish civil date and future-event boundary

Cash issue sending date and actual receipt date cannot be in the future relative
to Swedish civil date. Production obtains its own UTC clock; no renderer-supplied
clock can grant eligibility. With the existing Chrono library, compare that instant
against the start of the candidate Swedish civil day **in 2026**. This avoids
inventing future-year timezone support. The private date validator can accept an
explicit clock for deterministic unit checks.
Historical completed replay and read-only proof inspection do not become new
recognition merely because today's clock or evidence availability changed.

Three official texts were freshly retrieved on 2026-11-04 for the finite-year
clock data, separately from inherited rate/deadline evidence:

- SFS 1979:988 states Swedish normal time is UTC **plus one hour**:
  <https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/forordning-1979988-om-svensk-normaltid_sfs-1979-988/>.
- SFS 2001:127, 1–2 §§ adds one hour during summer time, switching at 02:00
  normal time in March and 03:00 summer time in October:
  <https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/forordning-2001127-om-sommartid_sfs-2001-127/>.
- Commission schedule **2021/C 149/01**, Official Journal C149/1, 2021-04-27,
  gives **2026-03-29 and 2026-10-25 at 01:00 UTC**:
  <https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX%3A52021XC0427%2801%29>.

Their explicit 2026 instants and offsets are evidence data, not a hand-written
last-Sunday/DST algorithm or a claim about other years. Chrono-TZ 0.10.4 was
researched but rejected to preserve the no-new-dependencies constraint. All eight
clock examples are literal independently derived winter/summer midnight and
spring/autumn transition expectations, not outputs of the future implementation.
Future processing years remain possible by comparing the actual UTC clock to the
start instant of the **2026 event day**; no later-year offset table is required.
Other event years remain outside this cash case.

## Invoice-v2 compatibility lineage and shared rule-version constraint

The published invoice-v1 fixture must remain byte-for-byte unchanged:

- Validation ID: `invoice-recognition-se-service25-2026-exact-v1`.
- Rule version: `rv-2026-domestic-service25-exact-v1`.
- SHA-256: `cb00f554e7ee3697b8461309e7184fd88b3a0aa70300f3f27ead9f6ca98b8594`.

The new invoice-v2 manifest copies v1 scope, sources, arithmetic, recognition and
exclusions unchanged. Its **new validation ID and shared rule version** plus
explicit `derivedFrom` ID/path/digest are compatibility lineage only, not reused
v1 identity and **not newly validated invoice scope**. The original dossier is
not amended. The cash manifest is separately prepared, not a relabeling of the
invoice's 1510 expectations or its sending-date declaration as cash permission.

Migration22 and the versioned registry preserve one active rule version per year:
new invoice issuance uses the shared v2 while cash has its own method/case identity.
Historical v1 rules, manifests, native records, PDFs, fingerprints and readers remain
usable without backfill, reinterpretation or digest replacement. A manifest digest
or an active row alone is not the eligibility gate. The source package itself
activates neither case; implementation and verification are separate work.

## Mechanical checks of this evidence package

On 2026-11-04, a read-only Python check parsed both new manifests losslessly and
recomputed the cash fixture's **12 amount cases** using arbitrary-precision
integers and rational fractions. It checked exact per-line VAT, fractional cases,
line/gross/aggregate overflow, the zero issue journal, the literal three-line
receipt balance and no 1510, the completion/issue/receipt ordering and receipt
date, separate limit equality/exceedance/unknown outcomes in **5 eligibility
cases**, the 2027 filing year/Wednesday deadline, and unique IDs in the original **25
proposed scenarios / 7 source references** before the user's one-PDF decision. It verified the frozen case/shared
rule IDs and compared **every original invoice-v1 field except the two successor
IDs** to invoice-v2 for exact value equality. The original v1 SHA-256 remained
`cb00f554e7ee3697b8461309e7184fd88b3a0aa70300f3f27ead9f6ca98b8594`.

During the initial evidence-only preparation, `npm run test:fixtures` passed **19 tests**. That existing suite validates the
separate golden-scenario fixture directory; it does **not** execute these new
accounting-validation scenarios or establish cash support. No production or test
source was edited in that initial worker's preparation, and it did not run the
full application suite. Those mechanical checks were not the independent review.
The subsequent parent-owned verification is recorded below.

After the user's one-receipt-per-PDF decision, the parent added its explicit
reservation scenario, eight Swedish-civil-date examples, seventeen future-day
examples and nine first-year examples. The user also explicitly included
shortened first years ending December2026. The published invoice-v1 bytes remain
unchanged.

The added `cash_validation_evidence.rs` and `cash_clock_evidence.rs` contain
**9 passing fixture-consistency tests**, not execution of the 26 proposed runtime
scenarios. They check arithmetic/lineage, both limits, the 9 first-year, 8 clock and
17 future-day examples, the literal journals/calendar and PDF reservation inventory.
`PATH="$HOME/.cargo/bin:$PATH" npm run ci:public` passed: **276 Rust / 158 frontend
tests**, golden/M8 checks, production build and unchanged generated bindings.
No cash production behavior, dependency, migration or active rule changed.

## Implementation checks and reproduced review defects

The initial integrated public CI run passed315 Rust/192 frontend tests, production
build and bindings. That green run did **not** clear security or integration review.
Independent backend review reproduced two blocking defects: in-process lopdf could
loop on a tiny cyclic `/Prev` PDF or expand compressed objects before post-parse
limits, while holding the writer; valid staged ordinary-expense matches were also
incorrectly rejected as cash continuation. Neither issue was dismissed because
existing tests passed.

The user chose a separately packaged, killable PDF helper rather than narrower PDF
acceptance. Parser work now precedes the accounting writer; the writer rechecks the
same retained source/hash and every financial predicate. The fixed helper receives
private descriptor input, has no GUI/DB/network role, and emits a bounded hash-bound
reply. Actual limits are documented once in `security-privacy`. Requested-Rust-heap
and relative-VM limits are **not a universal RSS or stack-memory ceiling**. Setup
failure or unsupported OS denies new recognition, never an unbounded fallback.
Native debug/release resource probes cover ordinary/compressed, cyclic and large
expansion cases; packaged helper discovery/runtime and signing are separate gates.

Public staged-expense regressions first failed after a real imported outflow and
expense posting. The correction accepts only properly scoped/matched expense
voucher, staging and original job provenance with compatible balanced lines, not
an expense label alone. First-origin empty-history requirements are unchanged.
Additional corruption and same-date/amount forgery cases retain fail-closed checks.

The retained backend RED transcripts in `fixtures/historical-regressions/` record
successful compilation followed by behavioral failures for cash issue, full
receipt, source/method/real-first-year boundaries, closing readiness, staged
expense continuation and PDF isolation. Some transcripts honestly include earlier
setup/toolchain corrections before the qualifying run. Native-v1 migration tests
were first run after migration22 existed, so no migration RED was fabricated.
Initial new-frontend helper/component tests failed import collection, **not** a
qualifying behavioral RED; not all frontend development is claimed behavior-first.
Later legacy-routing/localization regressions failed behaviorally before correction.
Modal layout tests now use real CSS, not the initially empty default test CSS
import; their computed-style checks do not replace native geometry verification.

Unit/component checks cannot prove real bank facts. The execution PDFs are visibly
synthetic and include no bank-authentication or named-bank-format coverage. The
original native invoice-v1 archive is historical engineering evidence only. Its
raw f9b6933… context/fingerprint, rows, PDF bytes and completed recovery are checked
through real schema21→22 migration, cold reopen and encrypted backup/restore.

The later isolated-helper implementation passed the complete public gate, including
26 helper-build tests,192 frontend tests and all Rust suites with isolated security
probes. Native arm64 app/DMG construction and execution of the helper inside the
actual build bundle also passed. The normal compressed fixture succeeded; cyclic
and expansion fixtures terminated without a valid reply. Artifact hashes and exact
qualifications are retained in [the November6 verification record](../verification/cash-method-2026-11-06.md).
Installed GUI cash UAT, final step review and broader platform/signing checks remain
separate, unfinished gates.

Formal step review also found an unstaged-expense continuation gap (`rc_c993d9f2`).
The public regression committed a prior-year expense without a bank match, then
observed incorrect success in preflight, issuance and receipt. One common expense
predicate now checks all vouchers against origin dates and original scoped posting
provenance. Valid staged and unstaged controls remain supported. The correction
passes focused regressions and the full public gate; fresh step review is required.
Earlier bundle hashes do not identify this later correction. Installed GUI cash
UAT therefore requires a rebuilt artifact.

## Independent review and remaining obligations

Two read-only AI reviewers approved the concept/source package without material
findings. Review `01a1a7b8-4881-7355-a36b-d74c75a35ba6` covered the initial amended
one-PDF boundary. Review `01a1b035-f0ac-7355-a36b-d757127bfe09` independently checked
the final short-year and finite-clock additions, authoritative passages and all
literal examples. Both distinguish source applicability from execution/activation;
neither is professional or tax-authority attestation. No professional review is
mandatory or falsely claimed.

Implementation review `01a1b076-3fd2-7355-a36b-d7893aab80cc` requested changes for
the reproduced parser and staged-expense defects above. Focused security follow-up
`01a1b0a8-ae64-7355-a36b-d7a5efd77752` approved their corrections. UI review found
unbounded long dialogs and contradictory supported-cash status wording; follow-up
`01a1b07b-a1ea-7355-a36b-d78bcc878d29` approved the explicit large-layout and CSV-only
message fixes with targeted component/computed-style checks. These reviews do not
substitute for final step review or actual packaged cash UAT.

- Final implementation/security review, integrated verification and actual packaged
  cash UAT remain pending. Source review and passing regressions alone do not
  complete those gates; the shared cash approval/legacy guards remain closed.
- The provided local text is not an original-PDF integrity attestation; the rate remains carried prior evidence, while the cash deadline page,
  first-year guidance and bounded relevant SFL sections were freshly retrieved.
  The historical v1 deadline retrieval-reset record is not rewritten. No inaccessible rounding policy
  is invented; fractional-öre cases remain outside coverage.
- No real/sanitized bank PDF and owner review was supplied for execution. The
  manifest's bank reference/locator is conspicuously **synthetic**, not evidence
  that any bank format, bank authentication, OCR or actual receipt was tested.
- Parent owns strict typed-policy freeze, active spec/formal coverage review,
  source registry integration, historical v1 readers, digest binding and
  implementation. Proposed negative scenarios need failing public-boundary tests,
  followed by atomicity/persistence/replay/lock and relevant native verification.
- Closing, opening-position entry/remediation, cash approvals and complete
  invoice/PDF/books/return legal correctness remain unvalidated. Excluding unpaid
  closing is a fail-closed boundary, not evidence that the legal obligation is zero.
