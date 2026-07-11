import { useId, type ReactNode } from "react"

export function HelpTip({ label, children }: { label: string; children: ReactNode }) {
  const contentId = useId()

  return (
    <span className="help-tip">
      <button
        type="button"
        className="help-tip-trigger"
        aria-label={`Help: ${label}`}
        aria-describedby={contentId}
      >
        ?
      </button>
      <span id={contentId} className="help-tip-content" role="tooltip">
        {children}
      </span>
    </span>
  )
}
