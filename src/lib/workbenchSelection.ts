/** Keep a list selection when still valid; otherwise pick the first row or clear. */
export function reconcileListSelection<T extends { id: string }>(
  current: string,
  rows: T[],
): string {
  if (current && rows.some((row) => row.id === current)) {
    return current
  }
  return rows[0]?.id ?? ""
}
