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

export function vatProfileStatusLabel(locale: Locale, status: string): string {
  switch (status) {
    case "exempt_low_turnover":
      return t(locale, "vat.profile.exempt")
    case "registered":
      return t(locale, "vat.profile.registered")
    case "voluntary_registered":
      return t(locale, "vat.profile.voluntaryRegistered")
    default:
      return status
  }
}

export function vatReportingPeriodLabel(locale: Locale, period: string): string {
  switch (period) {
    case "monthly":
      return t(locale, "vat.reporting.monthly")
    case "quarterly":
      return t(locale, "vat.reporting.quarterly")
    case "yearly":
      return t(locale, "vat.reporting.yearly")
    default:
      return period
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

export function yearEndReadinessLabel(locale: Locale, code: string): string {
  switch (code) {
    case "vat_periods_filed":
      return t(locale, "yearEnd.readiness.vatPeriodsFiled")
    case "profile_complete":
      return t(locale, "yearEnd.readiness.profileComplete")
    default:
      return t(locale, "yearEnd.readiness.unknown")
  }
}

export function yearEndReadinessDestination(code: string): "/vat" | "/onboarding" | null {
  switch (code) {
    case "vat_periods_filed":
      return "/vat"
    case "profile_complete":
      return "/onboarding"
    default:
      return null
  }
}
