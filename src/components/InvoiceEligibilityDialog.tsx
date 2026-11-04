import { useEffect, useId, useRef, useState } from "react"
import { ActionReviewDialog } from "./ActionReviewDialog"
import { eligibilityQuestions, InvoiceEligibilityEvidence, InvoiceEligibilityFactsView } from "./InvoiceEligibilityEvidence"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import { appErrorMessage, invoiceIssue, invoiceIssuePreflight, type InvoiceSummary } from "../lib/commands"
import type { InvoiceEligibilityFacts, InvoiceIssuePreflight } from "../lib/bindings"
import { localTodayIsoDate } from "../lib/invoiceStatus"
import { formatSekMinor } from "../lib/money"

const unansweredFacts: InvoiceEligibilityFacts = {
  ordinaryDomesticService: null,
  serviceCompletedOn: null,
  priorPaymentReceived: null,
  noEuTradeInYear: null,
  annualVatRegistrationConfirmed: null,
  expectedTaxableBaseWithinLimit: null,
  ordinaryInvoiceTimingConfirmed: null,
}

/** Mounted only for one draft/workspace. Preflight is guidance, never posting authority. */
export function InvoiceEligibilityDialog({ invoice, idempotencyKey, onCancel, onIssued, onBusyChange }: {
  invoice: InvoiceSummary
  idempotencyKey: string
  onCancel: () => void
  onIssued: (invoice: InvoiceSummary) => void
  onBusyChange: (busy: boolean) => void
}) {
  const { locale } = useLocale()
  const groupId = useId()
  const [facts, setFacts] = useState<InvoiceEligibilityFacts>(unansweredFacts)
  const [choices, setChoices] = useState<Partial<Record<keyof InvoiceEligibilityFacts, string>>>({})
  const [issueDate, setIssueDate] = useState(localTodayIsoDate)
  const [preflight, setPreflight] = useState<InvoiceIssuePreflight | null>(null)
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  const alive = useRef(true)
  const pending = useRef(false)
  useEffect(() => {
    alive.current = true
    return () => { alive.current = false }
  }, [])

  function updateFacts(next: InvoiceEligibilityFacts) {
    setFacts(next)
    setPreflight(null)
    setError("")
  }

  async function check() {
    if (pending.current) return
    const completedOn = facts.serviceCompletedOn
    if (!issueDate || !completedOn || Object.values(facts).some((value) => value === null || value === "")) {
      setError(t(locale, "eligibility.incomplete"))
      return
    }
    if (facts.ordinaryDomesticService !== true || facts.priorPaymentReceived !== false ||
      facts.noEuTradeInYear !== true || facts.annualVatRegistrationConfirmed !== true ||
      facts.expectedTaxableBaseWithinLimit !== true || facts.ordinaryInvoiceTimingConfirmed !== true ||
      !/^2026-\d{2}-\d{2}$/.test(issueDate) || !/^2026-\d{2}-\d{2}$/.test(completedOn) ||
      completedOn > issueDate) {
      const conflicts = eligibilityQuestions
        .filter(({ field }) => facts[field] !== (field !== "priorPaymentReceived"))
        .map(({ field, label }) => `${t(locale, label)} ${t(locale, facts[field] ? "eligibility.yes" : "eligibility.no")}`)
      setError([t(locale, "eligibility.outside"), ...conflicts].join(" · "))
      return
    }
    pending.current = true
    setBusy(true)
    onBusyChange(true)
    setError("")
    try {
      const result = await invoiceIssuePreflight({ invoiceId: invoice.id, issueDate, eligibilityFacts: facts })
      if (!alive.current) return
      if (result.requiresVatTreatmentReview) {
        // Preserve the existing threshold review, but it cannot submit issuance.
        setPreflight(result)
      } else if (!result.eligibility?.reviewFingerprint || result.invoiceId !== invoice.id || result.issueDate !== issueDate) {
        setError(t(locale, "eligibility.missingAssessment"))
      } else {
        setPreflight(result)
      }
    } catch (error) {
      if (alive.current) setError(`${t(locale, "eligibility.blocked")} ${appErrorMessage(error, "")}`)
    } finally {
      pending.current = false
      if (alive.current) { setBusy(false); onBusyChange(false) }
    }
  }

  async function issue() {
    if (pending.current || !preflight?.eligibility || preflight.requiresVatTreatmentReview) return
    pending.current = true
    setBusy(true)
    onBusyChange(true)
    try {
      const issued = await invoiceIssue({
        invoiceId: invoice.id, idempotencyKey, issueDate,
        eligibilityFacts: facts, reviewFingerprint: preflight.eligibility.reviewFingerprint,
      })
      if (alive.current) onIssued(issued)
    } catch (error) {
      if (alive.current) {
        setPreflight(null)
        setError(`${t(locale, "eligibility.freshReview")} ${appErrorMessage(error, "")}`)
      }
    } finally {
      pending.current = false
      if (alive.current) { setBusy(false); onBusyChange(false) }
    }
  }

  const blockedThreshold = preflight?.requiresVatTreatmentReview
  const summary = tVars(locale, "actionReview.issue.summary", {
    customer: invoice.counterpartyName, total: formatSekMinor(invoice.totalIncVatMinor),
    vat: formatSekMinor(invoice.totalVatMinor),
    rate: invoice.totalExVatMinor > 0 ? Math.round(invoice.totalVatMinor / invoice.totalExVatMinor * 100) : 0,
  })

  return <ActionReviewDialog
    open
    title={t(locale, preflight ? "actionReview.issue.title" : "eligibility.title")}
    summary={summary}
    consequences={preflight ? blockedThreshold ? [
      t(locale, "invoices.thresholdReviewAction"),
      tVars(locale, "invoices.thresholdProjectedTurnover", {
        projected: formatSekMinor(preflight.projectedTurnoverMinor), threshold: formatSekMinor(preflight.thresholdMinor ?? 0),
      }),
      tVars(locale, "invoices.thresholdRule", { year: preflight.taxYear, sourceUrl: preflight.sourceUrl ?? "" }),
    ] : [tVars(locale, "eligibility.send", { date: issueDate })] : []}
    correction={blockedThreshold ? t(locale, "invoices.thresholdReviewAction") : `${preflight ? `${t(locale, "actionReview.issue.correction")} ` : ""}${t(locale, "eligibility.limit")}`}
    confirmLabel={t(locale, preflight ? blockedThreshold ? "invoices.closeReview" : "actionReview.issue.confirm" : "eligibility.check")}
    cancelLabel={t(locale, "eligibility.keep")}
    busy={busy}
    onConfirm={() => { if (blockedThreshold) onCancel(); else if (preflight) void issue(); else void check() }}
    onCancel={() => { if (!pending.current) onCancel() }}
  >
    <p className="invoice-eligibility">{t(locale, "eligibility.scope")}</p>
    {preflight ? <>
      <InvoiceEligibilityFactsView facts={facts} locale={locale} />
      {preflight.eligibility ? <InvoiceEligibilityEvidence assessment={preflight.eligibility} locale={locale} /> : null}
      <button type="button" className="secondary" disabled={busy} onClick={() => setPreflight(null)}>{t(locale, "eligibility.edit")}</button>
    </> : <>
      <p>{t(locale, "eligibility.intro")}</p>
      <label>{t(locale, "eligibility.issueDate")}
        <input type="date" value={issueDate} disabled={busy} onChange={(event) => {
          setIssueDate(event.target.value); setPreflight(null); setError("")
        }} />
      </label>
      <label>{t(locale, "eligibility.completed")}
        <input type="date" value={facts.serviceCompletedOn ?? ""} disabled={busy}
          onChange={(event) => updateFacts({ ...facts, serviceCompletedOn: event.target.value || null })} />
      </label>
      {eligibilityQuestions.map(({ field, label, help }) => <fieldset key={field} disabled={busy}>
        <legend>{t(locale, label)}</legend>
        <p className="muted">{t(locale, help)}</p>
        <div className="eligibility-choices">
          {(["yes", "no", "unknown"] as const).map((choice) => <label key={choice}>
            <input type="radio" name={`${groupId}-${field}`} value={choice} checked={choices[field] === choice}
              onChange={() => {
                setChoices((current) => ({ ...current, [field]: choice }))
                updateFacts({ ...facts, [field]: choice === "unknown" ? null : choice === "yes" })
              }} />
            {t(locale, `eligibility.${choice}`)}
          </label>)}
        </div>
      </fieldset>)}
    </>}
    {error ? <p role="alert">{error}</p> : null}
  </ActionReviewDialog>
}
