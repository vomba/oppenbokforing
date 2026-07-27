import type { Locale } from "../i18n"
import { t } from "../i18n"

export function vatReturnStatusLabel(locale: Locale, status: string): string {
  switch (status) {
    case "draft":
      return t(locale, "vat.status.draft")
    case "approved":
      return t(locale, "vat.status.approved")
    default:
      return status
  }
}

export function voucherStatusLabel(locale: Locale, status: string): string {
  switch (status) {
    case "posted":
      return t(locale, "ledger.voucherStatus.posted")
    case "draft":
      return t(locale, "ledger.voucherStatus.draft")
    default:
      return status
  }
}

export function periodStatusLabel(locale: Locale, status: string): string {
  switch (status) {
    case "open":
      return t(locale, "ledger.periodStatus.open")
    case "locked":
      return t(locale, "ledger.periodStatus.locked")
    default:
      return status
  }
}

export function yearEndPackageStatusLabel(locale: Locale, status: string): string {
  switch (status) {
    case "draft":
      return t(locale, "yearEnd.packageStatus.draft")
    case "approved":
      return t(locale, "yearEnd.packageStatus.approved")
    default:
      return status
  }
}
