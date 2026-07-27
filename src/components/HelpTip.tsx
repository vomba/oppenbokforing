import { useId, type ReactNode } from "react"
import { useLocale } from "../context/LocaleContext"
import { tVars } from "../i18n"

export function HelpTip({ label, children }: { label: string; children: ReactNode }) {
  const contentId = useId()
  const { locale } = useLocale()

  return (
    <span className="help-tip">
      <button
        type="button"
        className="help-tip-trigger"
        aria-label={tVars(locale, "help.ariaLabel", { label })}
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
