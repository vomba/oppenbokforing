import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { MemoryRouter } from "react-router-dom"
import { describe, expect, it, beforeEach, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { SimpleModeProvider } from "../context/SimpleModeContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import type { InvoiceSummary } from "../lib/commands"
import { localTodayIsoDate } from "../lib/invoiceStatus"
import { InvoicesPage } from "./InvoicesPage"

const workspace = {
  id: "ws-1",
  name: "Testfirma",
  dataDir: "/tmp/data",
  databasePath: "/tmp/workspace.sqlite",
}

let activeWorkspace = workspace

const baseInvoice: InvoiceSummary = {
  id: "inv-1",
  counterpartyId: "cust-1",
  counterpartyName: "Customer AB",
  status: "issued",
  invoiceKind: "standard",
  invoiceNumber: "2026-001",
  sourceInvoiceId: null,
  issueDate: "2026-01-15",
  dueDate: "2026-02-15",
  totalExVatMinor: 1_000_000,
  totalVatMinor: 250_000,
  totalIncVatMinor: 1_250_000,
  pdfJobId: null,
  pdfDocumentId: null,
  voucherId: "v-1",
  paymentVoucherId: null,
  lines: [],
}

const {
  invoiceList,
  invoiceIssue,
  invoiceIssuePreflight,
  invoiceIssueValidationGet,
  invoiceCredit,
  invoiceLegacySnapshotRecoveryStatus,
  invoiceLegacySnapshotRecover,
  invoicePdfRefresh,
  invoicePdfStatus,
  documentReveal,
  taxProfileGetCurrent,
} = vi.hoisted(() => ({
  invoiceList: vi.fn(),
  invoiceIssue: vi.fn(),
  invoiceIssuePreflight: vi.fn(),
  invoiceIssueValidationGet: vi.fn(),
  invoiceCredit: vi.fn(),
  invoiceLegacySnapshotRecoveryStatus: vi.fn(),
  invoiceLegacySnapshotRecover: vi.fn(),
  invoicePdfRefresh: vi.fn(),
  invoicePdfStatus: vi.fn(),
  documentReveal: vi.fn(),
  taxProfileGetCurrent: vi.fn(),
}))

vi.mock("../context/WorkspaceContext", async () => {
  const actual = await vi.importActual<typeof import("../context/WorkspaceContext")>(
    "../context/WorkspaceContext",
  )
  return {
    ...actual,
    useWorkspace: () => ({
      workspace: activeWorkspace,
      setWorkspace: vi.fn(),
    }),
  }
})

vi.mock("../components/AppSidebar", () => ({
  AppSidebar: () => <nav aria-label="sidebar" />,
}))

vi.mock("../lib/commands", () => ({
  appErrorMessage: (_error: unknown, fallback: string) => fallback,
  counterpartyCreate: vi.fn(),
  counterpartyList: vi.fn().mockResolvedValue([
    { id: "cust-1", kind: "customer", name: "Customer AB", email: null, orgNumber: null },
  ]),
  documentReveal,
  invoiceCreateDraft: vi.fn(),
  invoiceCredit,
  invoiceIssue,
  invoiceIssuePreflight,
  invoiceIssueValidationGet,
  invoiceList,
  invoiceLegacySnapshotRecoveryStatus,
  invoiceLegacySnapshotRecover,
  invoicePdfRefresh,
  invoicePdfStatus,
  taxProfileGetCurrent,
}))

function invoicesTree(locale: "sv" | "en" = "sv") {
  return (
    <MemoryRouter>
      <WorkspaceProvider>
        <LocaleProvider initialLocale={locale}>
          <SimpleModeProvider>
            <InvoicesPage />
          </SimpleModeProvider>
        </LocaleProvider>
      </WorkspaceProvider>
    </MemoryRouter>
  )
}

function renderInvoices(locale: "sv" | "en" = "sv") {
  return render(invoicesTree(locale))
}

const eligibleAssessment = {
  validationId: "invoice-recognition-se-service25-2026-exact-v1",
  evidenceDigest: "reviewed-evidence-digest",
  reviewFingerprint: "reviewed-input-fingerprint",
  accountingYear: 2026,
  accountingDate: "2026-01-15",
  completedOn: "2026-01-14",
  deadlineRegime: "annual_may_12",
  periodKey: "2026",
  dueOn: "2027-05-12",
  coverageSummary: "2026 domestic service, exact VAT per line",
  ruleVersionId: "rv-2026-domestic-service25-exact-v1",
}

const expectedFacts = {
  ordinaryDomesticService: true,
  serviceCompletedOn: "2026-01-14",
  priorPaymentReceived: false,
  noEuTradeInYear: true,
  annualVatRegistrationConfirmed: true,
  expectedTaxableBaseWithinLimit: true,
  ordinaryInvoiceTimingConfirmed: true,
}

function answerEligibility() {
  fireEvent.change(screen.getByLabelText("Fakturadatum (dagen du skickar fakturan)"), {
    target: { value: "2026-01-15" },
  })
  fireEvent.change(screen.getByLabelText("När slutfördes tjänsten?"), {
    target: { value: "2026-01-14" },
  })
  for (const group of screen.getAllByRole("group")) {
    const priorPayment = within(group).queryByText("Har kunden redan betalat något för tjänsten?")
    fireEvent.click(within(group).getByLabelText(priorPayment ? "Nej" : "Ja"))
  }
}

async function reachFinalReview() {
  answerEligibility()
  fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
  await screen.findByRole("button", { name: "Skicka faktura" })
}

function completeLegacyRecoveryForm() {
  fireEvent.change(screen.getByLabelText("Företagsnamn från den ursprungliga PDF:en"), {
    target: { value: "Historiska Konsulter" },
  })
  fireEvent.change(screen.getByLabelText("Ägarens namn från den ursprungliga PDF:en"), {
    target: { value: "Anna Andersson" },
  })
  fireEvent.change(screen.getByLabelText("Skattestatus vid utfärdandet"), {
    target: { value: "f_skatt" },
  })
  fireEvent.change(screen.getByLabelText("Momsstatus vid utfärdandet"), {
    target: { value: "registered" },
  })
  fireEvent.change(screen.getByLabelText("Regelversion kontrollerad mot samtida handlingar"), {
    target: { value: "2026.1" },
  })
  fireEvent.click(
    screen.getByLabelText(
      "Jag intygar att företagsidentiteten och den synliga skatt-/momsformuleringen har skrivits av från den bevarade ursprungliga faktura-PDF:en och att eventuella statuskillnader och regelversionen har kontrollerats mot samtida handlingar.",
    ),
  )
}

describe("InvoicesPage", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    activeWorkspace = workspace
    invoiceIssueValidationGet.mockResolvedValue(null)
    taxProfileGetCurrent.mockResolvedValue({ taxStatus: "f_skatt" })
    invoiceIssuePreflight.mockResolvedValue({
      eligibility: eligibleAssessment,
      invoiceId: "inv-1",
      issueDate: "2026-01-15",
      currentTurnoverMinor: 0,
      projectedTurnoverMinor: 1_000_000,
      thresholdMinor: 12_000_000,
      requiresVatTreatmentReview: false,
      nextAction: null,
      ruleVersionId: "rv-2026-active",
      taxYear: 2026,
      sourceUrl: "https://example.test/vat",
    })
    invoiceLegacySnapshotRecoveryStatus.mockResolvedValue({
      invoiceId: "inv-1",
      recoveryRequired: false,
      preservedPdfDocumentId: null,
    })
  })

  it("requires explicit answers before preflight and never treats uncertainty as yes", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    expect(screen.getAllByRole("radio").every((radio) => !(radio as HTMLInputElement).checked)).toBe(true)
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
    expect(invoiceIssuePreflight).not.toHaveBeenCalled()
    expect(screen.getByRole("alert")).toBeInTheDocument()
    answerEligibility()
    fireEvent.click(within(screen.getAllByRole("group")[0]).getByLabelText("Osäker"))
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
    expect(invoiceIssuePreflight).not.toHaveBeenCalled()
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it("retains facts and the draft when preflight is blocked", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    invoiceIssuePreflight.mockRejectedValue(new Error("unsupported exact VAT"))
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    answerEligibility()
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
    await screen.findByRole("alert")
    expect(screen.getByLabelText("När slutfördes tjänsten?")).toHaveValue("2026-01-14")
    expect(screen.getByRole("button", { name: "Behåll utkast" })).toBeInTheDocument()
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it("submits precisely the reviewed facts, date and fingerprint", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    invoiceIssue.mockResolvedValue(baseInvoice)
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    await reachFinalReview()
    expect(invoiceIssuePreflight).toHaveBeenCalledWith({ invoiceId: "inv-1", issueDate: "2026-01-15", eligibilityFacts: expectedFacts })
    expect(screen.getByText(/2027-05-12/)).toBeInTheDocument()
    expect(screen.getByText(/reviewed-evidence-digest/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Skicka faktura" }))
    await waitFor(() => expect(invoiceIssue).toHaveBeenCalledWith({
      invoiceId: "inv-1", idempotencyKey: expect.any(String), issueDate: "2026-01-15",
      eligibilityFacts: expectedFacts, reviewFingerprint: eligibleAssessment.reviewFingerprint,
    }))
  })

  it.each([
    ["prior payment", 1, "Ja"],
    ["EU trade", 2, "Nej"],
    ["annual registration", 3, "Osäker"],
    ["taxable base", 4, "Nej"],
    ["late invoice", 5, "Nej"],
  ])("blocks contradictory or uncertain %s facts without final issuance", async (_reason, index, answer) => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    answerEligibility()
    fireEvent.click(within(screen.getAllByRole("group")[index as number]).getByLabelText(answer as string))
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
    expect(screen.getByRole("alert")).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Skicka faktura" })).not.toBeInTheDocument()
    expect(invoiceIssuePreflight).not.toHaveBeenCalled()
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it.each([
    ["2027-01-15", "2026-01-14"],
    ["2026-01-15", "2025-12-31"],
    ["2026-01-15", "2026-01-16"],
  ])("blocks uncovered or contradictory dates (%s / %s)", async (issueDate, completed) => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    answerEligibility()
    fireEvent.change(screen.getByLabelText("Fakturadatum (dagen du skickar fakturan)"), { target: { value: issueDate } })
    fireEvent.change(screen.getByLabelText("När slutfördes tjänsten?"), { target: { value: completed } })
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
    expect(screen.getByRole("alert")).toBeInTheDocument()
    expect(invoiceIssuePreflight).not.toHaveBeenCalled()
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it("discards the final review when the displayed invoice is refreshed with changed data", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    await reachFinalReview()
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null, totalIncVatMinor: 99 }])
    // Simulate a list refresh while review is open; it must not retain authority over an older snapshot.
    fireEvent.change(screen.getByLabelText("Filter"), { target: { value: "draft" } })
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it("starts with unanswered facts when switching to a different draft", async () => {
    invoiceList.mockResolvedValue([
      { ...baseInvoice, status: "draft", voucherId: null },
      { ...baseInvoice, id: "inv-2", status: "draft", voucherId: null },
    ])
    renderInvoices()
    const buttons = await screen.findAllByRole("button", { name: "Skicka" })
    fireEvent.click(buttons[0])
    await reachFinalReview()
    fireEvent.click(screen.getByRole("button", { name: "Behåll utkast" }))
    fireEvent.click(buttons[1])
    expect(screen.getAllByRole("radio").every((radio) => !(radio as HTMLInputElement).checked)).toBe(true)
    expect(screen.getByLabelText("När slutfördes tjänsten?")).toHaveValue("")
    expect(invoiceIssuePreflight).toHaveBeenCalledTimes(1)
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it("invalidates review on date/fact edits and requires a new fingerprint", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    const view = renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    await reachFinalReview()
    fireEvent.click(screen.getByRole("button", { name: "Ändra svar" }))
    expect(within(screen.getAllByRole("group")[0]).getByLabelText("Ja")).toBeChecked()
    fireEvent.change(screen.getByLabelText("Fakturadatum (dagen du skickar fakturan)"), { target: { value: "2026-01-16" } })
    fireEvent.change(screen.getByLabelText("När slutfördes tjänsten?"), { target: { value: "2026-01-15" } })
    expect(screen.queryByRole("button", { name: "Skicka faktura" })).not.toBeInTheDocument()
    expect(invoiceIssue).not.toHaveBeenCalled()
    activeWorkspace = { ...workspace, id: "ws-2" }
    view.rerender(invoicesTree())
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    expect(screen.getAllByRole("radio").every((radio) => !(radio as HTMLInputElement).checked)).toBe(true)
    expect(screen.getByLabelText("När slutfördes tjänsten?")).toHaveValue("")
  })

  it("ignores stale preflight results after workspace changes", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    let resolve!: (result: unknown) => void
    invoiceIssuePreflight.mockImplementationOnce(() => new Promise((done) => { resolve = done }))
    const view = renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    answerEligibility()
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
    activeWorkspace = { ...workspace, id: "ws-2" }
    view.rerender(invoicesTree())
    await act(async () => resolve({ invoiceId: "inv-1", issueDate: "2026-01-15", eligibility: eligibleAssessment }))
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it("retains the draft after freshness rejection and requires another preflight", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    invoiceIssue.mockRejectedValueOnce(new Error("freshness rejection"))
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    await reachFinalReview()
    fireEvent.click(screen.getByRole("button", { name: "Skicka faktura" }))
    await screen.findByRole("alert")
    expect(screen.getByLabelText("När slutfördes tjänsten?")).toHaveValue("2026-01-14")
    expect(screen.queryByRole("button", { name: "Skicka faktura" })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
    await screen.findByRole("button", { name: "Skicka faktura" })
    expect(invoiceIssuePreflight).toHaveBeenCalledTimes(2)
    expect(invoiceIssue).toHaveBeenCalledTimes(1)
  })

  it("blocks a missing eligibility assessment rather than trusting an old successful preflight", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    invoiceIssuePreflight.mockResolvedValueOnce({ invoiceId: "inv-1", issueDate: "2026-01-15", eligibility: null, requiresVatTreatmentReview: false })
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Skicka" }))
    answerEligibility()
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))
    await screen.findByRole("alert")
    expect(screen.queryByRole("button", { name: "Skicka faktura" })).not.toBeInTheDocument()
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it("presents the explicit annual questions and sending responsibility in English", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    renderInvoices("en")
    fireEvent.click(await screen.findByRole("button", { name: "Issue" }))
    expect(screen.getByText("Does the business have no EU trade throughout the 2026 accounting year?")).toBeInTheDocument()
    expect(screen.getByText("Is the business VAT registered for annual VAT reporting in 2026?")).toBeInTheDocument()
    expect(screen.getByText("Is the expected taxable base for 2026 at most SEK 1 million excluding VAT?")).toBeInTheDocument()
    expect(screen.getByText(/The app sends no email/)).toBeInTheDocument()
    expect(screen.getAllByRole("radio").every((radio) => !(radio as HTMLInputElement).checked)).toBe(true)
  })

  it("reads historical evidence without issuing, recovering or guessing facts", async () => {
    invoiceList.mockResolvedValue([baseInvoice])
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Visa redovisningsunderlag" }))
    await screen.findByText("Ingen historisk valideringspost finns. Nuvarande uppgifter används inte för att fylla luckan.")
    expect(invoiceIssueValidationGet).toHaveBeenCalledWith({ invoiceId: "inv-1" })
    expect(invoiceIssuePreflight).not.toHaveBeenCalled()
    expect(invoiceIssue).not.toHaveBeenCalled()
    expect(invoiceLegacySnapshotRecover).not.toHaveBeenCalled()
  })

  it("shows immutable historical facts and provenance without re-preflighting", async () => {
    invoiceList.mockResolvedValue([baseInvoice])
    invoiceIssueValidationGet.mockResolvedValueOnce({ invoiceId: "inv-1", facts: expectedFacts, assessment: eligibleAssessment, recordedAt: "2026-01-15T12:00:00Z" })
    renderInvoices()
    fireEvent.click(await screen.findByRole("button", { name: "Visa redovisningsunderlag" }))
    await screen.findByText(/Registrerat: 2026-01-15T12:00:00Z/)
    expect(screen.getByText(/reviewed-evidence-digest/)).toBeInTheDocument()
    expect(screen.getByText(/2027-05-12/)).toBeInTheDocument()
    expect(screen.getByText("2026-01-14")).toBeInTheDocument()
    expect(screen.queryByRole("radio")).not.toBeInTheDocument()
    expect(invoiceIssuePreflight).not.toHaveBeenCalled()
    expect(invoiceIssue).not.toHaveBeenCalled()
    expect(invoiceCredit).not.toHaveBeenCalled()
    expect(invoicePdfRefresh).not.toHaveBeenCalled()
  })

  it("hides credit and mark-paid actions for reconciled invoices", async () => {
    invoiceList.mockResolvedValue([
      { ...baseInvoice, paymentVoucherId: "pay-v-1" },
    ])

    renderInvoices()

    await waitFor(() => {
      expect(screen.getByText("Betald")).toBeInTheDocument()
    })
    expect(screen.queryByRole("button", { name: "Kreditera" })).not.toBeInTheDocument()
    expect(screen.queryByRole("link", { name: "Registrera betalning" })).not.toBeInTheDocument()
  })

  it("shows credit for open issued invoices", async () => {
    invoiceList.mockResolvedValue([baseInvoice])

    renderInvoices()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Kreditera" })).toBeInTheDocument()
    })
  })

  it("surfaces PDF-not-ready status when preview is requested before generation", async () => {
    invoiceList.mockResolvedValue([baseInvoice])
    invoicePdfRefresh.mockResolvedValue("queued")

    renderInvoices()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Förhandsgranska PDF" })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole("button", { name: "Förhandsgranska PDF" }))

    await waitFor(() => {
      expect(
        screen.getByText("PDF genereras fortfarande — försök igen om en stund."),
      ).toBeInTheDocument()
    })
    expect(documentReveal).not.toHaveBeenCalled()
  })

  it("shows FA-skatt invoice note when tax profile is FA-skatt", async () => {
    taxProfileGetCurrent.mockResolvedValue({
      id: "tp-1",
      taxStatus: "fa_skatt",
      expectedBusinessProfitMinor: 0,
      expectedSalaryIncomeMinor: 480_000_00,
      activeRuleYear: 2026,
    })
    invoiceList.mockResolvedValue([baseInvoice])

    renderInvoices()

    await waitFor(() => {
      expect(
        screen.getByText(/Med FA-skatt gäller A-skatt på lön/i),
      ).toBeInTheDocument()
    })
  })

  it("guides planning tax profiles to complete setup instead of opening issue review", async () => {
    taxProfileGetCurrent.mockResolvedValue({
      id: "tp-1",
      taxStatus: "planning",
      expectedBusinessProfitMinor: 0,
      expectedSalaryIncomeMinor: 0,
      activeRuleYear: 2026,
    })
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])

    renderInvoices()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Skicka" })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole("button", { name: "Skicka" }))

    await waitFor(() => {
      expect(
        screen.getByText(
          "Slutför din skatteprofil med godkänd F-skatt eller FA-skatt innan du skickar en kundfaktura.",
        ),
      ).toBeInTheDocument()
    })
    expect(invoiceIssuePreflight).not.toHaveBeenCalled()
    expect(invoiceIssue).not.toHaveBeenCalled()
  })

  it("reviews invoice issue before invoking the mutation", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    invoiceIssue.mockResolvedValue({ ...baseInvoice, status: "issued" })

    renderInvoices()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Skicka" })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole("button", { name: "Skicka" }))

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Behåll utkast" })).toBeInTheDocument()
    })
    expect(invoiceIssue).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "Behåll utkast" }))
    expect(invoiceIssue).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole("button", { name: "Skicka" }))
    await reachFinalReview()
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Skicka faktura" })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole("button", { name: "Skicka faktura" }))

    await waitFor(() => {
      expect(invoiceIssue).toHaveBeenCalledTimes(1)
    })
  })

  it("defaults and forwards the selected credit accounting date", async () => {
    invoiceList.mockResolvedValue([baseInvoice])
    invoiceCredit.mockResolvedValue({
      ...baseInvoice,
      id: "credit-1",
      invoiceKind: "credit_note",
      issueDate: "2027-01-10",
    })
    renderInvoices()

    const dateInput = await screen.findByLabelText("Bokföringsdatum för kreditfaktura")
    expect(dateInput).toHaveValue(localTodayIsoDate())
    fireEvent.change(dateInput, { target: { value: "2027-01-10" } })
    fireEvent.click(screen.getByRole("button", { name: "Kreditera" }))
    fireEvent.click(screen.getByRole("button", { name: "Skapa kreditfaktura" }))

    await waitFor(() => {
      expect(invoiceCredit).toHaveBeenCalledWith(
        expect.objectContaining({ issueDate: "2027-01-10" }),
      )
    })
  })

  it("shows the rule-backed VAT action and blocks confirmation before issue", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, status: "draft", voucherId: null }])
    invoiceIssuePreflight.mockResolvedValue({
      eligibility: eligibleAssessment,
      invoiceId: "inv-1",
      issueDate: "2026-01-15",
      currentTurnoverMinor: 12_000_000,
      projectedTurnoverMinor: 12_000_001,
      thresholdMinor: 12_000_000,
      requiresVatTreatmentReview: true,
      nextAction: "Review VAT registration and VAT treatment before issuing this invoice",
      ruleVersionId: "rv-2026-active",
      taxYear: 2026,
      sourceUrl: "https://example.test/vat-threshold",
    })

    renderInvoices()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Skicka" })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole("button", { name: "Skicka" }))
    answerEligibility()
    fireEvent.click(screen.getByRole("button", { name: "Kontrollera och granska" }))

    await waitFor(() => {
      const reviewDialog = screen.getByRole("dialog")
      expect(
        within(reviewDialog).getAllByText(
          "Granska momsregistrering och momshantering innan fakturan skickas.",
        ),
      ).toHaveLength(2)
    })
    expect(
      within(screen.getByRole("dialog")).getByText("Regel 2026: https://example.test/vat-threshold"),
    ).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Stäng granskning" }))
    expect(invoiceIssue).not.toHaveBeenCalled()
  })
  it("shows legacy snapshot recovery and opens the preserved verified PDF without regeneration", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, pdfDocumentId: "current-pdf" }])
    invoiceLegacySnapshotRecoveryStatus.mockResolvedValue({
      invoiceId: "inv-1",
      recoveryRequired: true,
      preservedPdfDocumentId: "retained-pdf",
    })

    renderInvoices()

    await waitFor(() => {
      expect(screen.getByText("Historiskt fakturaunderlag behöver återskapas")).toBeInTheDocument()
    })
    expect(
      screen.getByText(/Den bevarade verifierade PDF:en kan öppnas utan att fakturan återskapas eller krediteras/i),
    ).toBeInTheDocument()

    fireEvent.click(screen.getByRole("button", { name: "Visa bevarad verifierad PDF" }))

    await waitFor(() => {
      expect(documentReveal).toHaveBeenCalledWith({ documentId: "retained-pdf" })
    })
    expect(invoicePdfRefresh).not.toHaveBeenCalled()
  })

  it("does not invoke recovery when the review is cancelled", async () => {
    invoiceList.mockResolvedValue([baseInvoice])
    invoiceLegacySnapshotRecoveryStatus.mockResolvedValue({
      invoiceId: "inv-1",
      recoveryRequired: true,
      preservedPdfDocumentId: "retained-pdf",
    })

    renderInvoices()

    fireEvent.click(await screen.findByRole("button", { name: "Återskapa historiskt intyg" }))
    await screen.findByRole("dialog")
    fireEvent.click(screen.getByRole("button", { name: "Avbryt" }))

    expect(invoiceLegacySnapshotRecover).not.toHaveBeenCalled()
  })

  it("recovers a legacy snapshot only after the user reviews and attests historical values", async () => {
    invoiceList.mockResolvedValue([baseInvoice])
    invoiceLegacySnapshotRecoveryStatus.mockResolvedValue({
      invoiceId: "inv-1",
      recoveryRequired: true,
      preservedPdfDocumentId: "retained-pdf",
    })
    invoiceLegacySnapshotRecover.mockResolvedValue(true)
    const user = userEvent.setup()


    renderInvoices()

    fireEvent.click(await screen.findByRole("button", { name: "Återskapa historiskt intyg" }))
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "Visa bevarad verifierad PDF",
      }),
    )
    await waitFor(() => {
      expect(documentReveal).toHaveBeenCalledWith({ documentId: "retained-pdf" })
    })
    const businessName = screen.getByLabelText("Företagsnamn från den ursprungliga PDF:en")
    await user.type(businessName, "Historiska Konsulter")
    expect(businessName).toHaveFocus()
    expect(businessName).toHaveValue("Historiska Konsulter")

    completeLegacyRecoveryForm()

    fireEvent.click(screen.getByRole("button", { name: "Spara historiskt intyg" }))

    await waitFor(() => {
      expect(invoiceLegacySnapshotRecover).toHaveBeenCalledWith({
        invoiceId: "inv-1",
        documentId: "retained-pdf",
        businessName: "Historiska Konsulter",
        ownerName: "Anna Andersson",
        taxStatus: "f_skatt",
        vatStatus: "registered",
        ruleVersionId: "2026.1",
        attestation:
          "I attest that the business identity and displayed tax/VAT wording were transcribed from the retained original invoice PDF, and that any status distinctions and the rule version were checked against contemporaneous records.",
      })
    })
    expect(invoiceList).toHaveBeenCalledTimes(2)
  })

  it("keeps the preserved PDF pointer available when recovery fails", async () => {
    invoiceList.mockResolvedValue([baseInvoice])
    invoiceLegacySnapshotRecoveryStatus.mockResolvedValue({
      invoiceId: "inv-1",
      recoveryRequired: true,
      preservedPdfDocumentId: "retained-pdf",
    })
    invoiceLegacySnapshotRecover.mockRejectedValue(new Error("recovery failed"))

    renderInvoices()

    fireEvent.click(await screen.findByRole("button", { name: "Återskapa historiskt intyg" }))
    completeLegacyRecoveryForm()
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "Visa bevarad verifierad PDF",
      }),
    )
    await waitFor(() => {
      expect(documentReveal).toHaveBeenCalledWith({ documentId: "retained-pdf" })
    })
    fireEvent.click(screen.getByRole("button", { name: "Spara historiskt intyg" }))

    await waitFor(() => {
      expect(screen.getByText("Kunde inte återskapa historiskt intyg.")).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole("button", { name: "Avbryt" }))
    fireEvent.click(screen.getByRole("button", { name: "Visa bevarad verifierad PDF" }))
    await waitFor(() => {
      expect(documentReveal).toHaveBeenCalledWith({ documentId: "retained-pdf" })
    })
    expect(invoicePdfRefresh).not.toHaveBeenCalled()
  })
})
