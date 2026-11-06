import { useEffect, useRef, useState } from "react"
import { ActionReviewDialog } from "./ActionReviewDialog"
import { CashEvidence, CashFactsSummary } from "./CashEvidence"
import { CashFactChoice } from "./CashFactChoice"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import { appErrorMessage, cashBusinessOriginGet, invoiceCashIssue, invoiceCashIssuePreflight } from "../lib/commands"
import type { CashBusinessOrigin, CashIssuePreflight, InvoiceSummary } from "../lib/bindings"
import { cashIssueQuestions, cashOriginQuestions, emptyCashIssueFacts, emptyCashOriginFacts } from "../lib/cashPresentation"
import { formatSekMinor } from "../lib/money"

export function CashIssueDialog({ invoice, idempotencyKey, onCancel, onIssued, onBusyChange }: {
  invoice: InvoiceSummary; idempotencyKey: string; onCancel: () => void; onIssued: (invoice: InvoiceSummary) => void; onBusyChange: (busy: boolean) => void
}) {
  const { locale } = useLocale()
  const [facts, setFacts] = useState(emptyCashIssueFacts)
  const [originReview, setOriginReview] = useState(emptyCashOriginFacts)
  const [origin, setOrigin] = useState<CashBusinessOrigin | null>(null)
  const [originLoaded, setOriginLoaded] = useState(false)
  const [issueDate, setIssueDate] = useState("")
  const [review, setReview] = useState<CashIssuePreflight | null>(null)
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  const alive = useRef(true)
  const pending = useRef(false)
  useEffect(() => {
    alive.current = true
    cashBusinessOriginGet().then(result => { if (alive.current) { setOrigin(result); setOriginLoaded(true) } }).catch(error => { if (alive.current) setError(appErrorMessage(error, t(locale, "cash.originFailed"))) })
    return () => { alive.current = false }
  }, [])
  const actualOrigin = origin?.facts ?? originReview
  const shortYear = actualOrigin.businessStartedOn !== null && actualOrigin.businessStartedOn !== "2026-01-01"
  async function confirm() {
    if (pending.current || !originLoaded) return
    if (!review && (!issueDate || !facts.serviceCompletedOn || cashIssueQuestions.some(({ field }) => (field !== "shortYearTurnoverAdjustmentReviewed" || shortYear) && facts[field] === null) || (!origin && Object.values(originReview).some(value => value === null || value === "")))) {
      setError(t(locale, "cash.incomplete")); return
    }
    pending.current = true; setBusy(true); onBusyChange(true); setError("")
    const input = { invoiceId: invoice.id, issueDate, facts, originReview: origin ? null : originReview }
    try {
      if (review) {
        const result = await invoiceCashIssue({ ...input, idempotencyKey, reviewFingerprint: review.assessment.reviewFingerprint })
        if (alive.current) onIssued(result)
      } else {
        const result = await invoiceCashIssuePreflight(input)
        if (!alive.current) return
        if (result.invoiceId !== invoice.id || result.issueDate !== issueDate || result.reviewedInvoice.invoiceId !== invoice.id || result.assessment.accountingDate !== null || !result.assessment.reviewFingerprint) throw new Error(t(locale, "cash.blocked"))
        setReview(result)
      }
    } catch (error) {
      if (alive.current) { setReview(null); setError(`${t(locale, "cash.fresh")} ${appErrorMessage(error, t(locale, "cash.blocked"))}`) }
    } finally { pending.current = false; if (alive.current) { setBusy(false); onBusyChange(false) } }
  }
  return <ActionReviewDialog open layout="large" title={t(locale, "cash.issueTitle")} summary={review ? `${review.reviewedInvoice.counterpartyName} · ${formatSekMinor(review.reviewedInvoice.totalIncVatMinor)}` : t(locale, "cash.scope")} consequences={[t(locale, "cash.issueEffect")]} correction={t(locale, "cash.limits")} confirmLabel={t(locale, review ? "cash.confirmIssue" : "cash.reviewIssue")} cancelLabel={t(locale, "eligibility.keep")} busy={busy} onConfirm={() => void confirm()} onCancel={() => { if (!pending.current) onCancel() }}>
    {review ? <>
      <section><h4>{t(locale, "eligibility.invoiceSnapshot")}</h4><p>{`${t(locale, "cash.invoiceDueDate")}: ${review.reviewedInvoice.dueDate ?? t(locale, "cash.unknown")}`}</p><ul>{review.reviewedInvoice.lines.map(line => <li key={line.id}>{line.description} · {line.quantity} × {formatSekMinor(line.unitPriceMinor)} · {t(locale, "cash.net")} {formatSekMinor(line.lineExVatMinor)} · {t(locale, "cash.vat")} {formatSekMinor(line.lineVatMinor)} · {line.accountNumber}</li>)}</ul></section>
      <CashFactsSummary title={t(locale, "cash.reviewedIssueFacts")} facts={facts} />
      <CashFactsSummary title={t(locale, "cash.reviewedOrigin")} facts={actualOrigin} />
      <p>{tVars(locale, "cash.taxYearBase", { current: formatSekMinor(review.currentTaxableBaseMinor), projected: formatSekMinor(review.projectedTaxableBaseMinor) })}</p>
      <CashEvidence assessment={review.assessment} />
      <button type="button" disabled={busy} onClick={() => setReview(null)}>{t(locale, "eligibility.edit")}</button>
    </> : <fieldset disabled={busy || !originLoaded}>
      <label>{t(locale, "cash.issueDate")}<input type="date" value={issueDate} onChange={e => setIssueDate(e.target.value)} /></label>
      <label>{t(locale, "cash.serviceCompletedOn")}<input type="date" value={facts.serviceCompletedOn ?? ""} onChange={e => setFacts({ ...facts, serviceCompletedOn: e.target.value || null })} /></label>
      {origin ? <p>{t(locale, "cash.originVerified")}: {origin.facts.businessStartedOn ?? t(locale, "cash.unknown")} — {origin.facts.firstFinancialYearEndsOn ?? t(locale, "cash.unknown")} ({origin.id})</p> : <section>
        <p>{t(locale, "cash.originNote")}</p>
        <label>{t(locale, "cash.businessStartedOn")}<input type="date" value={originReview.businessStartedOn ?? ""} onChange={e => setOriginReview({ ...originReview, businessStartedOn: e.target.value || null })} /></label>
        <label>{t(locale, "cash.firstFinancialYearEndsOn")}<input type="date" value={originReview.firstFinancialYearEndsOn ?? ""} onChange={e => setOriginReview({ ...originReview, firstFinancialYearEndsOn: e.target.value || null })} /></label>
        {cashOriginQuestions.map(q => <CashFactChoice key={q.field} {...q} value={originReview[q.field]} onChange={value => setOriginReview({ ...originReview, [q.field]: value })} />)}
      </section>}
      {cashIssueQuestions.filter(q => q.field !== "shortYearTurnoverAdjustmentReviewed" || shortYear).map(q => <CashFactChoice key={q.field} {...q} value={facts[q.field]} onChange={value => setFacts({ ...facts, [q.field]: value })} />)}
      <p>{t(locale, "cash.shortYearHelp")}</p>
    </fieldset>}
    {error ? <p role="alert">{error}</p> : null}
  </ActionReviewDialog>
}
