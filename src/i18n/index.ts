import { en, type MessageKey } from "./en"
import { sv } from "./sv"

export type { MessageKey }

export type Locale = "en" | "sv"

const catalogs: Record<Locale, Record<MessageKey, string>> = { en, sv }

export function t(locale: Locale, key: MessageKey): string {
  return catalogs[locale][key] ?? catalogs.en[key]
}

export function tVars(
  locale: Locale,
  key: MessageKey,
  vars: Record<string, string | number>,
): string {
  let message = t(locale, key)
  for (const [name, value] of Object.entries(vars)) {
    message = message.replaceAll(`{${name}}`, String(value))
  }
  return message
}

export function isLocale(value: string): value is Locale {
  return value === "en" || value === "sv"
}
