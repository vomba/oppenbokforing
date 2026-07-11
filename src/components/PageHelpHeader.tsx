import type { ReactNode } from "react"
import { HelpTip } from "./HelpTip"
import { useLocale } from "../context/LocaleContext"
import { t, type MessageKey } from "../i18n"

export function PageHelpHeader({
  titleKey,
  helpKey,
  children,
}: {
  titleKey: MessageKey
  helpKey: MessageKey
  children?: ReactNode
}) {
  const { locale } = useLocale()
  return (
    <header className="page-help-header">
      <h3>
        {t(locale, titleKey)}
        <HelpTip label={t(locale, titleKey)}>{t(locale, helpKey)}</HelpTip>
      </h3>
      {children}
    </header>
  )
}
