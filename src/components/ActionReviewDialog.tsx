import { useEffect, useId, useRef, type ReactNode } from "react"
import { createPortal } from "react-dom"

export type ActionReviewDialogProps = {
  open: boolean
  title: string
  summary: string
  consequences: readonly string[]
  correction: string | null
  confirmLabel: string
  cancelLabel: string
  busy: boolean
  onConfirm: () => void
  onCancel: () => void
  children?: ReactNode
}

export function ActionReviewDialog({
  open,
  title,
  summary,
  consequences,
  correction,
  confirmLabel,
  cancelLabel,
  busy,
  onConfirm,
  onCancel,
  children,
}: ActionReviewDialogProps) {
  const titleId = useId()
  const summaryId = useId()
  const cancelRef = useRef<HTMLButtonElement>(null)
  const previousFocusRef = useRef<HTMLElement | null>(null)
  const dialogRef = useRef<HTMLElement>(null)
  const overlayRef = useRef<HTMLDivElement>(null)
  const onCancelRef = useRef(onCancel)

  useEffect(() => {
    onCancelRef.current = onCancel
  }, [onCancel])


  useEffect(() => {
    if (!open) return
    previousFocusRef.current = document.activeElement as HTMLElement | null
    cancelRef.current?.focus()
    const hiddenBackground = Array.from(document.body.children)
      .filter((element) => !element.contains(overlayRef.current))
      .map((element) => ({ element, ariaHidden: element.getAttribute("aria-hidden") }))
    hiddenBackground.forEach(({ element }) => element.setAttribute("aria-hidden", "true"))

    function trapFocus(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault()
        onCancelRef.current()
        return
      }
      if (event.key !== "Tab") return
      const dialog = dialogRef.current
      if (!dialog) return
      const focusable = dialog.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      )
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (!first || !last) return
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }

    document.addEventListener("keydown", trapFocus)
    return () => {
      document.removeEventListener("keydown", trapFocus)
      hiddenBackground.forEach(({ element, ariaHidden }) => {
        if (ariaHidden === null) element.removeAttribute("aria-hidden")
        else element.setAttribute("aria-hidden", ariaHidden)
      })
      previousFocusRef.current?.focus()
    }
  }, [open])

  if (!open) return null

  return createPortal(
    <div ref={overlayRef} className="guided-tour-overlay" role="presentation">
      <section
        className="guided-tour-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        ref={dialogRef}
        aria-describedby={summaryId}
      >
        <h3 id={titleId}>{title}</h3>
        <p id={summaryId}>{summary}</p>
        {children}
        <ul>
          {consequences.map((consequence) => (
            <li key={consequence}>{consequence}</li>
          ))}
        </ul>
        {correction ? <p>{correction}</p> : null}
        <div className="guided-tour-actions">
          <button ref={cancelRef} type="button" className="secondary" disabled={busy} onClick={onCancel}>
            {cancelLabel}
          </button>
          <button type="button" disabled={busy} onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </section>
    </div>,
    document.body,
  )
}
