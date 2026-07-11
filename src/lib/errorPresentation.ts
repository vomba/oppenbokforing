import type { MessageKey } from "../i18n"
import type { AppError } from "./bindings"

const INTERNAL_ERROR_FALLBACK: MessageKey = "errors.internal"

export function humanAppError(error: unknown, fallback: MessageKey): MessageKey | string {
  if (!error || typeof error !== "object") {
    return fallback
  }
  const appError = error as AppError
  if (appError.code === "internal_error") {
    return INTERNAL_ERROR_FALLBACK
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
