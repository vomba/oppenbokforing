import type { ReactNode } from "react"

export function HelpTip({ label, children }: { label: string; children: ReactNode }) {
  return (
    <span className="help-tip">
      <button type="button" className="help-tip-trigger" aria-label={`Help: ${label}`}>
        ?
      </button>
      <span className="help-tip-content" aria-hidden="true">
        {children}
      </span>
    </span>
  )
}
