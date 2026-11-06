import { Link } from "react-router-dom"
import { useEffect, useRef, useState } from "react"
import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { CashIssueDialog } from "../components/CashIssueDialog"
import { InvoiceEligibilityDialog } from "../components/InvoiceEligibilityDialog"
import { InvoiceRecognitionHistoryDialog } from "../components/InvoiceRecognitionHistoryDialog"
import { ActionReviewDialog } from "../components/ActionReviewDialog"
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
  invoiceList,
  taxProfileGetCurrent,
  vatProfileGetCurrent,
  invoiceLegacySnapshotRecover,
  invoiceLegacySnapshotRecoveryStatus,
  invoicePdfRefresh,
  invoicePdfStatus,
  type Counterparty,
  type InvoiceSummary,
} from "../lib/commands"
import { formatSekMinor, parseSekToMinorUnits } from "../lib/money"
import { invoiceDisplayStatus, invoiceStatusLabel, localTodayIsoDate } from "../lib/invoiceStatus"

type StatusFilter = "all" | "draft" | "issued"

type InvoiceReview = { invoice: InvoiceSummary }
type IssueReview = { invoice: InvoiceSummary; workspaceId: string; idempotencyKey: string }

type LegacySnapshotRecoveryReview = {
  invoice: InvoiceSummary
  preservedPdfDocumentId: string
}

type LegacySnapshotRecoveryForm = {
  businessName: string
  ownerName: string
  taxStatus: string
  vatStatus: string
  ruleVersionId: string
}

const LEGACY_SNAPSHOT_ATTESTATION =
  "I attest that the business identity and displayed tax/VAT wording were transcribed from the retained original invoice PDF, and that any status distinctions and the rule version were checked against contemporaneous records."

const emptyLegacySnapshotRecoveryForm: LegacySnapshotRecoveryForm = {
  businessName: "",
  ownerName: "",
  taxStatus: "",
  vatStatus: "",
  ruleVersionId: "",
}

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
  const [accountingMethod, setAccountingMethod] = useState<string | null>(null)
  const [taxStatus, setTaxStatus] = useState<string | null>(null)
  const [creditIssueDate, setCreditIssueDate] = useState(localTodayIsoDate)
  const [busy, setBusy] = useState(false)
  const [legacyRecoveryStatuses, setLegacyRecoveryStatuses] = useState<
    Record<string, { recoveryRequired: boolean; preservedPdfDocumentId: string | null }>
  >({})
  const [legacyRecoveryReview, setLegacyRecoveryReview] =
    useState<LegacySnapshotRecoveryReview | null>(null)
  const [legacyRecoveryForm, setLegacyRecoveryForm] = useState<LegacySnapshotRecoveryForm>(
    emptyLegacySnapshotRecoveryForm,
  )
  const [legacyRecoveryAttested, setLegacyRecoveryAttested] = useState(false)
  const [reviewedLegacyPdfDocumentIds, setReviewedLegacyPdfDocumentIds] = useState<
    Record<string, true>
  >({})
  const [legacyRecoveryError, setLegacyRecoveryError] = useState("")
  const [review, setReview] = useState<InvoiceReview | null>(null)
  const [issueReview, setIssueReview] = useState<IssueReview | null>(null)
  const [historyReview, setHistoryReview] = useState<{ invoice: InvoiceSummary; workspaceId: string } | null>(null)
  const issueKeysRef = useRef<Record<string, string>>({})
  const creditKeysRef = useRef<Record<string, string>>({})
  const workspaceIdRef = useRef(workspace?.id)
  workspaceIdRef.current = workspace?.id
  useEffect(() => {
    setIssueReview(null)
    setHistoryReview(null)
    setReview(null)
    setBusy(false)
    setInvoices([])
    setCustomers([])
    setTaxStatus(null)
    setAccountingMethod(null)
    issueKeysRef.current = {}
  }, [workspace?.id])

  useEffect(() => {
    setStatus(t(locale, "invoices.status"))
  }, [locale])

  async function refresh(filter: StatusFilter = statusFilter) {
    if (!workspace) return
    const requestedWorkspaceId = workspace.id
    const [customerRows, invoiceRows, taxProfile, vatProfile] = await Promise.all([
      counterpartyList(),
      invoiceList({
        status: filter === "all" ? null : filter,
      }),
      taxProfileGetCurrent().catch(() => null),
      vatProfileGetCurrent().catch(() => null),
    ])
    if (workspaceIdRef.current !== requestedWorkspaceId) return
    setTaxStatus(taxProfile?.taxStatus ?? null)
    setAccountingMethod(vatProfile?.accountingMethod ?? null)
    const customerOnly = customerRows.filter((row) => row.kind === "customer")
    setCustomers(customerOnly)
    setInvoices(invoiceRows)
    const legacyRecoveryStatusRows = await Promise.allSettled(
      invoiceRows
        .filter((invoice) => invoice.status === "issued" || invoice.status === "credited")
        .map((invoice) => invoiceLegacySnapshotRecoveryStatus({ invoiceId: invoice.id })),
    )
    if (workspaceIdRef.current !== requestedWorkspaceId) return
    setLegacyRecoveryStatuses(
      Object.fromEntries(
        legacyRecoveryStatusRows.flatMap((result) =>
          result.status === "fulfilled" ? [[result.value.invoiceId, result.value] as const] : [],
        ),
      ),
    )
    if (!selectedCustomerId && customerOnly.length > 0) {
      setSelectedCustomerId(customerOnly[0].id)
    }
  }

  useEffect(() => {
    const requestedWorkspaceId = workspace?.id
    refresh().catch(() => {
      if (workspaceIdRef.current !== requestedWorkspaceId) return
      setCustomers([])
      setInvoices([])
    })
  }, [workspace, statusFilter])

  useEffect(() => {
    if (!issueReview || issueReview.workspaceId !== workspace?.id) return
    const current = invoices.find((invoice) => invoice.id === issueReview.invoice.id)
    if (!current || JSON.stringify(current) !== JSON.stringify(issueReview.invoice)) {
      setIssueReview(null)
      setBusy(false)
    }
  }, [invoices, issueReview, workspace?.id])

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
    const unitPriceMinor = parseSekToMinorUnits(amountSek)
    if (unitPriceMinor === null || unitPriceMinor <= 0) {
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
        issueDate: creditIssueDate,
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
      const pdfStatus =
        invoice.status === "issued" || invoice.status === "credited"
          ? await invoicePdfRefresh({ invoiceId: invoice.id })
          : await invoicePdfStatus({ invoiceId: invoice.id })
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


  function updateLegacyRecoveryForm(
    field: keyof LegacySnapshotRecoveryForm,
    value: string,
  ) {
    setLegacyRecoveryForm((current) => ({ ...current, [field]: value }))
  }

  async function handleOpenPreservedPdf(documentId: string) {
    if (busy) return
    setBusy(true)
    try {
      await documentReveal({ documentId })
      setReviewedLegacyPdfDocumentIds((current) => ({ ...current, [documentId]: true }))
      setStatus(t(locale, "invoices.pdfOpened"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "invoices.pdfFailed")))
    } finally {
      setBusy(false)
    }
  }

  function openLegacyRecoveryReview(invoice: InvoiceSummary, preservedPdfDocumentId: string) {
    if (busy) return
    setLegacyRecoveryForm(emptyLegacySnapshotRecoveryForm)
    setLegacyRecoveryAttested(false)
    setLegacyRecoveryError("")
    setLegacyRecoveryReview({ invoice, preservedPdfDocumentId })
  }

  async function confirmLegacyRecovery() {
    if (!legacyRecoveryReview || busy) return
    const { invoice, preservedPdfDocumentId } = legacyRecoveryReview
    const hasReviewedPreservedPdf = reviewedLegacyPdfDocumentIds[preservedPdfDocumentId] === true
    const hasRequiredValues = Object.values(legacyRecoveryForm).every((value) => value.trim())
    if (!hasReviewedPreservedPdf) {
      setLegacyRecoveryError(t(locale, "invoices.legacyRecoveryPdfReviewRequired"))
      return
    }
    if (!hasRequiredValues || !legacyRecoveryAttested) {
      setLegacyRecoveryError(t(locale, "invoices.legacyRecoveryAttestationRequired"))
      return
    }

    setBusy(true)
    setLegacyRecoveryError("")
    try {
      await invoiceLegacySnapshotRecover({
        invoiceId: invoice.id,
        documentId: preservedPdfDocumentId,
        ...legacyRecoveryForm,
        attestation: LEGACY_SNAPSHOT_ATTESTATION,
      })
      await refresh()
      setLegacyRecoveryReview(null)
      setStatus(t(locale, "invoices.legacyRecoveryComplete"))
    } catch (error) {
      setLegacyRecoveryError(
        appErrorMessage(error, t(locale, "invoices.legacyRecoveryFailed")),
      )
    } finally {
      setBusy(false)
    }
  }
  function openReview(kind: "issue" | "credit", invoice: InvoiceSummary) {
    if (busy || !workspace) return
    if (kind === "credit") {
      setReview({ invoice })
      return
    }
    if (taxStatus !== "f_skatt" && taxStatus !== "fa_skatt") {
      setStatus(t(locale, "invoices.taxStatusRequired"))
      return
    }
    const idempotencyKey = issueKeysRef.current[invoice.id] ??= crypto.randomUUID()
    setIssueReview({ invoice, workspaceId: workspace.id, idempotencyKey })
  }

  function confirmReview() {
    if (!review) return
    const { invoice } = review
    setReview(null)
    void handleCredit(invoice.id)
  }

  const IssueDialog = accountingMethod === "cash_method" ? CashIssueDialog : InvoiceEligibilityDialog

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
            <label className="inline-filter">
              {t(locale, "invoices.creditIssueDate")}
              <input
                type="date"
                aria-label={t(locale, "invoices.creditIssueDate")}
                value={creditIssueDate}
                onChange={(event) => setCreditIssueDate(event.target.value)}
                disabled={busy}
              />
            </label>
          </div>
          {taxStatus === "fa_skatt" ? (
            <p className="muted">{t(locale, "invoices.faSkattPdfNote")}</p>
          ) : null}
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
                  const legacyRecoveryStatus = legacyRecoveryStatuses[invoice.id]
                  const preservedPdfDocumentId =
                    legacyRecoveryStatus?.recoveryRequired
                      ? legacyRecoveryStatus.preservedPdfDocumentId
                      : null
                  const canMarkPaid =
                    displayStatus === "issued" || displayStatus === "overdue"
                  return (
                    <tr key={invoice.id}>
                      <td>{invoice.invoiceNumber ?? "—"}</td>
                      <td>{invoice.counterpartyName}</td>
                      <td>{invoiceStatusLabel(locale, displayStatus)}</td>
                      <td>{formatSekMinor(invoice.totalIncVatMinor)}</td>
                      <td className="table-actions">
                        {invoice.status === "draft" ? (
                          <button
                            type="button"
                            onClick={() => openReview("issue", invoice)}
                            disabled={busy || (accountingMethod !== "cash_method" && accountingMethod !== "invoice_method")}
                          >
                            {t(locale, "invoices.issue")}
                          </button>
                        ) : null}
                        {invoice.status === "issued" &&
                        invoice.invoiceKind === "standard" &&
                        invoice.voucherId !== null &&
                        !invoice.paymentVoucherId ? (
                          <button
                            type="button"
                            className="secondary"
                            onClick={() => openReview("credit", invoice)}
                            disabled={busy}
                          >
                            {t(locale, "invoices.credit")}
                          </button>
                        ) : null}
                        {legacyRecoveryStatus?.recoveryRequired ? (
                          <div className="muted">
                            <p>
                              <strong>{t(locale, "invoices.legacyRecoveryRequired")}</strong>
                            </p>
                            <p>{t(locale, "invoices.legacyRecoveryExplanation")}</p>
                            {preservedPdfDocumentId ? (
                              <>
                                <button
                                  type="button"
                                  className="secondary"
                                  onClick={() => void handleOpenPreservedPdf(preservedPdfDocumentId)}
                                  disabled={busy}
                                >
                                  {t(locale, "invoices.openPreservedPdf")}
                                </button>
                                <button
                                  type="button"
                                  onClick={() =>
                                    openLegacyRecoveryReview(invoice, preservedPdfDocumentId)
                                  }
                                  disabled={busy}
                                >
                                  {t(locale, "invoices.legacyRecoveryAction")}
                                </button>
                              </>
                            ) : (
                              <p>{t(locale, "invoices.legacyRecoveryPdfUnavailable")}</p>
                            )}
                          </div>
                        ) : invoice.status === "issued" || invoice.status === "credited" ? (
                          <button
                            type="button"
                            className="secondary"
                            onClick={() => void handlePreviewPdf(invoice)}
                            disabled={busy}
                          >
                            {t(locale, "invoices.previewPdf")}
                          </button>
                        ) : null}
                        {invoice.status === "issued" || invoice.status === "credited" ? (
                          <button type="button" className="secondary" disabled={busy}
                            onClick={() => workspace && setHistoryReview({ invoice, workspaceId: workspace.id })}>
                            {t(locale, "eligibility.history")}
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
        {issueReview && issueReview.workspaceId === workspace?.id && (accountingMethod === "cash_method" || accountingMethod === "invoice_method") ? (
          <IssueDialog
            key={`${issueReview.workspaceId}:${issueReview.invoice.id}`}
            invoice={issueReview.invoice}
            idempotencyKey={issueReview.idempotencyKey}
            onBusyChange={setBusy}
            onCancel={() => setIssueReview(null)}
            onIssued={(issued) => {
              if (workspaceIdRef.current !== issueReview.workspaceId) return
              delete issueKeysRef.current[issued.id]
              setIssueReview(null)
              setBusy(false)
              void refresh().catch(() => undefined)
              setStatus(tVars(locale, "invoices.issued", { number: issued.invoiceNumber ?? issued.id }))
            }}
          />
        ) : null}
        {historyReview && historyReview.workspaceId === workspace?.id ? (
          <InvoiceRecognitionHistoryDialog
            key={`${historyReview.workspaceId}:${historyReview.invoice.id}`}
            invoiceId={historyReview.invoice.id}
            onClose={() => setHistoryReview(null)}
          />
        ) : null}
        {review ? (
          <ActionReviewDialog
            open
            title={t(locale, "actionReview.credit.title")}
            summary={`${tVars(locale, "actionReview.credit.summary", {
              customer: review.invoice.counterpartyName,
              total: formatSekMinor(review.invoice.totalIncVatMinor),
            })} ${tVars(locale, "invoices.creditAccountingDate", { date: creditIssueDate })}`}
            consequences={[t(locale, "actionReview.credit.consequence")]}
            correction={t(locale, "actionReview.credit.correction")}
            confirmLabel={t(locale, "actionReview.credit.confirm")}
            cancelLabel={t(locale, "actionReview.cancel")}
            busy={busy}
            onConfirm={confirmReview}
            onCancel={() => setReview(null)}
          />
        ) : null}
        {legacyRecoveryReview ? (
          <ActionReviewDialog
            open
            title={t(locale, "invoices.legacyRecoveryReviewTitle")}
            summary={tVars(locale, "invoices.legacyRecoveryReviewSummary", {
              invoice: legacyRecoveryReview.invoice.invoiceNumber ?? legacyRecoveryReview.invoice.id,
            })}
            consequences={[
              t(locale, "invoices.legacyRecoveryConsequence"),
              t(locale, "invoices.legacyRecoveryNotCorrection"),
            ]}
            correction={t(locale, "invoices.legacyRecoveryCorrection")}
            confirmLabel={t(locale, "invoices.legacyRecoveryConfirm")}
            cancelLabel={t(locale, "actionReview.cancel")}
            busy={busy}
            onConfirm={() => void confirmLegacyRecovery()}
            onCancel={() => setLegacyRecoveryReview(null)}
          >
            <p>{t(locale, "invoices.legacyRecoveryInputGuidance")}</p>
            <button
              type="button"
              className="secondary"
              onClick={() =>
                void handleOpenPreservedPdf(legacyRecoveryReview.preservedPdfDocumentId)
              }
              disabled={busy}
            >
              {t(locale, "invoices.openPreservedPdf")}
            </button>
            <label>
              {t(locale, "invoices.legacyRecoveryBusinessName")}
              <input
                value={legacyRecoveryForm.businessName}
                onChange={(event) => updateLegacyRecoveryForm("businessName", event.target.value)}
                disabled={busy}
              />
            </label>
            <label>
              {t(locale, "invoices.legacyRecoveryOwnerName")}
              <input
                value={legacyRecoveryForm.ownerName}
                onChange={(event) => updateLegacyRecoveryForm("ownerName", event.target.value)}
                disabled={busy}
              />
            </label>
            <label>
              {t(locale, "invoices.legacyRecoveryTaxStatus")}
              <select
                value={legacyRecoveryForm.taxStatus}
                onChange={(event) => updateLegacyRecoveryForm("taxStatus", event.target.value)}
                disabled={busy}
              >
                <option value="">{t(locale, "invoices.legacyRecoverySelect")}</option>
                <option value="f_skatt">F-skatt</option>
                <option value="fa_skatt">FA-skatt</option>
              </select>
            </label>
            <label>
              {t(locale, "invoices.legacyRecoveryVatStatus")}
              <select
                value={legacyRecoveryForm.vatStatus}
                onChange={(event) => updateLegacyRecoveryForm("vatStatus", event.target.value)}
                disabled={busy}
              >
                <option value="">{t(locale, "invoices.legacyRecoverySelect")}</option>
                <option value="registered">{t(locale, "invoices.legacyRecoveryVatRegistered")}</option>
                <option value="voluntary_registered">
                  {t(locale, "invoices.legacyRecoveryVatVoluntaryRegistered")}
                </option>
                <option value="exempt_low_turnover">
                  {t(locale, "invoices.legacyRecoveryVatExempt")}
                </option>
              </select>
            </label>
            <label>
              {t(locale, "invoices.legacyRecoveryRuleVersion")}
              <input
                value={legacyRecoveryForm.ruleVersionId}
                onChange={(event) => updateLegacyRecoveryForm("ruleVersionId", event.target.value)}
                disabled={busy}
              />
            </label>
            <label>
              <input
                type="checkbox"
                checked={legacyRecoveryAttested}
                onChange={(event) => setLegacyRecoveryAttested(event.target.checked)}
                disabled={busy}
              />
              {t(locale, "invoices.legacyRecoveryAttestation")}
            </label>
            {legacyRecoveryError ? <p role="alert">{legacyRecoveryError}</p> : null}
          </ActionReviewDialog>
        ) : null}
      </section>
    </main>
  )
}
