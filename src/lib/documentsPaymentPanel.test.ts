import { describe, expect, it } from "vitest"
import { resolveInvoicePaymentPanelState } from "./documentsPaymentPanel"
import type { InvoiceSummary } from "./commands"

const issuedInvoice: InvoiceSummary = {
  id: "inv-1",
  counterpartyId: "c-1",
  counterpartyName: "Customer",
  status: "issued",
  invoiceKind: "standard",
  invoiceNumber: "2026-001",
  sourceInvoiceId: null,
  issueDate: "2026-01-15",
  dueDate: "2026-02-15",
  totalExVatMinor: 100_000,
  totalVatMinor: 25_000,
  totalIncVatMinor: 125_000,
  pdfJobId: null,
  pdfDocumentId: null,
  voucherId: "v-1",
  paymentVoucherId: null,
  lines: [],
}

describe("resolveInvoicePaymentPanelState", () => {
  it("returns loading until inbox is loaded", () => {
    expect(
      resolveInvoicePaymentPanelState({
        invoiceIdFromUrl: "inv-1",
        inboxLoaded: false,
        paymentRecorded: false,
        invoice: issuedInvoice,
      }),
    ).toBe("loading")
  })

  it("returns not_found when invoice id is absent from issued list", () => {
    expect(
      resolveInvoicePaymentPanelState({
        invoiceIdFromUrl: "missing",
        inboxLoaded: true,
        paymentRecorded: false,
        invoice: null,
      }),
    ).toBe("not_found")
  })

  it("returns already_paid when payment voucher exists", () => {
    expect(
      resolveInvoicePaymentPanelState({
        invoiceIdFromUrl: "inv-1",
        inboxLoaded: true,
        paymentRecorded: false,
        invoice: { ...issuedInvoice, paymentVoucherId: "pay-1" },
      }),
    ).toBe("already_paid")
  })

  it("returns ready for open issued invoice", () => {
    expect(
      resolveInvoicePaymentPanelState({
        invoiceIdFromUrl: "inv-1",
        inboxLoaded: true,
        paymentRecorded: false,
        invoice: issuedInvoice,
      }),
    ).toBe("ready")
  })

  it("returns completed after successful payment", () => {
    expect(
      resolveInvoicePaymentPanelState({
        invoiceIdFromUrl: "inv-1",
        inboxLoaded: true,
        paymentRecorded: true,
        invoice: issuedInvoice,
      }),
    ).toBe("completed")
  })
})
