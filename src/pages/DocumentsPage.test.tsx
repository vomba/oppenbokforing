import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { MemoryRouter, Route, Routes } from "react-router-dom"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import type { Document, InvoiceSummary, StagedTransactionSummary } from "../lib/commands"
import { classifyIssuedTreatment } from "../lib/issuedTreatment"
import { cashHistory, invoiceIssueProof } from "../test/cashFixtures"
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
  invoiceCashHistoryGet,
  invoiceIssueValidationGet,
  vatProfileGetCurrent,
  reconciliationMatchCreate,
} = vi.hoisted(() => ({
  stagedTransactionsList: vi.fn(),
  invoiceList: vi.fn(),
  documentList: vi.fn(),
  accountList: vi.fn(),
  invoicePaymentRecord: vi.fn(),
  documentImport: vi.fn(),
  csvImportCreate: vi.fn(),
  invoiceCashHistoryGet: vi.fn().mockResolvedValue(null),
  invoiceIssueValidationGet: vi.fn(),
  vatProfileGetCurrent: vi.fn(),
  reconciliationMatchCreate: vi.fn(),
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
  invoiceCashHistoryGet,
  invoiceIssueValidationGet,
  vatProfileGetCurrent,
  reconciliationMatchCreate,
  accountList,
  csvImportCreate,
  documentImport,
  documentList,
  expensePost: vi.fn(),
  invoiceList,
  invoicePaymentRecord,
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
  vatProfileGetCurrent.mockResolvedValue({ accountingMethod: "invoice_method" })
  invoiceCashHistoryGet.mockResolvedValue(null)
  invoiceIssueValidationGet.mockResolvedValue(invoiceIssueProof)
  invoiceList.mockResolvedValue([baseInvoice])
  documentList.mockResolvedValue([] as Document[])
  accountList.mockResolvedValue([])
}

describe("DocumentsPage", () => {
  beforeEach(() => {
    vi.resetAllMocks()
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

  it.each(["validated", "legacy"])("shows completion state after successful %s invoice payment", async (kind) => {
    if (kind === "legacy") invoiceIssueValidationGet.mockResolvedValue(null)
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
      expect(within(screen.getByRole("note")).getByText(/statement\.pdf/)).toBeInTheDocument()
    })

    const paymentDateInput = screen.getByLabelText("Betalningsdatum")
    expect(paymentDateInput).toHaveValue("")
    expect(screen.getByRole("button", { name: "Markera faktura betald" })).toBeDisabled()

    fireEvent.change(paymentDateInput, { target: { value: "2026-03-02" } })

    expect(screen.getByRole("button", { name: "Markera faktura betald" })).toBeEnabled()

    fireEvent.click(screen.getByRole("button", { name: "Markera faktura betald" }))

    expect(invoicePaymentRecord).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "Avbryt" }))
    expect(invoicePaymentRecord).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole("button", { name: "Markera faktura betald" }))
    fireEvent.click(screen.getByRole("button", { name: "Bokför betalning" }))

    await waitFor(() => {
      expect(screen.getByRole("link", { name: "Tillbaka till fakturor" })).toBeInTheDocument()
    })
    expect(screen.queryByRole("button", { name: "Markera faktura betald" })).not.toBeInTheDocument()
    expect(invoicePaymentRecord).toHaveBeenCalledTimes(1)
    expect(invoicePaymentRecord).toHaveBeenCalledWith(
      expect.objectContaining({ paymentDate: "2026-03-02" }),
    )
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


describe("cash issued-source payment routing", () => {
  beforeEach(() => { vi.resetAllMocks(); invoiceCashHistoryGet.mockResolvedValue(null); defaultInboxMocks([stagedRow]) })
  it("uses immutable cash proof for the PDF receipt form and disables CSV payment matching", async () => {
    invoiceList.mockResolvedValue([{ ...baseInvoice, voucherId: null }])
    invoiceCashHistoryGet.mockResolvedValue({ ...cashHistory, invoiceId: baseInvoice.id, issue: { ...cashHistory.issue, invoiceId: baseInvoice.id } })
    renderDocuments("/documents?invoiceId=inv-1")
    const recordCashReceipt = await screen.findByRole("button", { name: "Registrera full kontantmetodsbetalning" })
    expect(screen.queryAllByText(/Obestyrkt eller oläsbar metod/)).toHaveLength(0)
    expect(screen.getAllByText("CSV-matchning för kontantmetoden är ännu inte aktiverad. Använd det separata flödet för en stödd full PDF-betalning.").length).toBeGreaterThan(0)
    fireEvent.click(recordCashReceipt)
    expect(await screen.findByRole("dialog")).toHaveTextContent("Registrera faktiskt mottagen full betalning")
    expect(screen.queryByRole("button", { name: "Markera faktura betald" })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Avbryt" }))
    fireEvent.click(screen.getByRole("button", { name: stagedRow.transactionDate }))
    expect(screen.getByRole("button", { name: "Matcha som fakturabetalning" })).toBeDisabled()
    expect(invoicePaymentRecord).not.toHaveBeenCalled()
    expect(reconciliationMatchCreate).not.toHaveBeenCalled()
  })
  it("never falls back to old invoice payment on cash-history read failure", async () => {
    invoiceCashHistoryGet.mockRejectedValue(new Error("unreadable"))
    renderDocuments("/documents?invoiceId=inv-1")
    await screen.findAllByText(/Obestyrkt eller oläsbar metod/)
    expect(screen.queryByRole("button", { name: "Välj bankutdrag (PDF)" })).not.toBeInTheDocument()
    expect(invoicePaymentRecord).not.toHaveBeenCalled()
  })
  it.each(["cash_method", null, "unknown"])("keeps legacy identity blocked with current method %s", async (method) => {
    vatProfileGetCurrent.mockResolvedValue(method === null ? null : { accountingMethod: method })
    invoiceIssueValidationGet.mockResolvedValue(null)
    invoiceList.mockResolvedValue([{ ...baseInvoice, voucherId: null }])
    renderDocuments("/documents?invoiceId=inv-1")
    await screen.findAllByText(/Obestyrkt eller oläsbar metod/)
    expect(screen.queryByRole("button", { name: "Registrera full kontantmetodsbetalning" })).not.toBeInTheDocument()
    expect(invoicePaymentRecord).not.toHaveBeenCalled()
  })
})

it("does not mistake a legacy voucher for proof of invoice-method recognition", async () => {
  expect(classifyIssuedTreatment(baseInvoice.id, null, null)).toBe("legacy")
  expect(classifyIssuedTreatment(baseInvoice.id, null, null)).not.toBe("invoice")
  vi.resetAllMocks()
  defaultInboxMocks([stagedRow])
  invoiceIssueValidationGet.mockResolvedValue(null)
  invoiceCashHistoryGet.mockResolvedValue(null)
  renderDocuments("/documents?invoiceId=inv-1")
  await screen.findAllByText(/Inget aktuellt valideringsunderlag/)
  fireEvent.click(await screen.findByRole("button", { name: stagedRow.transactionDate }))
  expect(screen.getByRole("button", { name: "Matcha som fakturabetalning" })).toBeEnabled()
  expect(invoicePaymentRecord).not.toHaveBeenCalled()
})

it("preserves the existing CSV settlement call for legacy history and explicit invoice profile", async () => {
  vi.resetAllMocks()
  defaultInboxMocks([stagedRow])
  invoiceIssueValidationGet.mockResolvedValue(null)
  reconciliationMatchCreate.mockResolvedValue({ matchId: "m", voucherId: "v" })
  renderDocuments("/documents?invoiceId=inv-1")
  await screen.findAllByText(/Inget aktuellt valideringsunderlag/)
  fireEvent.click(screen.getByRole("button", { name: stagedRow.transactionDate }))
  fireEvent.click(screen.getByRole("button", { name: "Matcha som fakturabetalning" }))
  expect(reconciliationMatchCreate).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole("button", { name: "Skapa matchning" }))
  await waitFor(() => expect(reconciliationMatchCreate).toHaveBeenCalledWith(expect.objectContaining({ stagedTransactionId: stagedRow.id, matchKind: "invoice_payment", invoiceId: baseInvoice.id })))
})
it.each(["profile", "invoice history"])("does not use legacy fallback on %s read failure", async (read) => {
  vi.resetAllMocks()
  defaultInboxMocks([stagedRow])
  invoiceIssueValidationGet.mockResolvedValue(null)
  if (read === "profile") vatProfileGetCurrent.mockRejectedValue(new Error("unreadable profile"))
  else invoiceIssueValidationGet.mockRejectedValue(new Error("unreadable history"))
  renderDocuments("/documents?invoiceId=inv-1")
  await screen.findAllByText(/Obestyrkt eller oläsbar metod/)
  fireEvent.click(screen.getByRole("button", { name: stagedRow.transactionDate }))
  expect(screen.getByRole("button", { name: "Matcha som fakturabetalning" })).toBeDisabled()
  expect(screen.queryByRole("button", { name: "Välj bankutdrag (PDF)" })).not.toBeInTheDocument()
  expect(invoicePaymentRecord).not.toHaveBeenCalled()
  expect(reconciliationMatchCreate).not.toHaveBeenCalled()
})
