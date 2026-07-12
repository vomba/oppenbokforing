import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { MemoryRouter, Route, Routes } from "react-router-dom"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import type { Document, InvoiceSummary, StagedTransactionSummary } from "../lib/commands"
import { DocumentsPage } from "./DocumentsPage"

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

const stagedRow: StagedTransactionSummary = {
  id: "st-1",
  csvImportId: null,
  transactionDate: "2026-03-01",
  description: "Inbetalning",
  amountMinor: 1_250_000,
  status: "staged",
}

const {
  stagedTransactionsList,
  invoiceList,
  documentList,
  accountList,
  invoicePaymentRecord,
  documentImport,
  csvImportCreate,
} = vi.hoisted(() => ({
  stagedTransactionsList: vi.fn(),
  invoiceList: vi.fn(),
  documentList: vi.fn(),
  accountList: vi.fn(),
  invoicePaymentRecord: vi.fn(),
  documentImport: vi.fn(),
  csvImportCreate: vi.fn(),
}))

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
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
  accountList,
  csvImportCreate,
  documentImport,
  documentList,
  expensePost: vi.fn(),
  invoiceList,
  invoicePaymentRecord,
  reconciliationMatchCreate: vi.fn(),
  stagedTransactionsList,
}))

function renderDocuments(initialEntry = "/documents") {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <WorkspaceProvider>
        <LocaleProvider initialLocale="sv">
          <Routes>
            <Route path="/documents" element={<DocumentsPage />} />
          </Routes>
        </LocaleProvider>
      </WorkspaceProvider>
    </MemoryRouter>,
  )
}

function defaultInboxMocks(staged: StagedTransactionSummary[] = []) {
  stagedTransactionsList.mockImplementation(({ status }: { status: string }) => {
    if (status === "staged") return Promise.resolve(staged)
    return Promise.resolve([])
  })
  invoiceList.mockResolvedValue([baseInvoice])
  documentList.mockResolvedValue([] as Document[])
  accountList.mockResolvedValue([])
}

describe("DocumentsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    defaultInboxMocks()
  })

  it("shows not payable copy when invoiceId query does not match an issued invoice", async () => {
    invoiceList.mockResolvedValue([])

    renderDocuments("/documents?invoiceId=missing-inv")

    await waitFor(() => {
      expect(
        screen.getByText(/Fakturan hittades inte eller kan inte markeras betald/i),
      ).toBeInTheDocument()
    })
    expect(screen.queryByRole("button", { name: "Markera faktura betald" })).not.toBeInTheDocument()
  })

  it("shows already paid copy when invoice has a payment voucher", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, paymentVoucherId: "pay-v-1" }])

    renderDocuments("/documents?invoiceId=inv-1")

    await waitFor(() => {
      expect(screen.getByText(/Fakturan är redan markerad betald/i)).toBeInTheDocument()
    })
    expect(screen.queryByRole("button", { name: "Markera faktura betald" })).not.toBeInTheDocument()
  })

  it("clears stale staged selection after inbox refresh removes the row", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog")
    vi.mocked(open).mockResolvedValue("/tmp/bank.csv")

    let stagedCalls = 0
    stagedTransactionsList.mockImplementation(({ status }: { status: string }) => {
      if (status !== "staged") return Promise.resolve([])
      stagedCalls += 1
      return Promise.resolve(stagedCalls === 1 ? [stagedRow] : [])
    })
    csvImportCreate.mockResolvedValue({ stagedCount: 0 })

    renderDocuments()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "2026-03-01" })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole("button", { name: "2026-03-01" }))

    await waitFor(() => {
      expect(screen.getByText("Matcha som fakturabetalning")).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole("button", { name: "Välj bank-CSV" }))

    await waitFor(() => {
      expect(csvImportCreate).toHaveBeenCalled()
      expect(screen.queryByText("Matcha som fakturabetalning")).not.toBeInTheDocument()
    })
  })

  it("shows completion state after successful invoice payment", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog")
    vi.mocked(open).mockResolvedValue("/tmp/statement.pdf")
    documentImport.mockResolvedValue({
      id: "doc-1",
      objectPath: "objects/abc",
      contentSha256: "abc",
      mimeType: "application/pdf",
      originalFilename: "statement.pdf",
      retentionYears: 7,
    } satisfies Document)
    invoicePaymentRecord.mockResolvedValue({ matchId: "m-1", voucherId: "v-pay-1" })
    invoiceList
      .mockResolvedValueOnce([baseInvoice])
      .mockResolvedValueOnce([baseInvoice])
      .mockResolvedValueOnce([{ ...baseInvoice, paymentVoucherId: "v-pay-1" }])

    renderDocuments("/documents?invoiceId=inv-1")

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Välj bankutdrag (PDF)" })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole("button", { name: "Välj bankutdrag (PDF)" }))

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Markera faktura betald" })).toBeEnabled()
    })

    fireEvent.click(screen.getByRole("button", { name: "Markera faktura betald" }))

    await waitFor(() => {
      expect(screen.getByRole("link", { name: "Tillbaka till fakturor" })).toBeInTheDocument()
    })
    expect(screen.queryByRole("button", { name: "Markera faktura betald" })).not.toBeInTheDocument()
    await waitFor(() => {
      expect(invoiceList).toHaveBeenCalledTimes(3)
    })
  })

  it("shows not payable after inbox load failure", async () => {
    stagedTransactionsList.mockRejectedValue(new Error("network"))

    renderDocuments("/documents?invoiceId=inv-1")

    await waitFor(() => {
      expect(
        screen.getByText(/Fakturan hittades inte eller kan inte markeras betald/i),
      ).toBeInTheDocument()
    })
    expect(screen.queryByText(/Laddar faktura/i)).not.toBeInTheDocument()
  })
})
