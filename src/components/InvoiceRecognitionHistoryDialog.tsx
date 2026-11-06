import { useEffect, useState } from "react"
import { ActionReviewDialog } from "./ActionReviewDialog"
import { InvoiceEligibilityEvidence, InvoiceEligibilityFactsView } from "./InvoiceEligibilityEvidence"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import { CashEvidence, CashFactsSummary } from "./CashEvidence"
import { classifyIssuedTreatment } from "../lib/issuedTreatment"
import type { CashInvoiceHistory } from "../lib/bindings"
import { appErrorMessage, invoiceCashHistoryGet, invoiceIssueValidationGet } from "../lib/commands"

type ValidationRecord = Awaited<ReturnType<typeof invoiceIssueValidationGet>>

export function InvoiceRecognitionHistoryDialog({ invoiceId, onClose }: {
  invoiceId: string
  onClose: () => void
}) {
  const { locale } = useLocale()
  const [record, setRecord] = useState<ValidationRecord>(null)
  const [cash, setCash] = useState<CashInvoiceHistory | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState("")
  useEffect(() => {
    let current = true
    setLoading(true)
    setError("")
    setRecord(null)
    setCash(null)
    invoiceCashHistoryGet({ invoiceId }).then(async (cashResult) => {
      if (!current) return null
      if (cashResult) {
        if (classifyIssuedTreatment(invoiceId, cashResult, null) === "blocked") throw new Error(t(locale, "cash.routeBlocked"))
        setCash(cashResult)
        return null
      }
      return invoiceIssueValidationGet({ invoiceId })
    }).then((result) => {
      if (current) { setRecord(result); setLoading(false) }
    }).catch((error) => {
      if (current) { setError(appErrorMessage(error, t(locale, "eligibility.historyFailed"))); setLoading(false) }
    })
    return () => { current = false }
  }, [invoiceId, locale])

  return <ActionReviewDialog open layout="large" title={t(locale, "eligibility.historyTitle")}
    summary={t(locale, "eligibility.historyNote")} consequences={[]} correction={t(locale, "eligibility.limit")}
    confirmLabel={t(locale, "eligibility.close")} cancelLabel={t(locale, "actionReview.cancel")}
    busy={false} onConfirm={onClose} onCancel={onClose}>
    <div className="invoice-eligibility" aria-busy={loading}>
      {loading ? <p role="status">{t(locale, "eligibility.loading")}</p> : null}
      {error ? <p role="alert">{error}</p> : !loading ? cash ? <>
        <h4>{t(locale, "cash.historyTitle")}</h4>
        <p>{t(locale, "cash.immutable")}</p>
        <CashFactsSummary title={t(locale, "cash.immutableOrigin")} facts={cash.origin} />
        <CashFactsSummary title={t(locale, "cash.originalIssueFacts")} facts={cash.issue.facts} />
        <CashEvidence assessment={cash.issue.assessment} />
        {cash.receipt ? <><CashFactsSummary title={t(locale, "cash.actualReceiptFacts")} facts={cash.receipt.facts} /><CashEvidence assessment={cash.receipt.assessment} /></> : <p>{t(locale, "cash.noReceipt")}</p>}
      </> : record ? <>
        <p>{tVars(locale, "eligibility.recorded", { date: record.recordedAt })}</p>
        <InvoiceEligibilityFactsView facts={record.facts} locale={locale} />
        <InvoiceEligibilityEvidence assessment={record.assessment} locale={locale} />
      </> : <p>{t(locale, "eligibility.noHistory")}</p> : null}
    </div>
  </ActionReviewDialog>
}
