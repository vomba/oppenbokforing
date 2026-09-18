import { describe, expect, it } from "vitest"
import { onboardingDraftFromProfiles } from "./onboardingWizard"

describe("onboardingDraftFromProfiles", () => {
  it("prefills business name from workspace label when no profile exists", () => {
    const { draft, hasSavedProfiles } = onboardingDraftFromProfiles({
      business: null,
      tax: null,
      vat: null,
      workspaceName: "Min enskilda firma",
    })

    expect(hasSavedProfiles).toBe(false)
    expect(draft.businessName).toBe("Min enskilda firma")
    expect(draft.ownerName).toBe("")
  })

  it("hydrates all saved profile fields for edit mode", () => {
    const { draft, hasSavedProfiles } = onboardingDraftFromProfiles({
      business: {
        id: "bp-1",
        businessName: "Konsult AB",
        ownerName: "Anna Svensson",
        residencyCountry: "SE",
        sniCode: "62010",
      },
      tax: {
        id: "tp-1",
        taxStatus: "fa_skatt",
        expectedBusinessProfitMinor: 120_000_00,
        expectedSalaryIncomeMinor: 48_000_50,
        activeRuleYear: 2026,
      },
      vat: {
        id: "vp-1",
        vatStatus: "registered",
        reportingPeriod: "monthly",
        accountingMethod: "cash_method",
        voluntaryRegistrationDate: null,
        vatFilingDeadlineRegime: "monthly_12",
      },
      workspaceName: "Min enskilda firma",
    })

    expect(hasSavedProfiles).toBe(true)
    expect(draft.businessName).toBe("Konsult AB")
    expect(draft.ownerName).toBe("Anna Svensson")
    expect(draft.sniCode).toBe("62010")
    expect(draft.taxStatus).toBe("fa_skatt")
    expect(draft.salarySek).toBe("48000,50")
    expect(draft.businessProfitSek).toBe("120000")
    expect(draft.vatStatus).toBe("registered")
    expect(draft.reportingPeriod).toBe("monthly")
    expect(draft.accountingMethod).toBe("cash_method")
  })

  it("preserves planning tax and voluntary VAT registration fields", () => {
    const { draft } = onboardingDraftFromProfiles({
      business: null,
      tax: {
        id: "tp-1",
        taxStatus: "planning",
        expectedBusinessProfitMinor: 120_000_00,
        expectedSalaryIncomeMinor: 48_000_50,
        activeRuleYear: 2026,
      },
      vat: {
        id: "vp-1",
        vatStatus: "voluntary_registered",
        reportingPeriod: "quarterly",
        accountingMethod: "cash_method",
        voluntaryRegistrationDate: "2026-01-01",
        vatFilingDeadlineRegime: "quarterly_12",
      },
      workspaceName: "Min enskilda firma",
    })

    expect(draft.taxStatus).toBe("planning")
    expect(draft.vatStatus).toBe("voluntary_registered")
    expect(draft.voluntaryRegistrationDate).toBe("2026-01-01")
    expect(draft.vatFilingDeadlineRegime).toBe("quarterly_12")
  })
})
