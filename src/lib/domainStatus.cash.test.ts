import { describe, expect, it } from "vitest"
import { yearEndReadinessDestination, yearEndReadinessLabel } from "./domainStatus"
import { t } from "../i18n"

describe("cash closing readiness", () => {
  it.each(["sv", "en"] as const)("explains the closed preparation boundary in %s", (locale) => {
    expect(yearEndReadinessLabel(locale, "cash_preparation_unvalidated"))
      .toBe(t(locale, "cash.limits"))
    expect(yearEndReadinessDestination("cash_preparation_unvalidated")).toBeNull()
  })
})
