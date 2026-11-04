import { useId } from "react"
import evidence from "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
import { t, tVars, type Locale, type MessageKey } from "../i18n"
import type { InvoiceEligibilityFacts, InvoiceIssuePreflight } from "../lib/bindings"

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
    {assessment.validationId === evidence.validationId ? <section aria-labelledby={sourcesId}>
      <h5 id={sourcesId}>{t(locale, "eligibility.sources")}</h5>
      <ul>{evidence.sourceReferences.map((source) => <li key={source.id}>
        <a href={source.url} target="_blank" rel="noopener noreferrer">{source.pinpoint}</a>
      </li>)}</ul>
    </section> : null}
  </section>
}
