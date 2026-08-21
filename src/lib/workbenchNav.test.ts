import { describe, expect, it } from "vitest"
import {
  navItemsForMode,
  simpleWorkbenchNavItems,
  workbenchNavItems,
} from "./workbenchNav"

describe("navItemsForMode", () => {
  it("uses the exact novice task order in simple mode", () => {
    expect(navItemsForMode(true)).toBe(simpleWorkbenchNavItems)
    expect(navItemsForMode(true).map((item) => item.key)).toEqual([
      "dashboard",
      "invoices",
      "documents",
      "vat",
      "yearEnd",
      "settings",
    ])
    expect(navItemsForMode(true).map((item) => item.labelKey)).toEqual([
      "nav.simple.dashboard",
      "nav.simple.invoices",
      "nav.simple.documents",
      "nav.simple.vat",
      "nav.simple.yearEnd",
      "nav.simple.settings",
    ])
  })

  it("shows full workbench navigation when simple mode is off", () => {
    expect(navItemsForMode(false)).toBe(workbenchNavItems)
    expect(navItemsForMode(false).map((item) => item.key)).toEqual([
      "dashboard",
      "invoices",
      "ledger",
      "documents",
      "vat",
      "yearEnd",
      "settings",
    ])
  })
})
