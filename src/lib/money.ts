export function formatSekMinor(amountMinor: number) {
  const amount = amountMinor / 100
  return new Intl.NumberFormat("sv-SE", {
    style: "currency",
    currency: "SEK",
  }).format(amount)
}

/** Parse a user-entered SEK amount to integer minor units (öre). */
export function parseSekToMinorUnits(value: string): number | null {
  let normalized = value.trim().replace(/[\s\u00a0]/g, "")
  if (!normalized) {
    return null
  }

  const hasComma = normalized.includes(",")
  const hasDot = normalized.includes(".")

  if (hasComma && hasDot) {
    normalized = normalized.replace(/\./g, "").replace(",", ".")
  } else if (hasComma) {
    normalized = normalized.replace(",", ".")
  }

  const match = normalized.match(/^(-?)(\d+)(?:\.(\d{1,2}))?$/)
  if (!match) {
    return null
  }

  const sign = match[1] === "-" ? -1 : 1
  const whole = Number(match[2])
  const fraction = (match[3] ?? "").padEnd(2, "0").slice(0, 2)
  const minor = sign * (whole * 100 + Number(fraction))
  if (!Number.isSafeInteger(minor)) {
    return null
  }
  return minor
}
