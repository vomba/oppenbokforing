import { useEffect, useState } from "react"
import { ActionReviewDialog } from "./ActionReviewDialog"
import { InvoiceEligibilityEvidence, InvoiceEligibilityFactsView } from "./InvoiceEligibilityEvidence"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import { appErrorMessage, invoiceIssueValidationGet } from "../lib/commands"

type ValidationRecord = Awaited<ReturnType<typeof invoiceIssueValidationGet>>

export function InvoiceRecognitionHistoryDialog({ invoiceId, onClose }: {
  invoiceId: string
  onClose: () => void
}) {
  const { locale } = useLocale()
  const [record, setRecord] = useState<ValidationRecord>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState("")
  useEffect(() => {
    let current = true
    setLoading(true)
    setError("")
    setRecord(null)
    invoiceIssueValidationGet({ invoiceId }).then((result) => {
      if (current) { setRecord(result); setLoading(false) }
    }).catch((error) => {
      if (current) { setError(appErrorMessage(error, t(locale, "eligibility.historyFailed"))); setLoading(false) }
    })
    return () => { current = false }
  }, [invoiceId, locale])

  return <ActionReviewDialog open title={t(locale, "eligibility.historyTitle")}
    summary={t(locale, "eligibility.historyNote")} consequences={[]} correction={t(locale, "eligibility.limit")}
    confirmLabel={t(locale, "eligibility.close")} cancelLabel={t(locale, "actionReview.cancel")}
    busy={false} onConfirm={onClose} onCancel={onClose}>
    <div className="invoice-eligibility" aria-busy={loading}>
      {loading ? <p role="status">{t(locale, "eligibility.loading")}</p> : null}
      {error ? <p role="alert">{error}</p> : !loading ? record ? <>
        <p>{tVars(locale, "eligibility.recorded", { date: record.recordedAt })}</p>
        <InvoiceEligibilityFactsView facts={record.facts} locale={locale} />
        <InvoiceEligibilityEvidence assessment={record.assessment} locale={locale} />
      </> : <p>{t(locale, "eligibility.noHistory")}</p> : null}
    </div>
  </ActionReviewDialog>
}
