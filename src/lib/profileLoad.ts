import type { AppError } from "./bindings"

export function isProfileNotFoundError(error: unknown, field: string): boolean {
  if (!error || typeof error !== "object") {
    return false
  }
  const appError = error as AppError
  return (
    appError.code === "validation_error" &&
    Boolean(appError.details?.some((detail) => detail.field === field))
  )
}

export async function loadOptionalProfile<T>(
  loader: () => Promise<T>,
  field: string,
): Promise<{ profile: T | null; failed: boolean }> {
  try {
    return { profile: await loader(), failed: false }
  } catch (error) {
    if (isProfileNotFoundError(error, field)) {
      return { profile: null, failed: false }
    }
    return { profile: null, failed: true }
  }
}
