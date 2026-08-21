import type { MessageKey } from "../i18n"
import type { TaxTask } from "./bindings"

export type TaxTaskPresentation = Readonly<{
  route: "/vat" | "/year-end" | "/onboarding"
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
  return { ...target, statusKey }
}
