import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import {
  complianceFailureMessages,
  isCompliancePassing,
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

  it("handles unsupported scenarios", () => {
    expect(complianceFailureMessages("unknown", { error: "scenario_not_implemented" })).toEqual([
      "onboarding.compliance.unsupportedScenario",
    ])
  })
})
