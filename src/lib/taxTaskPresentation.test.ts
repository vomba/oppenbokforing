import { describe, expect, it } from "vitest"
import { presentTaxTask } from "./taxTaskPresentation"

describe("presentTaxTask", () => {
  it("maps backend task data to an existing route and typed labels without changing order", () => {
    expect(
      presentTaxTask({
        id: "vat_return:2026-Q1",
        kind: "vat_return",
        status: "overdue",
        target: "vat",
        periodKey: "2026-Q1",
        dueOn: "2026-05-12",
        ruleVersionId: "rv-2026-active",
        taxYear: 2026,
        sourceUrl: "https://www.skatteverket.se/example",
      }),
    ).toEqual({
      route: "/vat",
      search: "?periodKey=2026-Q1",
      actionKey: "taxTasks.action.vatReturn",
      statusKey: "taxTasks.status.overdue",
    })
  })

  it("routes an unavailable VAT date directly to the VAT onboarding step", () => {
    expect(
      presentTaxTask({
        id: "profile_review:vat_filing_deadline_regime",
        kind: "profile_review",
        status: "date_unavailable",
        target: "onboarding",
        periodKey: "2026",
        dueOn: null,
        ruleVersionId: "rv-2026-active",
        taxYear: 2026,
        sourceUrl: "https://www.skatteverket.se/example",
      }),
    ).toMatchObject({
      route: "/onboarding",
      search: "?step=vat",
    })
  })
})
