import { useEffect, useId, useRef, useState } from "react"
import { validationSources } from "../lib/validationRegistry"
import { t, tVars, type Locale, type MessageKey } from "../i18n"
import type { InvoiceEligibilityFacts, InvoiceIssuePreflight } from "../lib/bindings"
import { appErrorMessage, invoiceValidationSourceOpen } from "../lib/commands"

export const eligibilityQuestions: ReadonlyArray<{
  field: Exclude<keyof InvoiceEligibilityFacts, "serviceCompletedOn">
  label: MessageKey
  help: MessageKey
}> = [
  { field: "ordinaryDomesticService", label: "eligibility.domestic", help: "eligibility.domesticHelp" },
  { field: "priorPaymentReceived", label: "eligibility.prior", help: "eligibility.priorHelp" },
  { field: "noEuTradeInYear", label: "eligibility.noEu", help: "eligibility.noEuHelp" },
  { field: "annualVatRegistrationConfirmed", label: "eligibility.annual", help: "eligibility.annualHelp" },
  { field: "expectedTaxableBaseWithinLimit", label: "eligibility.base", help: "eligibility.baseHelp" },
  { field: "ordinaryInvoiceTimingConfirmed", label: "eligibility.timing", help: "eligibility.timingHelp" },
]

export function InvoiceEligibilityFactsView({ facts, locale }: {
  facts: InvoiceEligibilityFacts
  locale: Locale
}) {
  return <section>
    <h4>{t(locale, "eligibility.factsTitle")}</h4>
    <dl>
      <dt>{t(locale, "eligibility.completed")}</dt><dd>{facts.serviceCompletedOn ?? t(locale, "eligibility.unknown")}</dd>
      {eligibilityQuestions.map(({ field, label }) => <div key={field}>
        <dt>{t(locale, label)}</dt>
        <dd>{t(locale, facts[field] === null ? "eligibility.unknown" : facts[field] ? "eligibility.yes" : "eligibility.no")}</dd>
      </div>)}
    </dl>
  </section>
}

export function InvoiceEligibilityEvidence({ assessment, locale }: {
  assessment: NonNullable<InvoiceIssuePreflight["eligibility"]>
  locale: Locale
}) {
  const sourcesId = useId()
  const pending = useRef(false)
  const alive = useRef(true)
  const [opening, setOpening] = useState(false)
  const [sourceError, setSourceError] = useState("")
  useEffect(() => {
    alive.current = true
    return () => { alive.current = false }
  }, [])

  async function openSource(sourceId: string) {
    if (pending.current) return
    pending.current = true
    setOpening(true)
    setSourceError("")
    try {
      await invoiceValidationSourceOpen({ validationId: assessment.validationId, sourceId })
    } catch (error) {
      if (alive.current) setSourceError(`${t(locale, "eligibility.sourceOpenFailed")} ${appErrorMessage(error, "")}`)
    } finally {
      pending.current = false
      if (alive.current) setOpening(false)
    }
  }

  return <section className="eligibility-evidence">
    <h4>{t(locale, "eligibility.evidenceTitle")}</h4>
    <p>{tVars(locale, "eligibility.assessment", {
      year: assessment.accountingYear, date: assessment.accountingDate, completed: assessment.completedOn,
    })}</p>
    <p>{tVars(locale, "eligibility.deadline", {
      period: assessment.periodKey, regime: assessment.deadlineRegime, due: assessment.dueOn,
    })}</p>
    <p>{assessment.coverageSummary}</p>
    <p>{tVars(locale, "eligibility.validation", { id: assessment.validationId })}</p>
    <p>{tVars(locale, "eligibility.digest", { digest: assessment.evidenceDigest })}</p>
    <p>{tVars(locale, "eligibility.rule", { id: assessment.ruleVersionId })}</p>
    <p>{tVars(locale, "eligibility.fingerprint", { fingerprint: assessment.reviewFingerprint })}</p>
    {validationSources(assessment.validationId).length > 0 ? <section aria-labelledby={sourcesId}>
      <h5 id={sourcesId}>{t(locale, "eligibility.sources")}</h5>
      <ul>{validationSources(assessment.validationId).map((source) => <li key={source.id}>
        <a href={source.url} aria-disabled={opening} onClick={(event) => {
          event.preventDefault()
          void openSource(source.id)
        }}>{source.label}</a>
      </li>)}</ul>
    </section> : null}
    {sourceError ? <p role="alert">{sourceError}</p> : null}
  </section>
}
