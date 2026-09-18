import type { MessageKey } from "../i18n"
import type { TaxTask } from "./bindings"

export type TaxTaskPresentation = Readonly<{
  route: "/vat" | "/year-end" | "/onboarding"
  search: string
  actionKey: MessageKey
  statusKey: MessageKey
}>

const TARGET_PRESENTATION: Readonly<Record<string, Pick<TaxTaskPresentation, "route" | "actionKey">>> = {
  vat: { route: "/vat", actionKey: "taxTasks.action.vatReturn" },
  year_end: { route: "/year-end", actionKey: "taxTasks.action.yearEnd" },
  onboarding: { route: "/onboarding", actionKey: "taxTasks.action.profileReview" },
}

const STATUS_KEYS: Readonly<Record<string, MessageKey>> = {
  overdue: "taxTasks.status.overdue",
  action_required: "taxTasks.status.actionRequired",
  upcoming: "taxTasks.status.upcoming",
  prepared_external_submission_required: "taxTasks.status.preparedExternalSubmissionRequired",
  date_unavailable: "taxTasks.status.dateUnavailable",
}

export function presentTaxTask(task: TaxTask): TaxTaskPresentation {
  const target = TARGET_PRESENTATION[task.target]
  const statusKey = STATUS_KEYS[task.status]
  if (!target || !statusKey) {
    throw new Error(`Unsupported tax task presentation: ${task.target}/${task.status}`)
  }
  if (task.target === "vat") {
    if (!/^\d{4}(?:-(?:M(?:0[1-9]|1[0-2])|Q[1-4]))?$/.test(task.periodKey)) {
      throw new Error(`Invalid VAT task period: ${task.periodKey}`)
    }
    return { ...target, search: `?periodKey=${encodeURIComponent(task.periodKey)}`, statusKey }
  }
  if (task.target === "year_end") {
    if (!/^\d{4}$/.test(task.periodKey)) {
      throw new Error(`Invalid year-end task period: ${task.periodKey}`)
    }
    return { ...target, search: `?fiscalYear=${encodeURIComponent(task.periodKey)}`, statusKey }
  }
  if (task.status !== "date_unavailable") {
    throw new Error(`Unsupported onboarding task status: ${task.status}`)
  }
  return { ...target, search: "?step=vat", statusKey }
}
