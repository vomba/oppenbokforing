import { Link, useLocation } from "react-router-dom"
import { t } from "../i18n"
import { useLocale } from "../context/LocaleContext"
import { useSimpleMode } from "../context/SimpleModeContext"
import { navItemsForMode, type NavKey } from "../lib/workbenchNav"

export type { NavKey }

export function AppSidebar({ current }: { current: NavKey }) {
  const { locale } = useLocale()
  const { simpleMode } = useSimpleMode()
  const location = useLocation()
  const navItems = navItemsForMode(simpleMode)

  return (
    <aside className="sidebar">
      <div>
        <p className="eyebrow">{t(locale, "app.title")}</p>
        <h1>{t(locale, "app.subtitle")}</h1>
      </div>
      <nav aria-label={t(locale, "nav.primary")} data-tour="sidebar">
        {navItems.map((item) => {
          const active = item.key === current || location.pathname === item.to
          return (
            <Link key={item.key} to={item.to} aria-current={active ? "page" : undefined}>
              {t(locale, item.labelKey)}
            </Link>
          )
        })}
      </nav>
    </aside>
  )
}
