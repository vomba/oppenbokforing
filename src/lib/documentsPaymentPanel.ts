import type { InvoiceSummary } from "./commands"

export type InvoicePaymentPanelState =
  | "loading"
  | "not_found"
  | "already_paid"
  | "ready"
  | "completed"

export function resolveInvoicePaymentPanelState(input: {
  invoiceIdFromUrl: string | null
  inboxLoaded: boolean
  paymentRecorded: boolean
  invoice: InvoiceSummary | null | undefined
}): InvoicePaymentPanelState {
  if (!input.invoiceIdFromUrl) {
    return "loading"
  }
  if (input.paymentRecorded) {
    return "completed"
  }
  if (!input.inboxLoaded) {
    return "loading"
  }
  if (!input.invoice) {
    return "not_found"
  }
  if (input.invoice.paymentVoucherId) {
    return "already_paid"
  }
  if (input.invoice.status !== "issued") {
    return "not_found"
  }
  return "ready"
}
