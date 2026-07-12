import type { BusinessProfile, TaxProfile, VatProfile } from "./bindings"
import { minorUnitsToSekInput } from "./money"

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

export function defaultOnboardingDraft(workspaceName = ""): OnboardingDraft {
  return {
    businessName: workspaceName.trim(),
    ownerName: "",
    sniCode: "",
    taxStatus: "f_skatt",
    salarySek: "0",
    businessProfitSek: "0",
    vatStatus: "exempt_low_turnover",
    reportingPeriod: "quarterly",
    accountingMethod: "invoice_method",
  }
}

function asTaxStatus(value: string): OnboardingDraft["taxStatus"] {
  return value === "fa_skatt" ? "fa_skatt" : "f_skatt"
}

function asVatStatus(value: string): OnboardingDraft["vatStatus"] {
  return value === "registered" ? "registered" : "exempt_low_turnover"
}

function asReportingPeriod(value: string): OnboardingDraft["reportingPeriod"] {
  if (value === "monthly" || value === "yearly") {
    return value
  }
  return "quarterly"
}

function asAccountingMethod(value: string): OnboardingDraft["accountingMethod"] {
  return value === "cash_method" ? "cash_method" : "invoice_method"
}

export function onboardingDraftFromProfiles(input: {
  business: BusinessProfile | null
  tax: TaxProfile | null
  vat: VatProfile | null
  workspaceName: string
}): { draft: OnboardingDraft; hasSavedProfiles: boolean } {
  const hasSavedProfiles = Boolean(input.business || input.tax || input.vat)
  const draft = defaultOnboardingDraft(input.business?.businessName || input.workspaceName)

  if (input.business) {
    draft.businessName = input.business.businessName
    draft.ownerName = input.business.ownerName
    draft.sniCode = input.business.sniCode ?? ""
  }

  if (input.tax) {
    draft.taxStatus = asTaxStatus(input.tax.taxStatus)
    draft.salarySek = minorUnitsToSekInput(input.tax.expectedSalaryIncomeMinor)
    draft.businessProfitSek = minorUnitsToSekInput(input.tax.expectedBusinessProfitMinor)
  }

  if (input.vat) {
    draft.vatStatus = asVatStatus(input.vat.vatStatus)
    draft.reportingPeriod = asReportingPeriod(input.vat.reportingPeriod)
    draft.accountingMethod = asAccountingMethod(input.vat.accountingMethod)
  }

  return { draft, hasSavedProfiles }
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
