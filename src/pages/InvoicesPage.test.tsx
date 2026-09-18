import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
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
      workspace,
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
  invoiceList,
  invoiceLegacySnapshotRecoveryStatus,
  invoiceLegacySnapshotRecover,
  invoicePdfRefresh,
  invoicePdfStatus,
  taxProfileGetCurrent,
}))

function renderInvoices() {
  return render(
    <MemoryRouter>
      <WorkspaceProvider>
        <LocaleProvider initialLocale="sv">
          <SimpleModeProvider>
            <InvoicesPage />
          </SimpleModeProvider>
        </LocaleProvider>
      </WorkspaceProvider>
    </MemoryRouter>,
  )
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
    taxProfileGetCurrent.mockResolvedValue({ taxStatus: "f_skatt" })
    invoiceIssuePreflight.mockResolvedValue({
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
      expect(screen.getByRole("button", { name: "Avbryt" })).toBeInTheDocument()
    })
    expect(invoiceIssue).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "Avbryt" }))
    expect(invoiceIssue).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole("button", { name: "Skicka" }))
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
