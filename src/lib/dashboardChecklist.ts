import type { MessageKey } from "../i18n"

export type DashboardChecklistTone = "neutral" | "amber" | "red"

export type DashboardChecklistItem = {
  id: string
  labelKey: MessageKey
  detailKey?: MessageKey
  href: string
  tone: DashboardChecklistTone
  priority: number
}

export type DashboardChecklistInput = {
  compliancePassed: boolean | null
  vatWarning: string | null | undefined
  stagedCount: number
  openInvoices: number
  yearEndReady: boolean | null
  unsatisfiedYearEndCodes: string[]
}

export function buildDashboardChecklist(input: DashboardChecklistInput): DashboardChecklistItem[] {
  const items: DashboardChecklistItem[] = []

  if (input.compliancePassed === false) {
    items.push({
      id: "compliance",
      labelKey: "dashboard.checklist.compliance",
      href: "/onboarding",
      tone: "red",
      priority: 10,
    })
  }

  if (input.vatWarning === "breached") {
    items.push({
      id: "vat-breached",
      labelKey: "dashboard.checklist.vatBreached",
      href: "/vat",
      tone: "red",
      priority: 20,
    })
  } else if (input.vatWarning === "approaching") {
    items.push({
      id: "vat-approaching",
      labelKey: "dashboard.checklist.vatApproaching",
      href: "/vat",
      tone: "amber",
      priority: 30,
    })
  }

  if (input.stagedCount > 0) {
    items.push({
      id: "staged",
      labelKey: "dashboard.checklist.stagedImports",
      detailKey: "dashboard.checklist.stagedImportsDetail",
      href: "/documents",
      tone: "amber",
      priority: 40,
    })
  }

  if (input.openInvoices > 0) {
    items.push({
      id: "open-invoices",
      labelKey: "dashboard.checklist.openInvoices",
      detailKey: "dashboard.checklist.openInvoicesDetail",
      href: "/invoices",
      tone: "neutral",
      priority: 50,
    })
  }

  if (input.yearEndReady === false) {
    items.push({
      id: "year-end",
      labelKey: "dashboard.checklist.yearEnd",
      detailKey:
        input.unsatisfiedYearEndCodes.length > 0
          ? "dashboard.checklist.yearEndDetail"
          : undefined,
      href: "/year-end",
      tone: "neutral",
      priority: 60,
    })
  }

  if (items.length === 0) {
    items.push({
      id: "caught-up",
      labelKey: "dashboard.checklist.caughtUp",
      href: "/invoices",
      tone: "neutral",
      priority: 100,
    })
  }

  return items.sort((left, right) => left.priority - right.priority)
}
