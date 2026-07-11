import { useEffect, useRef, useState } from "react"
import { t, tVars, type Locale, type MessageKey } from "../i18n"

export type GuidedTourStep = {
  id: string
  titleKey: MessageKey
  bodyKey: MessageKey
}

type GuidedTourProps = {
  locale: Locale
  steps: GuidedTourStep[]
  active: boolean
  onComplete: () => void
  onSkip: () => void
}

export function GuidedTour({ locale, steps, active, onComplete, onSkip }: GuidedTourProps) {
  const [index, setIndex] = useState(0)
  const dialogRef = useRef<HTMLDivElement>(null)
  const previousFocusRef = useRef<HTMLElement | null>(null)

  useEffect(() => {
    if (active) {
      setIndex(0)
    }
  }, [active])

  useEffect(() => {
    if (!active) return
    const step = steps[index]
    if (!step) return
    const target = document.querySelector(`[data-tour="${step.id}"]`)
    target?.scrollIntoView?.({ block: "nearest", behavior: "smooth" })
  }, [active, index, steps])

  useEffect(() => {
    if (!active) return

    previousFocusRef.current = document.activeElement as HTMLElement | null
    const dialog = dialogRef.current
    dialog?.focus()

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault()
        onSkip()
        return
      }
      if (event.key !== "Tab" || !dialog) return

      const focusable = dialog.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return

      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }

    document.addEventListener("keydown", handleKeyDown)
    return () => {
      document.removeEventListener("keydown", handleKeyDown)
      previousFocusRef.current?.focus()
    }
  }, [active, onSkip])

  if (!active || steps.length === 0) {
    return null
  }

  const step = steps[index]
  if (!step) {
    return null
  }

  const isLast = index >= steps.length - 1

  function handleNext() {
    if (isLast) {
      onComplete()
      return
    }
    setIndex((current) => current + 1)
  }

  return (
    <div className="guided-tour-overlay" role="presentation">
      <div
        ref={dialogRef}
        className="guided-tour-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="guided-tour-title"
        tabIndex={-1}
      >
        <p className="eyebrow">
          {tVars(locale, "tour.progress", { current: index + 1, total: steps.length })}
        </p>
        <h3 id="guided-tour-title">{t(locale, step.titleKey)}</h3>
        <p>{t(locale, step.bodyKey)}</p>
        <div className="guided-tour-actions">
          <button type="button" className="secondary" onClick={onSkip}>
            {t(locale, "tour.skip")}
          </button>
          <button type="button" onClick={handleNext}>
            {isLast ? t(locale, "tour.done") : t(locale, "tour.next")}
          </button>
        </div>
      </div>
    </div>
  )
}
