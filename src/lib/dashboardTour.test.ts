import { describe, expect, it } from "vitest"
import { dashboardTourSteps } from "./dashboardTour"
import { helpTopics } from "./helpTopics"

describe("dashboardTourSteps", () => {
  it("retains the five established stops and appends tax tasks", () => {
    expect(dashboardTourSteps.map((step) => step.id)).toEqual([
      "checklist",
      "sidebar",
      "spendable-cash",
      "backup",
      "rules",
      "tax-tasks",
    ])
  })
})

describe("helpTopics", () => {
  it("registers workbench help for core surfaces", () => {
    expect(Object.keys(helpTopics)).toEqual(
      expect.arrayContaining(["ledger", "documents", "yearEnd", "invoices", "vat"]),
    )
  })
})
