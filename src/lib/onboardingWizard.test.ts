import { describe, expect, it } from "vitest"
import { canAdvanceOnboardingStep, canVisitOnboardingStep, nextOnboardingStep } from "./onboardingWizard"

const draft = {
  businessName: "Test AB",
  ownerName: "Anna",
  sniCode: "",
  taxStatus: "f_skatt" as const,
  salarySek: "0",
  businessProfitSek: "0",
  vatStatus: "exempt_low_turnover" as const,
  reportingPeriod: "quarterly" as const,
  accountingMethod: "invoice_method" as const,
}

describe("onboardingWizard", () => {
  it("requires business names before tax step", () => {
    expect(
      canAdvanceOnboardingStep("business", { ...draft, businessName: "" }, { salary: true, profit: true }),
    ).toBe(false)
    expect(canAdvanceOnboardingStep("business", draft, { salary: true, profit: true })).toBe(true)
  })

  it("requires valid SEK before VAT step", () => {
    expect(canAdvanceOnboardingStep("tax", draft, { salary: false, profit: true })).toBe(false)
    expect(canAdvanceOnboardingStep("tax", draft, { salary: true, profit: true })).toBe(true)
  })

  it("advances through steps", () => {
    expect(nextOnboardingStep("business")).toBe("tax")
    expect(nextOnboardingStep("vat")).toBe("review")
    expect(nextOnboardingStep("review")).toBeNull()
  })

  it("blocks visiting later steps until earlier steps are valid", () => {
    expect(
      canVisitOnboardingStep(
        "tax",
        { ...draft, businessName: "", ownerName: "Anna" },
        { salary: true, profit: true },
      ),
    ).toBe(false)
    expect(canVisitOnboardingStep("review", draft, { salary: true, profit: true })).toBe(true)
    expect(canVisitOnboardingStep("review", draft, { salary: false, profit: true })).toBe(false)
  })
})
