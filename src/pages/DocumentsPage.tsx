import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { useEffect, useRef, useState } from "react"
import { open } from "@tauri-apps/plugin-dialog"
import { useSearchParams } from "react-router-dom"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { t } from "../i18n"
import { helpTopics } from "../lib/helpTopics"
import {
  appErrorMessage,
  csvImportCreate,
  documentImport,
  documentList,
  expensePost,
  invoiceList,
  reconciliationMatchCreate,
  stagedTransactionsList,
  type Document,
  type InvoiceSummary,
  type StagedTransactionSummary,
} from "../lib/commands"
import { reconcileListSelection } from "../lib/workbenchSelection"

function formatSek(minor: number) {
  return `${(minor / 100).toLocaleString("sv-SE", { minimumFractionDigits: 2 })} kr`
}

export function DocumentsPage() {
  const { workspace } = useWorkspace()
  const { locale } = useLocale()
  const [searchParams] = useSearchParams()
  const invoiceIdFromUrl = searchParams.get("invoiceId")
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState(t(locale, "documents.status"))
  const [staged, setStaged] = useState<StagedTransactionSummary[]>([])
  const [matched, setMatched] = useState<StagedTransactionSummary[]>([])
  const [invoices, setInvoices] = useState<InvoiceSummary[]>([])
  const [documents, setDocuments] = useState<Document[]>([])
  const [selectedStagedId, setSelectedStagedId] = useState("")
  const [selectedInvoiceId, setSelectedInvoiceId] = useState("")
  const [expenseAmountSek, setExpenseAmountSek] = useState("500")
  const [expenseVatRate, setExpenseVatRate] = useState("0.25")
  const [selectedDocumentId, setSelectedDocumentId] = useState("")
  const [noDocumentReason, setNoDocumentReason] = useState("")
  const docKeysRef = useRef<Record<string, string>>({})
  const csvKeysRef = useRef<Record<string, string>>({})
  const matchKeysRef = useRef<Record<string, string>>({})
  const expenseKeysRef = useRef<Record<string, string>>({})

  async function refreshInbox() {
    if (!workspace) return
    const [stagedRows, matchedRows, invoiceRows, documentRows] = await Promise.all([
      stagedTransactionsList({ status: "staged", limit: 100, beforeId: null }),
      stagedTransactionsList({ status: "matched", limit: 20, beforeId: null }),
      invoiceList({ status: "issued" }),
      documentList({ unattachedOnly: true, limit: 100, beforeId: null }),
    ])
    setStaged(stagedRows)
    setMatched(matchedRows)
    setInvoices(invoiceRows)
    setDocuments(documentRows)
    setSelectedStagedId((current) => reconcileListSelection(current, stagedRows))
    setSelectedInvoiceId((current) => reconcileListSelection(current, invoiceRows))
    setSelectedDocumentId((current) => reconcileListSelection(current, documentRows))
  }

  useEffect(() => {
    if (!workspace) return
    refreshInbox().catch(() => setStatus(t(locale, "documents.loadFailed")))
  }, [workspace, locale])

  useEffect(() => {
    if (invoiceIdFromUrl) {
      setSelectedInvoiceId(invoiceIdFromUrl)
      setStatus(t(locale, "documents.invoicePaymentHint"))
    }
  }, [invoiceIdFromUrl, locale])

  async function handlePickAndImportDocument() {
    if (!workspace || busy) return
    setBusy(true)
    try {
      const selection = await open({
        multiple: false,
        title: t(locale, "documents.pickEvidence"),
      })
      if (!selection || Array.isArray(selection)) return

      const idempotencyKey = docKeysRef.current[selection] ??= crypto.randomUUID()
      const filename = selection.split("/").pop() ?? "evidence"
      const imported = await documentImport({
        sourcePath: selection,
        filename,
        mimeType: "application/octet-stream",
        idempotencyKey,
      })

      delete docKeysRef.current[selection]
      setStatus(`${t(locale, "documents.documentImported")}: ${imported.originalFilename}`)
      await refreshInbox()
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "documents.importFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handlePickAndImportCsv() {
    if (!workspace || busy) return
    setBusy(true)
    try {
      const selection = await open({
        multiple: false,
        title: t(locale, "documents.pickCsv"),
        filters: [{ name: "CSV", extensions: ["csv"] }],
      })
      if (!selection || Array.isArray(selection)) return

      const idempotencyKey = csvKeysRef.current[selection] ??= crypto.randomUUID()
      const result = await csvImportCreate({
        sourcePath: selection,
        idempotencyKey,
      })

      delete csvKeysRef.current[selection]
      setStatus(`${t(locale, "documents.csvImported")}: ${result.stagedCount}`)
      await refreshInbox()
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "documents.csvFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleMatchInvoicePayment() {
    if (busy || !selectedStagedId || !selectedInvoiceId) return
    setBusy(true)
    const idempotencyKey = matchKeysRef.current[selectedStagedId] ??= crypto.randomUUID()
    try {
      const result = await reconciliationMatchCreate({
        stagedTransactionId: selectedStagedId,
        matchKind: "invoice_payment",
        invoiceId: selectedInvoiceId,
        idempotencyKey,
      })
      delete matchKeysRef.current[selectedStagedId]
      setStatus(
        result.voucherId
          ? `${t(locale, "documents.matchDone")}: ${result.voucherId}`
          : t(locale, "documents.matchDone"),
      )
      setSelectedStagedId("")
      await refreshInbox()
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "documents.matchFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handlePostExpense() {
    if (busy || !selectedStagedId) return
    const amountMinorExVat = Math.round(Number(expenseAmountSek) * 100)
    if (!Number.isFinite(amountMinorExVat) || amountMinorExVat <= 0) {
      setStatus(t(locale, "documents.invalidAmount"))
      return
    }
    if (!selectedDocumentId && !noDocumentReason.trim()) {
      setStatus(t(locale, "documents.documentOrReasonRequired"))
      return
    }

    setBusy(true)
    const idempotencyKey = expenseKeysRef.current[selectedStagedId] ??= crypto.randomUUID()
    try {
      const stagedRow = staged.find((row) => row.id === selectedStagedId)
      const result = await expensePost({
        amountMinorExVat,
        vatRate: Number(expenseVatRate),
        expenseAccountNumber: "5610",
        paymentAccountNumber: "1930",
        documentId: selectedDocumentId || null,
        noDocumentReason: noDocumentReason.trim() || null,
        stagedTransactionId: selectedStagedId,
        idempotencyKey,
        date: stagedRow?.transactionDate ?? null,
      })
      delete expenseKeysRef.current[selectedStagedId]
      setStatus(
        result.voucherId
          ? `${t(locale, "documents.expensePosted")}: ${result.voucherId}`
          : t(locale, "documents.expensePosted"),
      )
      setSelectedStagedId("")
      await refreshInbox()
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "documents.expenseFailed")))
    } finally {
      setBusy(false)
    }
  }

  const selectedStaged = staged.find((row) => row.id === selectedStagedId) ?? null

  return (
    <main className="app-shell">
      <AppSidebar current="documents" />

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{workspace?.name ?? "Workspace"}</p>
            <h2>
              {t(locale, helpTopics.documents.title)}
              <HelpTip label={t(locale, helpTopics.documents.title)}>
                {t(locale, helpTopics.documents.help)}
              </HelpTip>
            </h2>
            <p className="status-line" aria-live="polite">
              {status}
            </p>
          </div>
        </header>

        <section className="workbench">
          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "documents.imports")}</p>
              <h3>{t(locale, "documents.importEvidence")}</h3>
            </header>
            <div className="button-row">
              <button type="button" onClick={handlePickAndImportDocument} disabled={busy || !workspace}>
                {t(locale, "documents.pickEvidence")}
              </button>
              <button type="button" onClick={handlePickAndImportCsv} disabled={busy || !workspace}>
                {t(locale, "documents.pickCsv")}
              </button>
            </div>
          </div>

          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "documents.inbox")}</p>
              <h3>{t(locale, "documents.stagedTransactions")}</h3>
            </header>
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">{t(locale, "documents.date")}</th>
                  <th scope="col">{t(locale, "documents.description")}</th>
                  <th scope="col">{t(locale, "documents.amount")}</th>
                </tr>
              </thead>
              <tbody>
                {staged.map((row) => (
                  <tr key={row.id} className={selectedStagedId === row.id ? "selected-row" : undefined}>
                    <td>
                      <button
                        type="button"
                        className="link-button"
                        disabled={busy}
                        aria-pressed={selectedStagedId === row.id}
                        onClick={() => setSelectedStagedId(row.id)}
                      >
                        {row.transactionDate}
                      </button>
                    </td>
                    <td>{row.description}</td>
                    <td>{formatSek(row.amountMinor)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {selectedStaged ? (
            <div className="panel">
              <header>
                <p className="eyebrow">{t(locale, "documents.reconcile")}</p>
                <h3>{selectedStaged.description}</h3>
              </header>
              <p className="muted">
                {formatSek(selectedStaged.amountMinor)} · {selectedStaged.transactionDate}
              </p>

              <label>
                {t(locale, "documents.matchInvoice")}
                <select
                  value={selectedInvoiceId}
                  onChange={(event) => setSelectedInvoiceId(event.target.value)}
                  disabled={busy || invoices.length === 0}
                >
                  {invoices.map((invoice) => (
                    <option key={invoice.id} value={invoice.id}>
                      {invoice.invoiceNumber ?? invoice.id} · {formatSek(invoice.totalIncVatMinor)}
                    </option>
                  ))}
                </select>
              </label>
              <button
                type="button"
                disabled={busy || !selectedInvoiceId}
                onClick={() => void handleMatchInvoicePayment()}
              >
                {t(locale, "documents.matchAsPayment")}
              </button>

              <hr />

              <h4>{t(locale, "documents.postExpense")}</h4>
              <label>
                {t(locale, "documents.amountExVat")}
                <input
                  value={expenseAmountSek}
                  onChange={(event) => setExpenseAmountSek(event.target.value)}
                  disabled={busy}
                />
              </label>
              <label>
                {t(locale, "documents.vatRate")}
                <input
                  value={expenseVatRate}
                  onChange={(event) => setExpenseVatRate(event.target.value)}
                  disabled={busy}
                />
              </label>
              <label>
                {t(locale, "documents.evidenceDocument")}
                <select
                  value={selectedDocumentId}
                  onChange={(event) => setSelectedDocumentId(event.target.value)}
                  disabled={busy}
                >
                  <option value="">{t(locale, "documents.noDocumentSelected")}</option>
                  {documents.map((document) => (
                    <option key={document.id} value={document.id}>
                      {document.originalFilename}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                {t(locale, "documents.noDocumentReason")}
                <input
                  value={noDocumentReason}
                  onChange={(event) => setNoDocumentReason(event.target.value)}
                  disabled={busy}
                />
              </label>
              <button type="button" disabled={busy} onClick={() => void handlePostExpense()}>
                {t(locale, "documents.postExpenseAction")}
              </button>
            </div>
          ) : null}

          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "documents.history")}</p>
              <h3>{t(locale, "documents.matchedTransactions")}</h3>
            </header>
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">{t(locale, "documents.date")}</th>
                  <th scope="col">{t(locale, "documents.description")}</th>
                  <th scope="col">{t(locale, "documents.amount")}</th>
                </tr>
              </thead>
              <tbody>
                {matched.map((row) => (
                  <tr key={row.id}>
                    <td>{row.transactionDate}</td>
                    <td>{row.description}</td>
                    <td>{formatSek(row.amountMinor)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      </section>
    </main>
  )
}
