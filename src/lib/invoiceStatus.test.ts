import { describe, expect, it } from "vitest"
import { invoiceDisplayStatus, localTodayIsoDate } from "./invoiceStatus"
import type { InvoiceSummary } from "./commands"

function invoice(overrides: Partial<InvoiceSummary>): InvoiceSummary {
  return {
    id: "inv-1",
    counterpartyId: "cp-1",
    counterpartyName: "Customer",
    status: "issued",
    invoiceKind: "standard",
    invoiceNumber: "2026-0001",
    sourceInvoiceId: null,
    issueDate: "2026-01-01",
    dueDate: "2099-12-31",
    totalExVatMinor: 10_000,
    totalVatMinor: 2_500,
    totalIncVatMinor: 12_500,
    pdfJobId: null,
    pdfDocumentId: null,
    voucherId: "v-issue",
    paymentVoucherId: null,
    lines: [],
    ...overrides,
  }
}

describe("invoiceDisplayStatus", () => {
  it("marks paid when a payment voucher exists", () => {
    expect(
      invoiceDisplayStatus(invoice({ paymentVoucherId: "v-pay" })),
    ).toBe("paid")
  })

  it("marks overdue for unpaid issued invoices past due date", () => {
    expect(
      invoiceDisplayStatus(invoice({ dueDate: "2020-01-01" })),
    ).toBe("overdue")
  })

  it("labels credit notes separately from credited originals", () => {
    expect(
      invoiceDisplayStatus(
        invoice({ invoiceKind: "credit_note", status: "issued" }),
      ),
    ).toBe("credit_note")
    expect(invoiceDisplayStatus(invoice({ status: "credited" }))).toBe(
      "credited",
    )
  })

  it("uses local calendar date for overdue checks", () => {
    const noonUtc = new Date("2026-07-10T12:00:00Z")
    expect(localTodayIsoDate(noonUtc)).toBe(
      `${noonUtc.getFullYear()}-${String(noonUtc.getMonth() + 1).padStart(2, "0")}-${String(noonUtc.getDate()).padStart(2, "0")}`,
    )
  })
})
