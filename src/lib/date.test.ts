import { describe, expect, it } from "vitest"
import { formatIsoDate } from "./date"

describe("formatIsoDate", () => {
  it("formats an ISO date in Swedish and English without changing its calendar day", () => {
    expect(formatIsoDate("sv", "2026-05-12")).toBe("12 maj 2026")
    expect(formatIsoDate("en", "2026-05-12")).toBe("12 May 2026")
  })
})
