import type { MessageKey } from "../i18n"

export type NavKey =
  | "dashboard"
  | "invoices"
  | "ledger"
  | "documents"
  | "vat"
  | "yearEnd"
  | "settings"

export type WorkbenchNavItem = Readonly<{
  key: NavKey
  to: string
  labelKey: MessageKey
}>

export const workbenchNavItems: readonly WorkbenchNavItem[] = [
  { key: "dashboard", to: "/dashboard", labelKey: "nav.dashboard" },
  { key: "invoices", to: "/invoices", labelKey: "nav.invoices" },
  { key: "ledger", to: "/ledger", labelKey: "nav.ledger" },
  { key: "documents", to: "/documents", labelKey: "nav.documents" },
  { key: "vat", to: "/vat", labelKey: "nav.vat" },
  { key: "yearEnd", to: "/year-end", labelKey: "nav.yearEnd" },
  { key: "settings", to: "/settings", labelKey: "nav.settings" },
]

export const simpleWorkbenchNavItems: readonly WorkbenchNavItem[] = [
  { key: "dashboard", to: "/dashboard", labelKey: "nav.simple.dashboard" },
  { key: "invoices", to: "/invoices", labelKey: "nav.simple.invoices" },
  { key: "documents", to: "/documents", labelKey: "nav.simple.documents" },
  { key: "vat", to: "/vat", labelKey: "nav.simple.vat" },
  { key: "yearEnd", to: "/year-end", labelKey: "nav.simple.yearEnd" },
  { key: "settings", to: "/settings", labelKey: "nav.simple.settings" },
]

export function navItemsForMode(simpleMode: boolean): readonly WorkbenchNavItem[] {
  return simpleMode ? simpleWorkbenchNavItems : workbenchNavItems
}
