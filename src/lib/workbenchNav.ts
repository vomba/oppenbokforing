import type { MessageKey } from "../i18n"

export type NavKey =
  | "dashboard"
  | "invoices"
  | "ledger"
  | "documents"
  | "vat"
  | "yearEnd"
  | "settings"

export const workbenchNavItems: { key: NavKey; to: string; labelKey: MessageKey }[] = [
  { key: "dashboard", to: "/dashboard", labelKey: "nav.dashboard" },
  { key: "invoices", to: "/invoices", labelKey: "nav.invoices" },
  { key: "ledger", to: "/ledger", labelKey: "nav.ledger" },
  { key: "documents", to: "/documents", labelKey: "nav.documents" },
  { key: "vat", to: "/vat", labelKey: "nav.vat" },
  { key: "yearEnd", to: "/year-end", labelKey: "nav.yearEnd" },
  { key: "settings", to: "/settings", labelKey: "nav.settings" },
]

const SIMPLE_MODE_HIDDEN: NavKey[] = ["ledger"]

export function navItemsForMode(simpleMode: boolean) {
  if (!simpleMode) {
    return workbenchNavItems
  }
  return workbenchNavItems.filter((item) => !SIMPLE_MODE_HIDDEN.includes(item.key))
}
