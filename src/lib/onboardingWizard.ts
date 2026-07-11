export const ONBOARDING_STEPS = ["business", "tax", "vat", "review"] as const

export type OnboardingStep = (typeof ONBOARDING_STEPS)[number]

export type OnboardingDraft = {
  businessName: string
  ownerName: string
  sniCode: string
  taxStatus: "f_skatt" | "fa_skatt"
  salarySek: string
  businessProfitSek: string
  vatStatus: "exempt_low_turnover" | "registered"
  reportingPeriod: "monthly" | "quarterly" | "yearly"
  accountingMethod: "invoice_method" | "cash_method"
}

export function onboardingStepIndex(step: OnboardingStep): number {
  return ONBOARDING_STEPS.indexOf(step)
}

export function canAdvanceOnboardingStep(
  step: OnboardingStep,
  draft: OnboardingDraft,
  sekValid: { salary: boolean; profit: boolean },
): boolean {
  switch (step) {
    case "business":
      return draft.businessName.trim().length > 0 && draft.ownerName.trim().length > 0
    case "tax":
      return sekValid.salary && sekValid.profit
    case "vat":
      return true
    case "review":
      return false
    default:
      return false
  }
}

export function canVisitOnboardingStep(
  target: OnboardingStep,
  draft: OnboardingDraft,
  sekValid: { salary: boolean; profit: boolean },
): boolean {
  const targetIndex = onboardingStepIndex(target)
  if (targetIndex < 0) {
    return false
  }

  for (let index = 0; index < targetIndex; index += 1) {
    const priorStep = ONBOARDING_STEPS[index]
    if (!priorStep || !canAdvanceOnboardingStep(priorStep, draft, sekValid)) {
      return false
    }
  }

  return true
}

export function nextOnboardingStep(step: OnboardingStep): OnboardingStep | null {
  const index = onboardingStepIndex(step)
  if (index < 0 || index >= ONBOARDING_STEPS.length - 1) {
    return null
  }
  return ONBOARDING_STEPS[index + 1] ?? null
}

export function previousOnboardingStep(step: OnboardingStep): OnboardingStep | null {
  const index = onboardingStepIndex(step)
  if (index <= 0) {
    return null
  }
  return ONBOARDING_STEPS[index - 1] ?? null
}
