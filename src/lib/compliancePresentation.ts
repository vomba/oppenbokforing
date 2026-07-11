import type { MessageKey } from "../i18n"

type Outcomes = Record<string, unknown>

export function isCompliancePassing(scenarioId: string, outcomes: Outcomes): boolean {
  switch (scenarioId) {
    case "fa-skatt-salary-and-business":
      return (
        outcomes.salaryIncomeInBusinessLedger === false &&
        outcomes.requiresFaSkattGuidance === true &&
        outcomes.invoiceMustMentionFSkatt === true &&
        outcomes.taxPlanningUsesSalaryAssumption === true
      )
    case "vat-exempt-below-threshold":
      return (
        outcomes.mustChargeVat === false &&
        outcomes.mustRegisterForVat === false &&
        outcomes.invoiceMustStateVatExemption === true
      )
    case "vat-exempt-threshold-breach":
      return (
        outcomes.mustRegisterForVat === true &&
        outcomes.mustChargeVatFromBreachSale === true
      )
    default:
      return false
  }
}

export function profileComplianceFailureMessages(
  result: { scenarioIds: string[]; outcomes: Record<string, unknown> },
): MessageKey[] {
  const messages: MessageKey[] = []
  const outcomesByScenario = result.outcomes as Record<string, Record<string, unknown>>
  for (const scenarioId of result.scenarioIds) {
    messages.push(
      ...complianceFailureMessages(scenarioId, outcomesByScenario[scenarioId] ?? {}),
    )
  }
  return [...new Set(messages)]
}

export function complianceFailureMessages(scenarioId: string, outcomes: Outcomes): MessageKey[] {
  if (outcomes.error === "scenario_not_implemented") {
    return ["onboarding.compliance.unsupportedScenario"]
  }

  if (isCompliancePassing(scenarioId, outcomes)) {
    return []
  }

  const messages: MessageKey[] = []

  switch (scenarioId) {
    case "fa-skatt-salary-and-business":
      if (outcomes.salaryIncomeInBusinessLedger !== false) {
        messages.push("onboarding.compliance.salaryInLedger")
      }
      if (outcomes.requiresFaSkattGuidance !== true) {
        messages.push("onboarding.compliance.faSkattGuidance")
      }
      if (outcomes.invoiceMustMentionFSkatt !== true) {
        messages.push("onboarding.compliance.fSkattMention")
      }
      if (outcomes.taxPlanningUsesSalaryAssumption !== true) {
        messages.push("onboarding.compliance.salaryAssumption")
      }
      break
    case "vat-exempt-below-threshold":
      if (outcomes.mustChargeVat !== false) {
        messages.push("onboarding.compliance.mustNotChargeVat")
      }
      if (outcomes.mustRegisterForVat !== false) {
        messages.push("onboarding.compliance.mustNotRegisterVat")
      }
      if (outcomes.invoiceMustStateVatExemption !== true) {
        messages.push("onboarding.compliance.vatExemptionText")
      }
      break
    case "vat-exempt-threshold-breach":
      if (outcomes.mustRegisterForVat !== true) {
        messages.push("onboarding.compliance.registerForVat")
      }
      if (outcomes.mustChargeVatFromBreachSale !== true) {
        messages.push("onboarding.compliance.chargeVatFromBreach")
      }
      break
    default:
      messages.push("onboarding.compliance.reviewRequired")
  }

  if (messages.length === 0) {
    messages.push("onboarding.compliance.reviewRequired")
  }

  return messages
}
