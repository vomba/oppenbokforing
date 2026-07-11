import { Link } from "react-router-dom"
import { useEffect, useRef, useState } from "react"
import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { VoucherTraceLink } from "../components/VoucherTraceLink"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import { helpTopics } from "../lib/helpTopics"
import {
  appErrorMessage,
  counterpartyCreate,
  counterpartyList,
  documentReveal,
  invoiceCreateDraft,
  invoiceCredit,
  invoiceIssue,
  invoiceList,
  invoicePdfStatus,
  type Counterparty,
  type InvoiceSummary,
} from "../lib/commands"
import { invoiceDisplayStatus, invoiceStatusLabel } from "../lib/invoiceStatus"

function formatMinor(minor: number) {
  return (minor / 100).toFixed(2)
}

type StatusFilter = "all" | "draft" | "issued"

export function InvoicesPage() {
  const { workspace } = useWorkspace()
  const { locale } = useLocale()
  const [customers, setCustomers] = useState<Counterparty[]>([])
  const [invoices, setInvoices] = useState<InvoiceSummary[]>([])
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all")
  const [customerName, setCustomerName] = useState("")
  const [selectedCustomerId, setSelectedCustomerId] = useState("")
  const [description, setDescription] = useState("Consulting services")
  const [amountSek, setAmountSek] = useState("10000")
  const [vatRate, setVatRate] = useState("0.25")
  const [status, setStatus] = useState("")
  const [busy, setBusy] = useState(false)
  const issueKeysRef = useRef<Record<string, string>>({})
  const creditKeysRef = useRef<Record<string, string>>({})

  useEffect(() => {
    setStatus(t(locale, "invoices.status"))
  }, [locale])

  async function refresh(filter: StatusFilter = statusFilter) {
    if (!workspace) return
    const [customerRows, invoiceRows] = await Promise.all([
      counterpartyList(),
      invoiceList({
        status: filter === "all" ? null : filter,
      }),
    ])
    const customerOnly = customerRows.filter((row) => row.kind === "customer")
    setCustomers(customerOnly)
    setInvoices(invoiceRows)
    if (!selectedCustomerId && customerOnly.length > 0) {
      setSelectedCustomerId(customerOnly[0].id)
    }
  }

  useEffect(() => {
    refresh().catch(() => {
      setCustomers([])
      setInvoices([])
    })
  }, [workspace, statusFilter])

  async function handleCreateCustomer() {
    if (busy || !customerName.trim()) return
    setBusy(true)
    try {
      const created = await counterpartyCreate({
        kind: "customer",
        name: customerName.trim(),
        email: null,
        orgNumber: null,
      })
      setCustomerName("")
      setSelectedCustomerId(created.id)
      await refresh()
      setStatus(tVars(locale, "invoices.customerCreated", { name: created.name }))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "invoices.customerFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleCreateDraft() {
    if (busy || !selectedCustomerId) return
    const unitPriceMinor = Math.round(Number(amountSek) * 100)
    if (!Number.isFinite(unitPriceMinor) || unitPriceMinor <= 0) {
      setStatus(t(locale, "invoices.invalidAmount"))
      return
    }
    const rate = Number(vatRate)
    if (!Number.isFinite(rate) || rate < 0 || rate > 1) {
      setStatus(t(locale, "invoices.invalidVat"))
      return
    }

    setBusy(true)
    try {
      await invoiceCreateDraft({
        counterpartyId: selectedCustomerId,
        dueDate: null,
        lines: [
          {
            description,
            quantity: 1,
            unitPriceMinor,
            vatRate: rate,
            accountNumber: "3041",
          },
        ],
      })
      await refresh()
      setStatus(t(locale, "invoices.draftCreated"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "invoices.draftFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleIssue(invoiceId: string) {
    if (busy) return
    const idempotencyKey = issueKeysRef.current[invoiceId] ??= crypto.randomUUID()
    setBusy(true)
    try {
      const issued = await invoiceIssue({
        invoiceId,
        idempotencyKey,
        issueDate: null,
      })
      delete issueKeysRef.current[invoiceId]
      await refresh()
      setStatus(
        tVars(locale, "invoices.issued", { number: issued.invoiceNumber ?? issued.id }),
      )
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "invoices.issueFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleCredit(sourceInvoiceId: string) {
    if (busy) return
    const idempotencyKey =
      creditKeysRef.current[sourceInvoiceId] ??= crypto.randomUUID()
    setBusy(true)
    try {
      const credited = await invoiceCredit({
        sourceInvoiceId,
        idempotencyKey,
        reason: "Customer correction",
      })
      delete creditKeysRef.current[sourceInvoiceId]
      await refresh()
      setStatus(
        tVars(locale, "invoices.credited", { number: credited.invoiceNumber ?? credited.id }),
      )
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "invoices.creditFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handlePreviewPdf(invoice: InvoiceSummary) {
    if (busy) return
    setBusy(true)
    try {
      const pdfStatus = await invoicePdfStatus({ invoiceId: invoice.id })
      if (pdfStatus !== "succeeded") {
        setStatus(t(locale, "invoices.pdfNotReady"))
        return
      }
      const refreshed = await invoiceList({ status: statusFilter === "all" ? null : statusFilter })
      const latest = refreshed.find((row) => row.id === invoice.id)
      const documentId = latest?.pdfDocumentId ?? invoice.pdfDocumentId
      if (!documentId) {
        setStatus(t(locale, "invoices.pdfNotReady"))
        return
      }
      await documentReveal({ documentId })
      setStatus(t(locale, "invoices.pdfOpened"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "invoices.pdfFailed")))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="app-shell">
      <AppSidebar current="invoices" />

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{t(locale, "invoices.eyebrow")}</p>
            <h2>
              {t(locale, helpTopics.invoices.title)}
              <HelpTip label={t(locale, helpTopics.invoices.title)}>
                {t(locale, helpTopics.invoices.help)}
              </HelpTip>
            </h2>
            <p className="status-line">{status}</p>
          </div>
          <Link to="/dashboard">{t(locale, "invoices.back")}</Link>
        </header>

        <div className="onboarding-grid">
          <section className="panel">
            <h3>{t(locale, "invoices.customers")}</h3>
            <div className="form-row">
              <input
                aria-label={t(locale, "invoices.customerName")}
                value={customerName}
                onChange={(e) => setCustomerName(e.target.value)}
                placeholder={t(locale, "invoices.customerName")}
                disabled={busy}
              />
              <button type="button" onClick={handleCreateCustomer} disabled={busy}>
                {t(locale, "invoices.addCustomer")}
              </button>
            </div>
            <ul className="recent-list">
              {customers.map((customer) => (
                <li key={customer.id}>
                  <button
                    type="button"
                    className={customer.id === selectedCustomerId ? "secondary" : ""}
                    onClick={() => setSelectedCustomerId(customer.id)}
                  >
                    {customer.name}
                  </button>
                </li>
              ))}
            </ul>
          </section>

          <section className="panel">
            <h3>{t(locale, "invoices.newDraft")}</h3>
            <label>
              {t(locale, "invoices.description")}
              <input value={description} onChange={(e) => setDescription(e.target.value)} />
            </label>
            <label>
              {t(locale, "invoices.amountSek")}
              <input value={amountSek} onChange={(e) => setAmountSek(e.target.value)} />
            </label>
            <label>
              {t(locale, "invoices.vatRate")}
              <input value={vatRate} onChange={(e) => setVatRate(e.target.value)} />
            </label>
            <button type="button" onClick={handleCreateDraft} disabled={busy || !selectedCustomerId}>
              {t(locale, "invoices.createDraft")}
            </button>
          </section>
        </div>

        <section className="panel">
          <div className="panel-header-row">
            <h3>{t(locale, "invoices.list")}</h3>
            <label className="inline-filter">
              {t(locale, "invoices.filter")}
              <select
                value={statusFilter}
                onChange={(e) => setStatusFilter(e.target.value as StatusFilter)}
                disabled={busy}
              >
                <option value="all">{t(locale, "invoices.filterAll")}</option>
                <option value="draft">{t(locale, "invoices.status.draft")}</option>
                <option value="issued">{t(locale, "invoices.status.issued")}</option>
              </select>
            </label>
          </div>
          {invoices.length === 0 ? (
            <p className="muted">{t(locale, "invoices.empty")}</p>
          ) : (
            <table className="data-table">
              <thead>
                <tr>
                  <th>{t(locale, "invoices.number")}</th>
                  <th>{t(locale, "invoices.customer")}</th>
                  <th>{t(locale, "invoices.statusCol")}</th>
                  <th>{t(locale, "invoices.total")}</th>
                  <th>{t(locale, "invoices.actions")}</th>
                </tr>
              </thead>
              <tbody>
                {invoices.map((invoice) => {
                  const displayStatus = invoiceDisplayStatus(invoice)
                  const canMarkPaid =
                    displayStatus === "issued" || displayStatus === "overdue"
                  return (
                    <tr key={invoice.id}>
                      <td>{invoice.invoiceNumber ?? "—"}</td>
                      <td>{invoice.counterpartyName}</td>
                      <td>{invoiceStatusLabel(locale, displayStatus)}</td>
                      <td>{formatMinor(invoice.totalIncVatMinor)}</td>
                      <td className="table-actions">
                        {invoice.status === "draft" ? (
                          <button
                            type="button"
                            onClick={() => handleIssue(invoice.id)}
                            disabled={busy}
                          >
                            {t(locale, "invoices.issue")}
                          </button>
                        ) : null}
                        {invoice.status === "issued" &&
                        invoice.invoiceKind === "standard" &&
                        !invoice.paymentVoucherId ? (
                          <button
                            type="button"
                            className="secondary"
                            onClick={() => handleCredit(invoice.id)}
                            disabled={busy}
                          >
                            {t(locale, "invoices.credit")}
                          </button>
                        ) : null}
                        {invoice.status === "issued" ? (
                          <button
                            type="button"
                            className="secondary"
                            onClick={() => void handlePreviewPdf(invoice)}
                            disabled={busy}
                          >
                            {t(locale, "invoices.previewPdf")}
                          </button>
                        ) : null}
                        {canMarkPaid ? (
                          <Link
                            className="text-link"
                            to={`/documents?invoiceId=${encodeURIComponent(invoice.id)}`}
                          >
                            {t(locale, "invoices.markPaid")}
                          </Link>
                        ) : null}
                        {invoice.voucherId ? (
                          <VoucherTraceLink
                            voucherId={invoice.voucherId}
                            label={t(locale, "invoices.issueTrace")}
                          />
                        ) : null}
                        {invoice.paymentVoucherId ? (
                          <VoucherTraceLink
                            voucherId={invoice.paymentVoucherId}
                            label={t(locale, "invoices.paymentTrace")}
                          />
                        ) : null}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
        </section>
      </section>
    </main>
  )
}
