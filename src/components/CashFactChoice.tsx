import { useLocale } from "../context/LocaleContext"
import { t, type MessageKey } from "../i18n"
export function CashFactChoice({ field, label, value, onChange }: { field: string; label: MessageKey; value: boolean | null; onChange: (value: boolean | null) => void }) {
  const { locale } = useLocale()
  return <label>{t(locale, label)}<select data-field={field} value={value === null ? "unknown" : value ? "yes" : "no"} onChange={event => onChange(event.target.value === "unknown" ? null : event.target.value === "yes")}>
    <option value="unknown">{t(locale, "eligibility.unknown")}</option><option value="yes">{t(locale, "eligibility.yes")}</option><option value="no">{t(locale, "eligibility.no")}</option>
  </select></label>
}
