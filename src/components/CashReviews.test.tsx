import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import dialogStyles from "../styles.css?inline"
import { beforeEach, expect, it, vi } from "vitest"
import { InvoiceRecognitionHistoryDialog } from "./InvoiceRecognitionHistoryDialog"
import { t, type Locale } from "../i18n"
import { CashIssueDialog } from "./CashIssueDialog"
import { CashEvidence, CashFactsSummary } from "./CashEvidence"
import { open } from "@tauri-apps/plugin-dialog"
import { CashReceiptDialog } from "./CashReceiptDialog"
import { LocaleProvider } from "../context/LocaleContext"
import { emptyCashIssueFacts, emptyCashOriginFacts, emptyCashReceiptFacts, cashFactText } from "../lib/cashPresentation"
import type { CashInvoiceHistory, InvoiceSummary } from "../lib/bindings"
import { cashHistory as history } from "../test/cashFixtures"
const mocks = vi.hoisted(() => ({ origin: vi.fn(), preflight: vi.fn(), issue: vi.fn(), receipt: vi.fn(), source: vi.fn(), importPdf: vi.fn(), history: vi.fn(), invoiceHistory: vi.fn() }))
vi.mock("../lib/commands", async () => ({
  ...await vi.importActual<typeof import("../lib/commands")>("../lib/commands"),
  cashBusinessOriginGet: mocks.origin, invoiceCashIssuePreflight: mocks.preflight,
  invoiceCashIssue: mocks.issue, invoiceCashReceiptRecord: mocks.receipt,
  invoiceValidationSourceOpen: mocks.source, documentImport: mocks.importPdf,
  invoiceCashHistoryGet: mocks.history, invoiceIssueValidationGet: mocks.invoiceHistory,
}))
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }))
const invoice: InvoiceSummary = { id: "i", counterpartyId: "c", counterpartyName: "Stale customer", status: "draft", invoiceKind: "standard", invoiceNumber: null, sourceInvoiceId: null, issueDate: null, dueDate: null, totalExVatMinor: 10000, totalVatMinor: 2500, totalIncVatMinor: 12500, pdfJobId: null, pdfDocumentId: null, voucherId: null, paymentVoucherId: null, lines: [] }

function expectBoundedCashDialog() {
  const stylesheet = document.createElement("style")
  stylesheet.textContent = dialogStyles
  document.head.append(stylesheet)
  try {
    const style = getComputedStyle(screen.getByRole("dialog"))
    expect(style.maxHeight).toBe("85vh")
    expect(style.overflowY).toBe("auto")
    expect(style.maxWidth).toBe("700px")
  } finally {
    stylesheet.remove()
  }
}

beforeEach(() => { vi.clearAllMocks(); mocks.origin.mockResolvedValue(null) })
it("starts every cash fact unknown, including actual amount, and displays null as unknown", () => {
  expect(Object.values(emptyCashIssueFacts()).every(v => v === null)).toBe(true)
  expect(Object.values(emptyCashOriginFacts()).every(v => v === null)).toBe(true)
  expect(Object.values(emptyCashReceiptFacts())).toHaveLength(9)
  expect(Object.values(emptyCashReceiptFacts()).every(v => v === null)).toBe(true)
  expect(cashFactText(null)).toBe("Unknown")
})
it("requires explicit issue and first-year review; an empty workspace is never proof", async () => {
  render(<LocaleProvider initialLocale="en"><CashIssueDialog invoice={invoice} idempotencyKey="key" onCancel={vi.fn()} onIssued={vi.fn()} onBusyChange={vi.fn()} /></LocaleProvider>)
  await screen.findByLabelText("Actual business start date")
  expect(screen.getByLabelText("Issue date")).toHaveValue("")
  expect(screen.getByLabelText("Actual business start date")).toHaveValue("")
  expect(screen.getByLabelText("First financial year ends on")).toHaveValue("")
  fireEvent.click(screen.getByRole("button", { name: "Review cash issue" }))
  expect(mocks.preflight).not.toHaveBeenCalled()
  expect(await screen.findByRole("alert")).toHaveTextContent("Answer")
})
it("uses backend reviewed invoice, preserves facts on stale failure and requires fresh preflight", async () => {
  mocks.origin.mockResolvedValue(history.origin)
  mocks.preflight.mockResolvedValue({ invoiceId: "i", issueDate: "2026-06-01", assessment: history.issue.assessment, reviewedInvoice: { invoiceId: "i", counterpartyName: "Fresh customer", totalIncVatMinor: 12500, totalVatMinor: 2500, totalExVatMinor: 10000, dueDate: null, lines: [] }, currentTaxableBaseMinor: 0, projectedTaxableBaseMinor: 10000 })
  mocks.issue.mockRejectedValue(new Error("stale"))
  render(<LocaleProvider initialLocale="en"><CashIssueDialog invoice={invoice} idempotencyKey="key" onCancel={vi.fn()} onIssued={vi.fn()} onBusyChange={vi.fn()} /></LocaleProvider>)
  await screen.findByText(/Verified origin/)
  expectBoundedCashDialog()
  fireEvent.change(screen.getByLabelText("Issue date"), { target: { value: "2026-06-01" } })
  fireEvent.change(screen.getByLabelText("Service completed on"), { target: { value: "2026-05-31" } })
  for (const select of screen.getAllByRole("combobox")) fireEvent.change(select, { target: { value: select.getAttribute("data-field") === "priorPaymentReceived" ? "no" : "yes" } })
  fireEvent.click(screen.getByRole("button", { name: "Review cash issue" }))
  await screen.findByText(/Fresh customer/)
  expectBoundedCashDialog()
  expect(screen.getAllByText(/Not recognized/).length).toBeGreaterThan(0)
  fireEvent.click(screen.getByRole("button", { name: "Issue without recognition" }))
  await screen.findByRole("alert")
  expect(screen.getByLabelText("Issue date")).toHaveValue("2026-06-01")
  expect(screen.queryByRole("button", { name: "Issue without recognition" })).not.toBeInTheDocument()
  expect(mocks.issue).toHaveBeenCalledWith(expect.objectContaining({ reviewFingerprint: "fingerprint", originReview: null }))
})
it("receipt never copies invoice total, preserves retained evidence and facts on error", async () => {
  mocks.receipt.mockRejectedValue(new Error("receipt unavailable"))
  render(<LocaleProvider initialLocale="en"><CashReceiptDialog history={history} invoice={invoice} idempotencyKey="key" onClose={vi.fn()} onRecorded={vi.fn()} /></LocaleProvider>)
  expect(screen.getByLabelText("Actual received amount (SEK)")).toHaveValue("")
  expectBoundedCashDialog()
  const inputs: Record<string, string> = { "Retained bank PDF document ID": "pdf", "Actual received amount (SEK)": "125", "Actual received on": "2026-06-02", "Currency": "SEK", "Bank ID": "bank", "Receiving business account reference": "account", "Transaction reference": "tx", "Source entry locator (page / entry)": "page 1 entry 3" }
  for (const [label, value] of Object.entries(inputs)) fireEvent.change(screen.getByLabelText(label), { target: { value } })
  for (const select of screen.getAllByRole("combobox")) fireEvent.change(select, { target: { value: "yes" } })
  fireEvent.click(screen.getByRole("button", { name: "Review full receipt" }))
  await screen.findAllByText(/1930 debit/)
  expectBoundedCashDialog()
  fireEvent.click(screen.getByRole("button", { name: "Recognize full receipt" }))
  await screen.findByRole("alert")
  expect(screen.getByLabelText("Retained bank PDF document ID")).toHaveValue("pdf")
  expect(screen.getByLabelText("Actual received amount (SEK)")).toHaveValue("125")
  await waitFor(() => expect(mocks.receipt).toHaveBeenCalledWith({ invoiceId: "i", documentId: "pdf", idempotencyKey: "key", facts: { actualReceivedAmountMinor: 12500, actualReceivedOn: "2026-06-02", currency: "SEK", receivedAlready: true, bankReceiptIdentity: { bankId: "bank", receivingAccountReference: "account", transactionReference: "tx" }, sourceEntryLocator: "page 1 entry 3", bankPdfMatchesReceiptConfirmed: true, issuedCaseStillAppliesConfirmed: true, noOtherReceiptsOrAdjustmentsConfirmed: true } }))
})

it("requires and sends the real shortened first year and its turnover review", async () => {
  mocks.preflight.mockRejectedValue(new Error("blocked"))
  render(<LocaleProvider initialLocale="en"><CashIssueDialog invoice={invoice} idempotencyKey="key" onCancel={vi.fn()} onIssued={vi.fn()} onBusyChange={vi.fn()} /></LocaleProvider>)
  await waitFor(() => expect(screen.getByLabelText("Actual business start date")).toBeEnabled())
  for (const [label, value] of Object.entries({ "Actual business start date": "2026-10-01", "First financial year ends on": "2026-12-31", "Issue date": "2026-11-02", "Service completed on": "2026-11-01" })) fireEvent.change(screen.getByLabelText(label), { target: { value } })
  for (const select of screen.getAllByRole("combobox")) {
    const field = select.getAttribute("data-field")
    if (field !== "shortYearTurnoverAdjustmentReviewed") fireEvent.change(select, { target: { value: field === "priorPaymentReceived" ? "no" : "yes" } })
  }
  fireEvent.click(screen.getByRole("button", { name: "Review cash issue" }))
  expect(mocks.preflight).not.toHaveBeenCalled()
  fireEvent.change(screen.getByLabelText("Short first year: reviewed normal turnover on a 12-month basis?"), { target: { value: "yes" } })
  fireEvent.click(screen.getByRole("button", { name: "Review cash issue" }))
  await waitFor(() => expect(mocks.preflight).toHaveBeenCalledWith(expect.objectContaining({
    issueDate: "2026-11-02", facts: expect.objectContaining({ serviceCompletedOn: "2026-11-01", shortYearTurnoverAdjustmentReviewed: true }),
    originReview: { newBusinessStartedIn2026: true, businessStartedOn: "2026-10-01", firstFinancialYearEndsOn: "2026-12-31", zeroOpeningBalancesConfirmed: true, noOpeningUnpaidInvoicesOrBillsConfirmed: true, noPriorUnrecordedActivityConfirmed: true, outsideWorkspaceHistoryReviewed: true },
  })))
  expect(screen.getByLabelText("Actual business start date")).toHaveValue("2026-10-01")
})
it("does not turn an origin read error into proof of an empty workspace", async () => {
  mocks.origin.mockRejectedValue(new Error("offline"))
  render(<LocaleProvider initialLocale="en"><CashIssueDialog invoice={invoice} idempotencyKey="key" onCancel={vi.fn()} onIssued={vi.fn()} onBusyChange={vi.fn()} /></LocaleProvider>)
  await screen.findByRole("alert")
  fireEvent.click(screen.getByRole("button", { name: "Review cash issue" }))
  expect(mocks.preflight).not.toHaveBeenCalled()
})
it("keeps cash sources ID-only including all clock sources; null facts remain Unknown", async () => {
  mocks.source.mockResolvedValue(undefined)
  render(<LocaleProvider initialLocale="en"><CashFactsSummary title="Source facts" facts={{ ordinaryDomesticService: null }} /><CashEvidence assessment={history.issue.assessment} /></LocaleProvider>)
  expect(screen.getAllByText("Unknown").length).toBeGreaterThan(0)
  expect(screen.getAllByRole("link")).toHaveLength(12)
  fireEvent.click(screen.getByRole("link", { name: "Commission schedule 2021/C 149/01" }))
  await waitFor(() => expect(mocks.source).toHaveBeenCalledWith({ validationId: history.issue.assessment.validationId, sourceId: "eu-summer-time-2026" }))
})
it("imports a retained bank PDF without inferring any actual receipt facts", async () => {
  vi.mocked(open).mockResolvedValue("/bank/statement.pdf")
  mocks.importPdf.mockResolvedValue({ id: "retained-pdf" })
  render(<LocaleProvider initialLocale="en"><CashReceiptDialog history={history} invoice={invoice} idempotencyKey="key" onClose={vi.fn()} onRecorded={vi.fn()} /></LocaleProvider>)
  fireEvent.click(screen.getByRole("button", { name: "Pick bank statement (PDF)" }))
  await waitFor(() => expect(screen.getByLabelText("Retained bank PDF document ID")).toHaveValue("retained-pdf"))
  expect(mocks.importPdf).toHaveBeenCalledWith(expect.objectContaining({ sourcePath: "/bank/statement.pdf", filename: "statement.pdf", mimeType: "application/pdf" }))
  expect(screen.getByLabelText("Actual received amount (SEK)")).toHaveValue("")
  expect(screen.getByLabelText("Actual received on")).toHaveValue("")
  expect(screen.getByLabelText("Currency")).toHaveValue("")
  fireEvent.click(screen.getByRole("button", { name: "Review full receipt" }))
  expect(mocks.receipt).not.toHaveBeenCalled()
})

const locales: Locale[] = ["sv", "en"]
it.each(locales)("shows localized %s cash history facts, uncertainty, nested bank identity and receipt accounting date", async (locale) => {
  const retained: CashInvoiceHistory = {
    ...history,
    origin: { ...history.origin, facts: { ...history.origin.facts, newBusinessStartedIn2026: true, noPriorUnrecordedActivityConfirmed: false } },
    receipt: {
      invoiceId: history.invoiceId, recordedAt: "2026-06-02", result: { matchId: "match", voucherId: "voucher" },
      facts: { ...emptyCashReceiptFacts(), actualReceivedAmountMinor: 12500, actualReceivedOn: "2026-06-02", bankReceiptIdentity: { bankId: "bank-id", receivingAccountReference: "account-id", transactionReference: "tx-id" } },
      assessment: { ...history.issue.assessment, accountingDate: "2026-06-02", documentId: "pdf-id", documentSha256: "sha256" },
    },
  }
  mocks.history.mockResolvedValue(retained)
  render(<LocaleProvider initialLocale={locale}><InvoiceRecognitionHistoryDialog invoiceId={history.invoiceId} onClose={vi.fn()} /></LocaleProvider>)
  expect(await screen.findByRole("heading", { name: locale === "sv" ? "Ursprungliga uppgifter vid utfärdande" : "Original issue facts" })).toBeInTheDocument()
  expect(screen.getByText(t(locale, "cash.ordinaryDomesticService"))).toBeInTheDocument()
  expect(screen.getAllByText(locale === "sv" ? "Osäker" : "Unknown").length).toBeGreaterThan(0)
  expect(screen.getByText(locale === "sv" ? "Ja" : "Yes")).toBeInTheDocument()
  expect(screen.getByText(locale === "sv" ? "Nej" : "No")).toBeInTheDocument()
  expect(screen.getByText(t(locale, "cash.receivingAccountReference"))).toBeInTheDocument()
  expect(screen.getByText("account-id")).toBeInTheDocument()
  expect(screen.getByText(locale === "sv" ? "Bokföringsdatum för betalningen: 2026-06-02" : "Receipt accounting date: 2026-06-02")).toBeInTheDocument()
  expect(screen.getByText(locale === "sv" ? "Inte bokförd" : "Not recognized")).toBeInTheDocument()
  expect(screen.queryByText("ordinaryDomesticService")).not.toBeInTheDocument()
  expect(screen.queryByText("bankReceiptIdentity")).not.toBeInTheDocument()
  expect(screen.queryByText(/English DTO summary|Recognition accounting date:/)).not.toBeInTheDocument()
})
it.each(locales)("localizes the %s receipt review, including bank identity and section labels", async (locale) => {
  render(<LocaleProvider initialLocale={locale}><CashReceiptDialog history={history} invoice={invoice} idempotencyKey="key" onClose={vi.fn()} onRecorded={vi.fn()} /></LocaleProvider>)
  const inputs = [
    { label: t(locale, "cash.documentId"), value: "pdf" },
    { label: t(locale, "cash.actualReceivedAmountMinor"), value: "125" },
    { label: t(locale, "cash.actualReceivedOn"), value: "2026-06-02" },
    { label: t(locale, "cash.currency"), value: "SEK" },
    { label: t(locale, "cash.bankId"), value: "bank" },
    { label: t(locale, "cash.receivingAccountReference"), value: "account" },
    { label: t(locale, "cash.transactionReference"), value: "tx" },
    { label: t(locale, "cash.sourceEntryLocator"), value: "page 1" },
  ]
  for (const { label, value } of inputs) fireEvent.change(screen.getByLabelText(label), { target: { value } })
  for (const select of screen.getAllByRole("combobox")) fireEvent.change(select, { target: { value: "yes" } })
  fireEvent.click(screen.getByRole("button", { name: t(locale, "cash.reviewReceipt") }))
  expect(await screen.findByRole("heading", { name: locale === "sv" ? "Faktisk betalning — ägarens granskning, inte bankautentisering" : "Actual receipt facts — owner review, not bank authentication" })).toBeInTheDocument()
  expect(screen.getByText(t(locale, "cash.bankId"))).toBeInTheDocument()
  expect(screen.getByText("bank")).toBeInTheDocument()
  expect(screen.getByText(t(locale, "cash.actualReceivedOn"))).toBeInTheDocument()
  expect(screen.getAllByText(locale === "sv" ? "Ja" : "Yes")).toHaveLength(4)
  expect(screen.queryByText("receivedAlready")).not.toBeInTheDocument()
  expect(screen.getAllByText(locale === "sv" ? /1930 debet/ : /1930 debit/).length).toBeGreaterThan(0)
})
