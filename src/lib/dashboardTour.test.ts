import { describe, expect, it } from "vitest"
import { dashboardTourSteps } from "./dashboardTour"
import { helpTopics } from "./helpTopics"

describe("dashboardTourSteps", () => {
  it("defines five first-run tour stops", () => {
    expect(dashboardTourSteps.map((step) => step.id)).toEqual([
      "checklist",
      "sidebar",
      "spendable-cash",
      "backup",
      "rules",
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
