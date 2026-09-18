import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { LocaleProvider, localeFromSettings, useLocale } from "./LocaleContext"

function LocaleProbe() {
  const { locale } = useLocale()
  return <output>{locale}</output>
}

describe("LocaleContext", () => {
  it("defaults first-run language and invalid saved settings to Swedish", () => {
    render(
      <LocaleProvider>
        <LocaleProbe />
      </LocaleProvider>,
    )

    expect(screen.getByText("sv")).toBeInTheDocument()
    expect(localeFromSettings(undefined)).toBe("sv")
    expect(localeFromSettings("invalid")).toBe("sv")
  })

  it("keeps an explicit English saved setting", () => {
    expect(localeFromSettings("en")).toBe("en")
  })
})
