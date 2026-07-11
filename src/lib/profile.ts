export function complianceScenarioForProfile(input: {
  vatStatus?: string | null
  taxStatus?: string | null
}): string {
  if (input.vatStatus === "exempt_low_turnover") {
    return "vat-exempt-below-threshold"
  }
  if (input.taxStatus === "fa_skatt") {
    return "fa-skatt-salary-and-business"
  }
  return "vat-exempt-below-threshold"
}

export function parseMinorUnits(value: string): number | null {
  const trimmed = value.trim()
  if (!/^-?\d+$/.test(trimmed)) {
    return null
  }
  return Number(trimmed)
}
