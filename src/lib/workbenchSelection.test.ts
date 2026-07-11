import { describe, expect, it } from "vitest"
import { reconcileListSelection } from "./workbenchSelection"

describe("reconcileListSelection", () => {
  it("keeps the current id when it is still in the list", () => {
    const rows = [{ id: "a" }, { id: "b" }]
    expect(reconcileListSelection("b", rows)).toBe("b")
  })

  it("falls back to the first row when the current id disappeared", () => {
    const rows = [{ id: "a" }, { id: "b" }]
    expect(reconcileListSelection("removed", rows)).toBe("a")
  })

  it("clears selection when the list is empty", () => {
    expect(reconcileListSelection("removed", [])).toBe("")
  })
})
