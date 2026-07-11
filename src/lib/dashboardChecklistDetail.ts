import type { Locale } from "../i18n"
import { t, tVars } from "../i18n"
import type { DashboardChecklistInput, DashboardChecklistItem } from "./dashboardChecklist"

export function checklistItemDetail(
  locale: Locale,
  item: DashboardChecklistItem,
  input: Pick<DashboardChecklistInput, "stagedCount" | "openInvoices" | "unsatisfiedYearEndCodes">,
): string | null {
  switch (item.id) {
    case "staged":
      return tVars(locale, "dashboard.checklist.stagedImportsDetailCount", {
        count: input.stagedCount,
      })
    case "open-invoices":
      return tVars(locale, "dashboard.checklist.openInvoicesDetailCount", {
        count: input.openInvoices,
      })
    case "year-end":
      return input.unsatisfiedYearEndCodes.length > 0
        ? tVars(locale, "dashboard.checklist.yearEndDetailCount", {
            count: input.unsatisfiedYearEndCodes.length,
          })
        : null
    default:
      return item.detailKey ? t(locale, item.detailKey) : null
  }
}
