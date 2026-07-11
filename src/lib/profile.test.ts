import { describe, expect, it } from "vitest"
import { complianceScenarioForProfile, parseMinorUnits } from "./profile"

describe("complianceScenarioForProfile", () => {
  it("selects VAT exempt scenario for low-turnover profiles", () => {
    expect(
      complianceScenarioForProfile({ vatStatus: "exempt_low_turnover", taxStatus: "f_skatt" }),
    ).toBe("vat-exempt-below-threshold")
  })

  it("selects FA-skatt scenario for registered VAT profiles", () => {
    expect(
      complianceScenarioForProfile({ vatStatus: "registered", taxStatus: "fa_skatt" }),
    ).toBe("fa-skatt-salary-and-business")
  })
})

describe("parseMinorUnits", () => {
  it("accepts whole numbers", () => {
    expect(parseMinorUnits("120000")).toBe(120000)
  })

  it("rejects non-numeric input", () => {
    expect(parseMinorUnits("abc")).toBeNull()
    expect(parseMinorUnits("12.5")).toBeNull()
  })
})
