import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import {
  complianceFailureMessages,
  isCompliancePassing,
  profileComplianceFailureMessages,
} from "./compliancePresentation"

const fixturesDir = join(process.cwd(), "fixtures/golden-scenarios")

function loadExpected(id: string): Record<string, unknown> {
  const fixture = JSON.parse(readFileSync(join(fixturesDir, `${id}.json`), "utf8")) as {
    expected: Record<string, unknown>
  }
  return fixture.expected
}

describe("isCompliancePassing", () => {
  const complianceScenarios = readdirSync(fixturesDir)
    .filter((name) => name.endsWith(".json") && name !== "schema.json")
    .map((name) => name.replace(/\.json$/, ""))
    .filter((id) =>
      [
        "f-skatt-business",
        "fa-skatt-salary-and-business",
        "vat-exempt-below-threshold",
        "vat-exempt-threshold-breach",
      ].includes(id),
    )

  it.each(complianceScenarios)("golden fixture %s expected outcomes pass", (id) => {
    expect(isCompliancePassing(id, loadExpected(id))).toBe(true)
  })
})

describe("complianceFailureMessages", () => {
  it("maps VAT exempt failures to human message keys", () => {
    const messages = complianceFailureMessages("vat-exempt-below-threshold", {
      mustChargeVat: true,
      mustRegisterForVat: false,
      invoiceMustStateVatExemption: false,
    })
    expect(messages).toContain("onboarding.compliance.mustNotChargeVat")
    expect(messages).toContain("onboarding.compliance.vatExemptionText")
  })

  it("returns no messages for passing golden outcomes", () => {
    expect(
      complianceFailureMessages(
        "fa-skatt-salary-and-business",
        loadExpected("fa-skatt-salary-and-business"),
      ),
    ).toEqual([])
  })

  it("presents F-skatt failures without FA-skatt or salary requirements", () => {
    expect(complianceFailureMessages("f-skatt-business", {})).toEqual([
      "onboarding.compliance.fSkattGuidance",
      "onboarding.compliance.fSkattMention",
    ])
    const passing = loadExpected("f-skatt-business")
    expect(complianceFailureMessages("f-skatt-business", passing)).toEqual([])
    for (const key of Object.keys(passing)) {
      expect(isCompliancePassing("f-skatt-business", { ...passing, [key]: false })).toBe(false)
      expect(complianceFailureMessages("f-skatt-business", { ...passing, [key]: false })).toEqual([
        key === "invoiceMustMentionFSkatt"
          ? "onboarding.compliance.fSkattMention"
          : "onboarding.compliance.fSkattGuidance",
      ])
    }
    expect(profileComplianceFailureMessages({
      scenarioIds: ["f-skatt-business", "vat-exempt-below-threshold"],
      outcomes: {
        "f-skatt-business": passing,
        "vat-exempt-below-threshold": {
          ...loadExpected("vat-exempt-below-threshold"),
          invoiceMustStateVatExemption: false,
        },
      },
    })).toEqual(["onboarding.compliance.vatExemptionText"])
  })

  it("keeps FA-skatt salary failures explicit", () => {
    expect(complianceFailureMessages("fa-skatt-salary-and-business", {
      ...loadExpected("fa-skatt-salary-and-business"),
      taxPlanningUsesSalaryAssumption: false,
    })).toEqual(["onboarding.compliance.salaryAssumption"])
  })

  it("handles unsupported scenarios", () => {
    expect(complianceFailureMessages("unknown", { error: "scenario_not_implemented" })).toEqual([
      "onboarding.compliance.unsupportedScenario",
    ])
  })
})
