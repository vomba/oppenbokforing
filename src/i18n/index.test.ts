import { describe, expect, it } from "vitest"
import { isLocale, t, tVars } from "./index"

describe("i18n", () => {
  it("defaults to English strings", () => {
    expect(t("en", "nav.settings")).toBe("Settings")
  })

  it("renders Swedish navigation labels", () => {
    expect(t("sv", "nav.settings")).toBe("Inställningar")
  })

  it("validates supported locales", () => {
    expect(isLocale("sv")).toBe(true)
    expect(isLocale("de")).toBe(false)
  })

  it("interpolates variables into catalog strings", () => {
    expect(tVars("en", "dashboard.checklist.openInvoicesDetailCount", { count: 4 })).toBe(
      "4 invoices awaiting payment",
    )
  })
})
