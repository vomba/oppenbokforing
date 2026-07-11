import type { Locale } from "../i18n"
import { t } from "../i18n"
import type { InvoiceSummary } from "./commands"

export type InvoiceDisplayStatus =
  | "draft"
  | "issued"
  | "paid"
  | "overdue"
  | "credited"
  | "credit_note"

export function invoiceDisplayStatus(invoice: InvoiceSummary): InvoiceDisplayStatus {
  if (invoice.invoiceKind === "credit_note") {
    return "credit_note"
  }
  if (invoice.status === "credited") {
    return "credited"
  }
  if (invoice.paymentVoucherId) {
    return "paid"
  }
  if (
    invoice.status === "issued" &&
    invoice.dueDate &&
    invoice.dueDate < todayIsoDate()
  ) {
    return "overdue"
  }
  if (invoice.status === "draft") {
    return "draft"
  }
  if (invoice.status === "issued") {
    return "issued"
  }
  return "issued"
}

export function invoiceStatusLabel(locale: Locale, status: InvoiceDisplayStatus): string {
  return t(locale, `invoices.status.${status}`)
}

export function localTodayIsoDate(date: Date = new Date()): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${year}-${month}-${day}`
}

function todayIsoDate(): string {
  return localTodayIsoDate()
}
