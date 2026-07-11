import type { MessageKey } from "../i18n"

export type DashboardTourStep = {
  id: string
  titleKey: MessageKey
  bodyKey: MessageKey
}

export const dashboardTourSteps: DashboardTourStep[] = [
  {
    id: "checklist",
    titleKey: "tour.checklist.title",
    bodyKey: "tour.checklist.body",
  },
  {
    id: "sidebar",
    titleKey: "tour.sidebar.title",
    bodyKey: "tour.sidebar.body",
  },
  {
    id: "spendable-cash",
    titleKey: "tour.cash.title",
    bodyKey: "tour.cash.body",
  },
  {
    id: "backup",
    titleKey: "tour.backup.title",
    bodyKey: "tour.backup.body",
  },
  {
    id: "rules",
    titleKey: "tour.rules.title",
    bodyKey: "tour.rules.body",
  },
]
