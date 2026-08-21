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
      simpleModeNavOrder: string[]
      simpleModeTaskLabels: Record<"sv" | "en", string[]>
      criticalReviewActions: string[]
      calendarTaskOrder: string[]
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

  it("M8.5 — simple mode uses the fixture's task order and retains VAT/year-end routes", () => {
    expect(fixture.expected.simpleModeDefault).toBe(true)
    const items = navItemsForMode(fixture.expected.simpleModeDefault)
    expect(items.map((item) => item.key)).toEqual(fixture.expected.simpleModeNavOrder)
    expect(items.find((item) => item.key === "vat")?.to).toBe("/vat")
    expect(items.find((item) => item.key === "yearEnd")?.to).toBe("/year-end")
    for (const hidden of fixture.expected.simpleModeHiddenNav) {
      expect(items.map((item) => item.key)).not.toContain(hidden)
    }
  })

  it("defines bilingual task labels and critical review boundaries", () => {
    expect(fixture.expected.simpleModeTaskLabels.sv).toHaveLength(6)
    expect(fixture.expected.simpleModeTaskLabels.en).toHaveLength(6)
    expect(fixture.expected.criticalReviewActions).toEqual([
      "invoice_issue",
      "invoice_credit",
      "payment_reconciliation",
      "expense_post",
      "vat_approve",
      "year_end_approve",
    ])
    expect(fixture.expected.calendarTaskOrder).toEqual([
      "overdue",
      "action_required",
      "upcoming",
      "prepared_external_submission_required",
      "date_unavailable",
    ])
  })
})
