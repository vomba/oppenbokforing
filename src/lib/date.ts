import type { Locale } from "../i18n"

export function formatIsoDate(locale: Locale, isoDate: string): string {
  const date = new Date(`${isoDate}T00:00:00Z`)
  if (!/^\d{4}-\d{2}-\d{2}$/.test(isoDate) || Number.isNaN(date.getTime())) {
    return isoDate
  }

  return new Intl.DateTimeFormat(locale === "sv" ? "sv-SE" : "en-GB", {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(date)
}
