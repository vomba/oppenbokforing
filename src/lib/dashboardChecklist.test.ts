import { describe, expect, it } from "vitest"
import { buildDashboardChecklist } from "./dashboardChecklist"

describe("buildDashboardChecklist", () => {
  it("orders urgent items before routine work", () => {
    const items = buildDashboardChecklist({
      compliancePassed: false,
      vatWarning: "breached",
      stagedCount: 2,
      openInvoices: 1,
      yearEndReady: false,
      unsatisfiedYearEndCodes: ["open_invoices"],
    })

    expect(items.map((item) => item.id)).toEqual([
      "compliance",
      "vat-breached",
      "staged",
      "open-invoices",
      "year-end",
    ])
    expect(items[0]?.tone).toBe("red")
    expect(items[1]?.tone).toBe("red")
  })

  it("shows caught-up item when nothing is pending", () => {
    const items = buildDashboardChecklist({
      compliancePassed: true,
      vatWarning: "none",
      stagedCount: 0,
      openInvoices: 0,
      yearEndReady: true,
      unsatisfiedYearEndCodes: [],
    })

    expect(items).toHaveLength(1)
    expect(items[0]?.id).toBe("caught-up")
  })
})
