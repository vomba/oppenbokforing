import { open } from "@tauri-apps/plugin-dialog"
import { useEffect, useRef, useState } from "react"
import { ActionReviewDialog } from "./ActionReviewDialog"
import { CashFactsSummary } from "./CashEvidence"
import { CashFactChoice } from "./CashFactChoice"
import { useLocale } from "../context/LocaleContext"
import { t, tVars, type MessageKey } from "../i18n"
import type { CashInvoiceHistory, InvoiceSummary } from "../lib/bindings"
import { appErrorMessage, documentImport, documentReveal, invoiceCashReceiptRecord } from "../lib/commands"
import { cashReceiptQuestions, emptyCashReceiptFacts } from "../lib/cashPresentation"
import { formatSekMinor, parseSekToMinorUnits } from "../lib/money"

export function CashReceiptDialog({ history, invoice, idempotencyKey, onClose, onRecorded }: {
  history: CashInvoiceHistory; invoice: InvoiceSummary; idempotencyKey: string; onClose: () => void; onRecorded: () => void
}) {
  const { locale } = useLocale()
  const [facts, setFacts] = useState(emptyCashReceiptFacts)
  const [amount, setAmount] = useState("")
  const [documentId, setDocumentId] = useState("")
  const [identity, setIdentity] = useState({ bankId: "", receivingAccountReference: "", transactionReference: "" })
  const [review, setReview] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const importKeys = useRef<Record<string, string>>({})
  const pending = useRef(false)
  const alive = useRef(true)
  useEffect(() => { alive.current = true; return () => { alive.current = false } }, [])
  async function pickPdf() {
    if (pending.current) return
    pending.current = true; setBusy(true); setError("")
    try {
      const path = await open({ multiple: false, title: t(locale, "documents.pickBankStatement"), filters: [{ name: "PDF", extensions: ["pdf"] }] })
      if (!path || Array.isArray(path) || !alive.current) return
      const imported = await documentImport({ sourcePath: path, filename: path.split("/").pop() ?? "bank-statement.pdf", mimeType: "application/pdf", idempotencyKey: importKeys.current[path] ??= crypto.randomUUID() })
      if (alive.current) { setDocumentId(imported.id); delete importKeys.current[path] }
    } catch (error) { if (alive.current) setError(appErrorMessage(error, t(locale, "documents.importFailed"))) }
    finally { pending.current = false; if (alive.current) setBusy(false) }
  }
  async function revealPdf() {
    if (pending.current || !documentId.trim()) return
    pending.current = true; setBusy(true)
    try { await documentReveal({ documentId: documentId.trim() }) }
    catch (error) { if (alive.current) setError(appErrorMessage(error, t(locale, "documents.importFailed"))) }
    finally { pending.current = false; if (alive.current) setBusy(false) }
  }
  async function confirm() {
    if (pending.current) return
    if (!review) {
      if (!documentId.trim() || Object.values(facts).some(value => value === null || value === "") || facts.actualReceivedAmountMinor === null || facts.actualReceivedAmountMinor <= 0) { setError(t(locale, "cash.incomplete")); return }
      if (facts.currency !== "SEK" || facts.actualReceivedAmountMinor !== invoice.totalIncVatMinor || cashReceiptQuestions.some(q => facts[q.field] !== true)) { setError(t(locale, "cash.blocked")); return }
      setError(""); setReview(true); return
    }
    pending.current = true; setBusy(true); setError("")
    try {
      await invoiceCashReceiptRecord({ invoiceId: history.invoiceId, documentId: documentId.trim(), idempotencyKey, facts })
      if (alive.current) onRecorded()
    } catch (error) {
      if (alive.current) { setReview(false); setError(`${t(locale, "cash.fresh")} ${appErrorMessage(error, t(locale, "cash.blocked"))}`) }
    } finally { pending.current = false; if (alive.current) setBusy(false) }
  }
  const identityFields: ReadonlyArray<{ field: keyof typeof identity; label: MessageKey }> = [
    { field: "bankId", label: "cash.bankId" }, { field: "receivingAccountReference", label: "cash.receivingAccountReference" }, { field: "transactionReference", label: "cash.transactionReference" },
  ]
  return <ActionReviewDialog open layout="large" title={t(locale, "cash.receiptTitle")} summary={`${invoice.invoiceNumber ?? history.invoiceId} · ${t(locale, "cash.receiptEvidence")}`} consequences={review ? [t(locale, "cash.receiptEffect"), `${facts.actualReceivedOn} · ${formatSekMinor(facts.actualReceivedAmountMinor ?? 0)}`] : []} correction={t(locale, "cash.limits")} confirmLabel={t(locale, review ? "cash.confirmReceipt" : "cash.reviewReceipt")} cancelLabel={t(locale, "actionReview.cancel")} busy={busy} onConfirm={() => void confirm()} onCancel={() => { if (!pending.current) onClose() }}>
    <p>{t(locale, "cash.onePdf")}</p>
    {review ? <>
      <p>{t(locale, "cash.documentId")}: {documentId}</p>
      <p>{tVars(locale, "cash.receiptPosting", { date: facts.actualReceivedOn ?? t(locale, "cash.unknown"), amount: formatSekMinor(facts.actualReceivedAmountMinor ?? 0), net: formatSekMinor(invoice.totalExVatMinor), vat: formatSekMinor(invoice.totalVatMinor) })}</p>
      <CashFactsSummary title={t(locale, "cash.receiptOwnerReview")} facts={facts} />
      <button type="button" disabled={busy} onClick={() => setReview(false)}>{t(locale, "eligibility.edit")}</button>
    </> : <fieldset disabled={busy}>
      <p>{t(locale, "cash.expectedAmount")}: {formatSekMinor(invoice.totalIncVatMinor)}</p>
      <button type="button" onClick={() => void pickPdf()}>{t(locale, "documents.pickBankStatement")}</button>
      <button type="button" disabled={!documentId.trim()} onClick={() => void revealPdf()}>{t(locale, "cash.openPdf")}</button>
      <label>{t(locale, "cash.documentId")}<input value={documentId} onChange={e => setDocumentId(e.target.value)} /></label>
      <label>{t(locale, "cash.actualReceivedAmountMinor")}<input inputMode="decimal" value={amount} onChange={e => { setAmount(e.target.value); setFacts({ ...facts, actualReceivedAmountMinor: e.target.value.trim() ? parseSekToMinorUnits(e.target.value) : null }) }} /></label>
      <label>{t(locale, "cash.actualReceivedOn")}<input type="date" value={facts.actualReceivedOn ?? ""} onChange={e => setFacts({ ...facts, actualReceivedOn: e.target.value || null })} /></label>
      <label>{t(locale, "cash.currency")}<input value={facts.currency ?? ""} onChange={e => setFacts({ ...facts, currency: e.target.value.trim() || null })} /></label>
      {identityFields.map(({ field, label }) => <label key={field}>{t(locale, label)}<input value={identity[field]} onChange={e => {
        const next = { ...identity, [field]: e.target.value }
        setIdentity(next); setFacts({ ...facts, bankReceiptIdentity: Object.values(next).every(value => value.trim()) ? next : null })
      }} /></label>)}
      <label>{t(locale, "cash.sourceEntryLocator")}<input value={facts.sourceEntryLocator ?? ""} onChange={e => setFacts({ ...facts, sourceEntryLocator: e.target.value || null })} /></label>
      {cashReceiptQuestions.map(q => <CashFactChoice key={q.field} {...q} value={facts[q.field]} onChange={value => setFacts({ ...facts, [q.field]: value })} />)}
    </fieldset>}
    {error ? <p role="alert">{error}</p> : null}
  </ActionReviewDialog>
}
