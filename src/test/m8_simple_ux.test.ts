import Ajv2020 from "ajv/dist/2020.js"
import addFormats from "ajv-formats"
import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import {
  complianceFailureMessages,
  isCompliancePassing,
} from "../lib/compliancePresentation"
import { buildDashboardChecklist } from "../lib/dashboardChecklist"
import { parseSekToMinorUnits } from "../lib/money"
import { ONBOARDING_STEPS } from "../lib/onboardingWizard"
import { dashboardTourSteps } from "../lib/dashboardTour"
import { navItemsForMode } from "../lib/workbenchNav"

const uiDir = join(process.cwd(), "fixtures/ui-scenarios")
const goldenDir = join(process.cwd(), "fixtures/golden-scenarios")
const schema = JSON.parse(
  readFileSync(join(uiDir, "schema.json"), "utf8"),
) as object

const ajv = new Ajv2020({ allErrors: true, strict: false })
addFormats(ajv)
const validate = ajv.compile(schema)

function loadGoldenExpected(id: string): Record<string, unknown> {
  const fixture = JSON.parse(readFileSync(join(goldenDir, `${id}.json`), "utf8")) as {
    expected: Record<string, unknown>
  }
  return fixture.expected
}

describe("M8 ui-scenario fixtures", () => {
  const ids = readdirSync(uiDir)
    .filter((name) => name.endsWith(".json") && name !== "schema.json")
    .map((name) => name.replace(/\.json$/, ""))

  it("lists the guided UX scenario", () => {
    expect(ids).toContain("guided-ux-onboarding-checklist")
  })

  it.each(ids)("%s validates against UI scenario schema", (id) => {
    const fixture = JSON.parse(readFileSync(join(uiDir, `${id}.json`), "utf8")) as {
      id: string
      milestone: number
      specRef: string
    }

    const valid = validate(fixture)
    if (!valid) {
      const details = validate.errors?.map((e) => `${e.instancePath} ${e.message}`).join("; ")
      throw new Error(`Schema validation failed for ${id}: ${details}`)
    }

    expect(fixture.id).toBe(id)
    expect(fixture.milestone).toBe(8)
    expect(fixture.specRef.length).toBeGreaterThan(0)
  })
})

describe("M8 guided UX integration (fixture-driven)", () => {
  const fixture = JSON.parse(
    readFileSync(join(uiDir, "guided-ux-onboarding-checklist.json"), "utf8"),
  ) as {
    expected: {
      onboardingSteps: string[]
      defaultLocale: string
      sekParsing: { input: string; minorUnits: number }
      checklistOrderWhenBlocked: string[]
      complianceGoldenScenarios: string[]
      dashboardTourSteps: string[]
      simpleModeDefault: boolean
      simpleModeHiddenNav: string[]
    }
  }

  it("M8.1 — onboarding wizard exposes four ordered steps", () => {
    expect([...ONBOARDING_STEPS]).toEqual(fixture.expected.onboardingSteps)
  })

  it("M8.1-SEK — SEK decimal input converts to minor units", () => {
    const { input, minorUnits } = fixture.expected.sekParsing
    expect(parseSekToMinorUnits(input)).toBe(minorUnits)
  })

  it("M8.1-ERR — compliance golden scenarios pass with human-readable failures only", () => {
    for (const scenarioId of fixture.expected.complianceGoldenScenarios) {
      const expected = loadGoldenExpected(scenarioId)
      expect(isCompliancePassing(scenarioId, expected)).toBe(true)
      expect(complianceFailureMessages(scenarioId, expected)).toEqual([])
    }
  })

  it("M8.2-CHK — dashboard checklist orders blocked work before routine items", () => {
    const items = buildDashboardChecklist({
      compliancePassed: false,
      vatWarning: "breached",
      stagedCount: 2,
      openInvoices: 1,
      yearEndReady: false,
      unsatisfiedYearEndCodes: ["open_invoices"],
    })

    expect(items.map((item) => item.id)).toEqual(fixture.expected.checklistOrderWhenBlocked)
  })

  it("documents Swedish-first default locale expectation", () => {
    expect(fixture.expected.defaultLocale).toBe("sv")
  })

  it("M8.3 — dashboard tour step order matches ui-scenario", () => {
    expect(fixture.expected.dashboardTourSteps).toEqual(dashboardTourSteps.map((step) => step.id))
  })

  it("M8.5 — simple mode hides infrastructure navigation by default", () => {
    expect(fixture.expected.simpleModeDefault).toBe(true)
    const keys = navItemsForMode(fixture.expected.simpleModeDefault).map((item) => item.key)
    for (const hidden of fixture.expected.simpleModeHiddenNav) {
      expect(keys).not.toContain(hidden)
    }
  })
})
