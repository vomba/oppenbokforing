import { describe, expect, it } from "vitest"
import { en } from "./en"
import { sv } from "./sv"
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

  it("provides every eligibility label and interpolation variable in Swedish and English", () => {
    const keys = Object.keys(en).filter((key) => key.startsWith("eligibility.")) as Array<keyof typeof en>
    expect(keys.length).toBeGreaterThan(40)
    for (const key of keys) {
      expect(sv[key]).toBeTruthy()
      expect(en[key]).toBeTruthy()
      expect(sv[key]).not.toBe(en[key])
      expect(sv[key].match(/\{[^}]+\}/g) ?? []).toEqual(en[key].match(/\{[^}]+\}/g) ?? [])
    }
  })

  it("resolves every novice task and review label in both supported locales", () => {
    const keys = [
      "nav.simple.dashboard",
      "nav.simple.invoices",
      "nav.simple.documents",
      "nav.simple.vat",
      "nav.simple.yearEnd",
      "nav.simple.settings",
      "actionReview.cancel",
      "actionReview.issue.title",
      "actionReview.issue.confirm",
      "actionReview.credit.title",
      "actionReview.credit.confirm",
      "invoices.creditIssueDate",
      "invoices.creditAccountingDate",
      "invoices.thresholdReviewAction",
      "invoices.thresholdProjectedTurnover",
      "invoices.thresholdRule",
      "invoices.closeReview",
      "actionReview.payment.title",
      "actionReview.payment.confirm",
      "actionReview.match.title",
      "actionReview.match.confirm",
      "actionReview.expense.title",
      "actionReview.expense.confirm",
      "actionReview.vat.title",
      "actionReview.vat.confirm",
      "actionReview.yearEnd.title",
      "actionReview.yearEnd.confirm",
      "tour.taxTasks.title",
      "tour.taxTasks.body",
    ] as const

    for (const locale of ["sv", "en"] as const) {
      for (const key of keys) {
        expect(t(locale, key)).not.toBe(key)
      }
    }
  })
})
