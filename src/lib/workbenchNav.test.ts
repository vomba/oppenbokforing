import { describe, expect, it } from "vitest"
import { navItemsForMode, workbenchNavItems } from "./workbenchNav"

describe("navItemsForMode", () => {
  it("hides ledger navigation in simple mode", () => {
    const keys = navItemsForMode(true).map((item) => item.key)
    expect(keys).not.toContain("ledger")
    expect(keys).toContain("invoices")
    expect(keys).toContain("settings")
  })

  it("shows full navigation when simple mode is off", () => {
    expect(navItemsForMode(false).map((item) => item.key)).toEqual(
      workbenchNavItems.map((item) => item.key),
    )
  })
})
