import { describe, expect, it } from "vitest"
import { parseMinorUnits } from "./profile"

describe("parseMinorUnits", () => {
  it("parses integer minor units", () => {
    expect(parseMinorUnits("1250000")).toBe(1250000)
    expect(parseMinorUnits("-100")).toBe(-100)
  })

  it("rejects non-integer input", () => {
    expect(parseMinorUnits("12.50")).toBeNull()
    expect(parseMinorUnits("")).toBeNull()
  })
})
