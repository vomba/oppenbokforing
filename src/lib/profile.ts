export function parseMinorUnits(value: string): number | null {
  const trimmed = value.trim()
  if (!/^-?\d+$/.test(trimmed)) {
    return null
  }
  return Number(trimmed)
}
