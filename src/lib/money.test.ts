import { describe, expect, it } from "vitest"
import { formatSekMinor, parseSekToMinorUnits } from "./money"

describe("formatSekMinor", () => {
  it("formats integer minor units as SEK", () => {
    expect(formatSekMinor(123456)).toMatch(/1.*234,56/)
  })
})

describe("parseSekToMinorUnits", () => {
  it("parses whole kronor", () => {
    expect(parseSekToMinorUnits("120000")).toBe(12000000)
    expect(parseSekToMinorUnits("0")).toBe(0)
  })

  it("parses Swedish decimal comma", () => {
    expect(parseSekToMinorUnits("48 000,50")).toBe(4800050)
    expect(parseSekToMinorUnits("1200,5")).toBe(120050)
  })

  it("parses dot decimals when no comma", () => {
    expect(parseSekToMinorUnits("1200.50")).toBe(120050)
  })

  it("parses thousands with dot and decimal comma", () => {
    expect(parseSekToMinorUnits("1.234,56")).toBe(123456)
  })

  it("rejects invalid input", () => {
    expect(parseSekToMinorUnits("abc")).toBeNull()
    expect(parseSekToMinorUnits("12,345,67")).toBeNull()
    expect(parseSekToMinorUnits("")).toBeNull()
  })
})
