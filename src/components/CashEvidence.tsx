import { useEffect, useRef, useState } from "react"
import type { CashIssueAssessment, CashReceiptAssessment } from "../lib/bindings"
import { appErrorMessage, invoiceValidationSourceOpen } from "../lib/commands"
import { cashFactLabels, cashFactText } from "../lib/cashPresentation"
import { validationSources } from "../lib/validationRegistry"
import { formatSekMinor } from "../lib/money"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"

const technicalKeys = new Set(["id", "firstInvoiceId", "validationId", "evidenceDigest", "reviewFingerprint", "ruleVersionId", "originId", "coverageSummary", "documentId", "documentSha256", "deadlineRegime"])

function FactRows({ facts }: { facts: object }) {
  const { locale } = useLocale()
  const entries = Object.entries(facts)
  const technical = entries.filter(([key]) => technicalKeys.has(key) || !cashFactLabels[key])
  function row([key, value]: [string, unknown]) {
    return <div key={key}><dt>{cashFactLabels[key] ? t(locale, cashFactLabels[key]) : key}</dt><dd>{
      value !== null && typeof value === "object" ? <FactRows facts={value} /> :
        key === "accountingDate" && value === null ? t(locale, "cash.notRecognized") :
          key === "actualReceivedAmountMinor" && typeof value === "number" ? formatSekMinor(value) : cashFactText(value, locale)
    }</dd></div>
  }
  return <><dl>{entries.filter(([key]) => !technicalKeys.has(key) && cashFactLabels[key]).map(row)}</dl>
    {technical.length ? <details><summary>{t(locale, "cash.technicalDetails")}</summary><dl>{technical.map(row)}</dl></details> : null}</>
}

export function CashFactsSummary({ title, facts }: { title: string; facts: object }) {
  return <section><h4>{title}</h4><FactRows facts={facts} /></section>
}
export function CashEvidence({ assessment }: { assessment: CashIssueAssessment | CashReceiptAssessment }) {
  const { locale } = useLocale()
  const alive = useRef(true)
  const pending = useRef(false)
  const [error, setError] = useState("")
  useEffect(() => { alive.current = true; return () => { alive.current = false } }, [])
  async function open(sourceId: string) {
    if (pending.current) return
    pending.current = true
    try {
      await invoiceValidationSourceOpen({ validationId: assessment.validationId, sourceId })
    } catch (error) {
      if (alive.current) setError(appErrorMessage(error, t(locale, "eligibility.sourceOpenFailed")))
    } finally { pending.current = false }
  }
  return <section>
    {assessment.accountingDate === null ? <p>{t(locale, "cash.issueEffect")}</p> : <p>{tVars(locale, "cash.receiptAccountingDate", { date: assessment.accountingDate })}</p>}
    <CashFactsSummary title={t(locale, "cash.immutableAssessment")} facts={assessment} />
    <h4>{t(locale, "eligibility.sources")}</h4>
    <ul>{validationSources(assessment.validationId).map(source => <li key={source.id}><a href={source.url} onClick={event => { event.preventDefault(); void open(source.id) }}>{source.label}</a></li>)}</ul>
    {error ? <p role="alert">{error}</p> : null}
  </section>
}
