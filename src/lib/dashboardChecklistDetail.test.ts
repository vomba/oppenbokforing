import { describe, expect, it } from "vitest"
import { checklistItemDetail } from "./dashboardChecklistDetail"
import { buildDashboardChecklist } from "./dashboardChecklist"

describe("checklistItemDetail", () => {
  it("formats staged and invoice counts", () => {
    const [staged, invoices] = buildDashboardChecklist({
      compliancePassed: true,
      vatWarning: "none",
      stagedCount: 3,
      openInvoices: 2,
      yearEndReady: true,
      unsatisfiedYearEndCodes: [],
    })

    expect(staged?.id).toBe("staged")
    expect(
      checklistItemDetail("en", staged!, {
        stagedCount: 3,
        openInvoices: 2,
        unsatisfiedYearEndCodes: [],
      }),
    ).toBe("3 unmatched rows in Documents")

    expect(invoices?.id).toBe("open-invoices")
    expect(
      checklistItemDetail("sv", invoices!, {
        stagedCount: 3,
        openInvoices: 2,
        unsatisfiedYearEndCodes: [],
      }),
    ).toBe("2 fakturor väntar på betalning")
  })
})
