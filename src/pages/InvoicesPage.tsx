import { Link } from "react-router-dom"
import { useEffect, useRef, useState } from "react"
import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
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
  invoiceIssue,
  invoiceIssuePreflight,
  invoiceList,
  taxProfileGetCurrent,
  invoiceLegacySnapshotRecover,
  invoiceLegacySnapshotRecoveryStatus,
  invoicePdfRefresh,
  invoicePdfStatus,
  type Counterparty,
  type InvoiceSummary,
} from "../lib/commands"
import type { InvoiceIssuePreflight } from "../lib/bindings"
import { formatSekMinor, parseSekToMinorUnits } from "../lib/money"
import { invoiceDisplayStatus, invoiceStatusLabel, localTodayIsoDate } from "../lib/invoiceStatus"

type StatusFilter = "all" | "draft" | "issued"

type InvoiceReview = {
  kind: "issue" | "credit"
  invoice: InvoiceSummary
  preflight: InvoiceIssuePreflight | null
}

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

function isTaxStatusValidationError(error: unknown): boolean {
  if (
    !error ||
    typeof error !== "object" ||
    !("details" in error) ||
    !Array.isArray(error.details)
  ) {
    return false
  }
  return error.details.some(
    (detail) =>
      typeof detail === "object" &&
      detail !== null &&
      "field" in detail &&
      detail.field === "taxStatus",
  )
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
  const issueKeysRef = useRef<Record<string, string>>({})
  const creditKeysRef = useRef<Record<string, string>>({})

  useEffect(() => {
    setStatus(t(locale, "invoices.status"))
  }, [locale])

  async function refresh(filter: StatusFilter = statusFilter) {
    if (!workspace) return
    const [customerRows, invoiceRows, taxProfile] = await Promise.all([
      counterpartyList(),
      invoiceList({
        status: filter === "all" ? null : filter,
      }),
      taxProfileGetCurrent().catch(() => null),
    ])
    setTaxStatus(taxProfile?.taxStatus ?? null)
    const customerOnly = customerRows.filter((row) => row.kind === "customer")
    setCustomers(customerOnly)
    setInvoices(invoiceRows)
    const legacyRecoveryStatusRows = await Promise.allSettled(
      invoiceRows
        .filter((invoice) => invoice.status === "issued" || invoice.status === "credited")
        .map((invoice) => invoiceLegacySnapshotRecoveryStatus({ invoiceId: invoice.id })),
    )
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
      setStatus(
        isTaxStatusValidationError(error)
          ? t(locale, "invoices.taxStatusRequired")
          : appErrorMessage(error, t(locale, "invoices.issueFailed")),
      )
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
  async function openReview(kind: "issue" | "credit", invoice: InvoiceSummary) {
    if (busy) return
    if (kind === "credit") {
      setReview({ kind, invoice, preflight: null })
      return
    }
    if (taxStatus !== "f_skatt" && taxStatus !== "fa_skatt") {
      setStatus(t(locale, "invoices.taxStatusRequired"))
      return
    }


    setBusy(true)
    try {
      const preflight = await invoiceIssuePreflight({
        invoiceId: invoice.id,
        issueDate: null,
      })
      setReview({ kind, invoice, preflight })
    } catch (error) {
      setStatus(
        isTaxStatusValidationError(error)
          ? t(locale, "invoices.taxStatusRequired")
          : appErrorMessage(error, t(locale, "invoices.issueFailed")),
      )
    } finally {
      setBusy(false)
    }
  }

  function confirmReview() {
    if (!review) return
    const { kind, invoice, preflight } = review
    setReview(null)
    if (kind === "issue") {
      if (preflight?.requiresVatTreatmentReview) {
        setStatus(t(locale, "invoices.thresholdReviewAction"))
        return
      }
      void handleIssue(invoice.id)
      return
    }
    void handleCredit(invoice.id)
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
        {review ? (
          <ActionReviewDialog
            open
            title={t(
              locale,
              review.kind === "issue" ? "actionReview.issue.title" : "actionReview.credit.title",
            )}
            summary={
              review.kind === "issue"
                ? tVars(locale, "actionReview.issue.summary", {
                    customer: review.invoice.counterpartyName,
                    total: formatSekMinor(review.invoice.totalIncVatMinor),
                    vat: formatSekMinor(review.invoice.totalVatMinor),
                    rate:
                      review.invoice.totalExVatMinor > 0
                        ? Math.round((review.invoice.totalVatMinor / review.invoice.totalExVatMinor) * 100)
                        : 0,
                  })
                : `${tVars(locale, "actionReview.credit.summary", {
                    customer: review.invoice.counterpartyName,
                    total: formatSekMinor(review.invoice.totalIncVatMinor),
                  })} ${tVars(locale, "invoices.creditAccountingDate", {
                    date: creditIssueDate,
                  })}`
            }
            consequences={
              review.kind === "issue" && review.preflight?.requiresVatTreatmentReview
                ? [
                    t(locale, "invoices.thresholdReviewAction"),
                    tVars(locale, "invoices.thresholdProjectedTurnover", {
                      projected: formatSekMinor(review.preflight.projectedTurnoverMinor),
                      threshold: formatSekMinor(review.preflight.thresholdMinor ?? 0),
                    }),
                    tVars(locale, "invoices.thresholdRule", {
                      year: review.preflight.taxYear,
                      sourceUrl: review.preflight.sourceUrl ?? "",
                    }),
                  ]
                : [
                    t(
                      locale,
                      review.kind === "issue"
                        ? "actionReview.issue.consequence"
                        : "actionReview.credit.consequence",
                    ),
                  ]
            }
            correction={
              review.kind === "issue" && review.preflight?.requiresVatTreatmentReview
                ? t(locale, "invoices.thresholdReviewAction")
                : t(
                    locale,
                    review.kind === "issue"
                      ? "actionReview.issue.correction"
                      : "actionReview.credit.correction",
                  )
            }
            confirmLabel={
              review.kind === "issue" && review.preflight?.requiresVatTreatmentReview
                ? t(locale, "invoices.closeReview")
                : t(
                    locale,
                    review.kind === "issue"
                      ? "actionReview.issue.confirm"
                      : "actionReview.credit.confirm",
                  )
            }
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
