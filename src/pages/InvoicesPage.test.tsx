import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { MemoryRouter } from "react-router-dom"
import { describe, expect, it, beforeEach, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { SimpleModeProvider } from "../context/SimpleModeContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import type { InvoiceSummary } from "../lib/commands"
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

const { invoiceList, invoicePdfRefresh, invoicePdfStatus, documentReveal, taxProfileGetCurrent } =
  vi.hoisted(() => ({
  invoiceList: vi.fn(),
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
  invoiceCredit: vi.fn(),
  invoiceIssue: vi.fn(),
  invoiceList,
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

describe("InvoicesPage", () => {
  beforeEach(() => {
    taxProfileGetCurrent.mockResolvedValue({ taxStatus: "f_skatt" })
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
})
