import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { useEffect, useRef, useState } from "react"
import { open } from "@tauri-apps/plugin-dialog"
import { Link, useSearchParams } from "react-router-dom"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import { helpTopics } from "../lib/helpTopics"
import { resolveInvoicePaymentPanelState } from "../lib/documentsPaymentPanel"
import {
  appErrorMessage,
  accountList,
  csvImportCreate,
  documentImport,
  documentList,
  expensePost,
  invoiceList,
  invoicePaymentRecord,
  reconciliationMatchCreate,
  stagedTransactionsList,
  type AccountSummary,
  type Document,
  type InvoiceSummary,
  type StagedTransactionSummary,
} from "../lib/commands"
import { parseSekToMinorUnits } from "../lib/money"
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
  const [expenseAccountNumber, setExpenseAccountNumber] = useState("5610")
  const [expenseAccounts, setExpenseAccounts] = useState<AccountSummary[]>([])
  const [selectedDocumentId, setSelectedDocumentId] = useState("")
  const [noDocumentReason, setNoDocumentReason] = useState("")
  const docKeysRef = useRef<Record<string, string>>({})
  const csvKeysRef = useRef<Record<string, string>>({})
  const matchKeysRef = useRef<Record<string, string>>({})
  const expenseKeysRef = useRef<Record<string, string>>({})
  const paymentKeysRef = useRef<Record<string, string>>({})
  const [paymentDocumentId, setPaymentDocumentId] = useState("")
  const [paymentDocumentName, setPaymentDocumentName] = useState("")
  const [paymentDate, setPaymentDate] = useState("")
  const [inboxLoaded, setInboxLoaded] = useState(false)
  const [paymentRecorded, setPaymentRecorded] = useState(false)

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
    setInboxLoaded(true)
  }

  useEffect(() => {
    if (!workspace) return
    refreshInbox().catch(() => setStatus(t(locale, "documents.loadFailed")))
    accountList()
      .then((accounts) =>
        setExpenseAccounts(accounts.filter((account) => account.accountType === "expense")),
      )
      .catch(() => setExpenseAccounts([]))
  }, [workspace, locale])

  useEffect(() => {
    if (invoiceIdFromUrl) {
      setSelectedInvoiceId(invoiceIdFromUrl)
      setPaymentRecorded(false)
      setStatus(t(locale, "documents.invoicePaymentHint"))
      const invoice = invoices.find((row) => row.id === invoiceIdFromUrl)
      if (invoice?.issueDate) {
        setPaymentDate((current) => current || invoice.issueDate!)
      }
    }
  }, [invoiceIdFromUrl, locale, invoices])

  async function handlePickBankStatement() {
    if (!workspace || busy) return
    setBusy(true)
    try {
      const selection = await open({
        multiple: false,
        title: t(locale, "documents.pickBankStatement"),
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      })
      if (!selection || Array.isArray(selection)) return

      const idempotencyKey = docKeysRef.current[selection] ??= crypto.randomUUID()
      const filename = selection.split("/").pop() ?? "bank-statement.pdf"
      const imported = await documentImport({
        sourcePath: selection,
        filename,
        mimeType: "application/pdf",
        idempotencyKey,
      })

      delete docKeysRef.current[selection]
      setPaymentDocumentId(imported.id)
      setPaymentDocumentName(imported.originalFilename)
      setStatus(tVars(locale, "documents.bankStatementSelected", { name: imported.originalFilename }))
      await refreshInbox()
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "documents.importFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleRecordInvoicePayment() {
    if (busy || !invoiceIdFromUrl || paymentRecorded) return
    if (!paymentDocumentId) {
      setStatus(t(locale, "documents.invoicePaymentNeedsStatement"))
      return
    }
    setBusy(true)
    const idempotencyKey = paymentKeysRef.current[invoiceIdFromUrl] ??= crypto.randomUUID()
    try {
      const result = await invoicePaymentRecord({
        invoiceId: invoiceIdFromUrl,
        documentId: paymentDocumentId,
        paymentDate: paymentDate.trim() || null,
        idempotencyKey,
      })
      delete paymentKeysRef.current[invoiceIdFromUrl]
      setPaymentRecorded(true)
      setPaymentDocumentId("")
      setPaymentDocumentName("")
      setPaymentDate("")
      setStatus(
        result.voucherId
          ? `${t(locale, "documents.invoicePaymentComplete")} (${result.voucherId})`
          : t(locale, "documents.invoicePaymentComplete"),
      )
      await refreshInbox()
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "documents.invoicePaymentRecordFailed")))
    } finally {
      setBusy(false)
    }
  }

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
    const amountMinorExVat = parseSekToMinorUnits(expenseAmountSek)
    if (amountMinorExVat === null || amountMinorExVat <= 0) {
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
        expenseAccountNumber,
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
  const invoiceFromUrl = invoices.find((row) => row.id === invoiceIdFromUrl) ?? null
  const paymentPanelState = resolveInvoicePaymentPanelState({
    invoiceIdFromUrl,
    inboxLoaded,
    paymentRecorded,
    invoice: invoiceFromUrl,
  })
  const paymentPanelActionable = paymentPanelState === "ready"

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
          {invoiceIdFromUrl ? (
            <div className="panel" role="note">
              <h3>{t(locale, "documents.invoicePaymentWithStatementTitle")}</h3>
              {paymentPanelState === "completed" ? (
                <>
                  <p className="muted" role="status">
                    {t(locale, "documents.invoicePaymentComplete")}
                  </p>
                  <Link to="/invoices">{t(locale, "documents.backToInvoices")}</Link>
                </>
              ) : paymentPanelState === "loading" ? (
                <p className="muted" role="status">
                  {t(locale, "documents.invoicePaymentLoading")}
                </p>
              ) : paymentPanelState === "already_paid" ? (
                <p className="muted" role="status">
                  {t(locale, "documents.invoiceAlreadyPaid")}
                </p>
              ) : paymentPanelState === "not_found" ? (
                <p className="muted" role="status">
                  {t(locale, "documents.invoiceNotPayable")}
                </p>
              ) : (
                <>
                  {invoiceFromUrl ? (
                    <p className="muted">
                      {invoiceFromUrl.invoiceNumber ?? invoiceFromUrl.id} ·{" "}
                      {formatSek(invoiceFromUrl.totalIncVatMinor)}
                    </p>
                  ) : null}
                  <div className="button-row">
                    <button
                      type="button"
                      onClick={() => void handlePickBankStatement()}
                      disabled={busy || !workspace}
                    >
                      {t(locale, "documents.pickBankStatement")}
                    </button>
                  </div>
                  {paymentDocumentName ? (
                    <p className="muted">
                      {tVars(locale, "documents.bankStatementSelected", {
                        name: paymentDocumentName,
                      })}
                    </p>
                  ) : null}
                  <label>
                    {t(locale, "documents.paymentDate")}
                    <input
                      type="date"
                      value={paymentDate}
                      onChange={(event) => setPaymentDate(event.target.value)}
                      disabled={busy}
                    />
                  </label>
                  <button
                    type="button"
                    disabled={busy || !paymentDocumentId || !paymentPanelActionable}
                    onClick={() => void handleRecordInvoicePayment()}
                  >
                    {t(locale, "documents.recordInvoicePayment")}
                  </button>
                  <p className="muted">{t(locale, "documents.orUseCsv")}</p>
                </>
              )}
            </div>
          ) : null}

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
                {staged.length === 0 ? (
                  <tr>
                    <td colSpan={3} className="muted">
                      {t(locale, "documents.stagedEmpty")}
                    </td>
                  </tr>
                ) : null}
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
                {t(locale, "documents.expenseAccount")}
                <select
                  value={expenseAccountNumber}
                  onChange={(event) => setExpenseAccountNumber(event.target.value)}
                  disabled={busy || expenseAccounts.length === 0}
                >
                  {expenseAccounts.map((account) => (
                    <option key={account.id} value={account.number}>
                      {account.number} · {account.name}
                    </option>
                  ))}
                </select>
              </label>
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
                {matched.length === 0 ? (
                  <tr>
                    <td colSpan={3} className="muted">
                      {t(locale, "documents.matchedEmpty")}
                    </td>
                  </tr>
                ) : null}
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
