import type { MessageKey } from "../i18n"
import type { AppError } from "./bindings"

/**
 * Present a command error for UI status lines.
 * Hides storage/internal details; surfaces validation messages (+ field hints).
 */
export function presentAppError(error: unknown, fallback: string): string {
  if (!error || typeof error !== "object") {
    return fallback
  }
  const appError = error as AppError
  if (appError.code === "internal_error" || appError.code === "storage_error") {
    return fallback
  }
  if (appError.code === "validation_error" && appError.details?.length) {
    const fields = appError.details
      .map((detail) => detail.field ?? detail.message)
      .filter(Boolean)
      .join(", ")
    return fields ? `${appError.message} (${fields})` : appError.message
  }
  if (typeof appError.message === "string" && appError.message.length > 0) {
    return appError.message
  }
  return fallback
}

/** MessageKey-aware wrapper for onboarding (caller resolves keys via `t`). */
export function humanAppError(
  error: unknown,
  fallback: MessageKey,
): MessageKey | string {
  if (!error || typeof error !== "object") {
    return fallback
  }
  const appError = error as AppError
  if (appError.code === "internal_error" || appError.code === "storage_error") {
    return fallback
  }
  if (appError.code === "validation_error" && appError.details?.length) {
    const fields = appError.details
      .map((detail) => detail.field ?? detail.message)
      .filter(Boolean)
      .join(", ")
    return fields ? `${appError.message} (${fields})` : appError.message
  }
  if (typeof appError.message === "string" && appError.message.length > 0) {
    return appError.message
  }
  return fallback
}
