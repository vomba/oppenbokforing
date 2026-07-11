import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { useEffect, useRef, useState } from "react"
import { useLocation } from "react-router-dom"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { t } from "../i18n"
import { helpTopics } from "../lib/helpTopics"
import {
  appErrorMessage,
  cashflowOverviewGet,
  taxProfileGetCurrent,
  vatProfileGetCurrent,
  vatReturnApprove,
  vatReturnDraftCreate,
  vatReturnExport,
  vatThresholdStatusGet,
  workspaceSettingsGet,
  type CashflowOverview,
  type VatProfile,
  type VatReturnSummary,
  type VatThresholdStatus,
} from "../lib/commands"
import { resolveExportDirectory } from "../lib/exportDirectory"

function formatSek(minor: number) {
  return `${(minor / 100).toLocaleString("sv-SE", { minimumFractionDigits: 2 })} kr`
}

function periodKeysForYear(reportingPeriod: string, year: number): string[] {
  if (reportingPeriod === "yearly") return [String(year)]
  if (reportingPeriod === "monthly") {
    return Array.from({ length: 12 }, (_, i) => `${year}-M${String(i + 1).padStart(2, "0")}`)
  }
  return [`${year}-Q1`, `${year}-Q2`, `${year}-Q3`, `${year}-Q4`]
}

function defaultPeriodKey(reportingPeriod: string, year: number) {
  if (reportingPeriod === "yearly") return String(year)
  if (reportingPeriod === "monthly") return `${year}-M01`
  return `${year}-Q1`
}

function currentPeriodKey(reportingPeriod: string, year: number) {
  if (reportingPeriod === "yearly") return String(year)
  const now = new Date()
  if (now.getFullYear() !== year) {
    return defaultPeriodKey(reportingPeriod, year)
  }
  if (reportingPeriod === "monthly") {
    return `${year}-M${String(now.getMonth() + 1).padStart(2, "0")}`
  }
  const quarter = Math.floor(now.getMonth() / 3) + 1
  return `${year}-Q${quarter}`
}

export function VatPage() {
  const { workspace } = useWorkspace()
  const { locale } = useLocale()
  const location = useLocation()
  const [vatProfile, setVatProfile] = useState<VatProfile | null>(null)
  const [periodKey, setPeriodKey] = useState("")
  const [periodOptions, setPeriodOptions] = useState<string[]>([])
  const [vatReturn, setVatReturn] = useState<VatReturnSummary | null>(null)
  const [threshold, setThreshold] = useState<VatThresholdStatus | null>(null)
  const [cashflow, setCashflow] = useState<CashflowOverview | null>(null)
  const [status, setStatus] = useState("")
  const [busy, setBusy] = useState(false)
  const [defaultExportDirectory, setDefaultExportDirectory] = useState<string | null>(null)
  const draftKeyRef = useRef<Record<string, string>>({})
  const approveKeyRef = useRef<Record<string, string>>({})

  const vatRegistered =
    vatProfile?.vatStatus === "registered" ||
    vatProfile?.vatStatus === "voluntary_registered"

  useEffect(() => {
    setStatus(t(locale, "vat.status"))
  }, [locale])

  useEffect(() => {
    if (!workspace) return
    Promise.all([
      taxProfileGetCurrent().catch(() => null),
      vatProfileGetCurrent().catch(() => null),
      vatThresholdStatusGet().catch(() => null),
      cashflowOverviewGet().catch(() => null),
    ]).then(([taxProfile, profile, thresholdStatus, overview]) => {
      setVatProfile(profile)
      const reportingPeriod = profile?.reportingPeriod ?? "quarterly"
      const year = taxProfile?.activeRuleYear ?? new Date().getFullYear()
      const options = periodKeysForYear(reportingPeriod, year)
      const key = currentPeriodKey(reportingPeriod, year)
      setPeriodOptions(options)
      setPeriodKey(key)
      setThreshold(thresholdStatus)
      setCashflow(overview)
      if (profile && profile.vatStatus !== "registered" && profile.vatStatus !== "voluntary_registered") {
        setStatus(t(locale, "vat.notRegistered"))
      }
    })
    workspaceSettingsGet()
      .then((settings) => setDefaultExportDirectory(settings.defaultExportDirectory))
      .catch(() => setDefaultExportDirectory(null))
  }, [workspace, location.key, locale])

  async function handleDraftCreate() {
    if (busy) return
    if (!periodKey.trim()) {
      setStatus("Select a VAT period before creating a draft.")
      return
    }
    if (!vatRegistered) {
      setStatus(t(locale, "vat.notRegistered"))
      return
    }
    setBusy(true)
    const idempotencyKey = draftKeyRef.current[periodKey] ??= crypto.randomUUID()
    try {
      const draft = await vatReturnDraftCreate({
        periodKey: periodKey.trim(),
        idempotencyKey,
      })
      delete draftKeyRef.current[periodKey]
      setVatReturn(draft)
      setStatus(t(locale, "vat.draftCreated"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "vat.draftFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleApprove() {
    if (busy || !vatReturn || vatReturn.status === "approved") return
    setBusy(true)
    const idempotencyKey = approveKeyRef.current[vatReturn.id] ??= crypto.randomUUID()
    try {
      const approved = await vatReturnApprove({
        vatReturnId: vatReturn.id,
        idempotencyKey,
      })
      delete approveKeyRef.current[vatReturn.id]
      setVatReturn(approved)
      setStatus(t(locale, "vat.approved"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "vat.approveFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleExport() {
    if (busy || !vatReturn || vatReturn.status !== "approved") return
    setBusy(true)
    try {
      const exportDirectory = await resolveExportDirectory(
        "Choose export folder",
        defaultExportDirectory,
      )
      if (!exportDirectory) {
        setStatus("Export cancelled.")
        return
      }
      const exported = await vatReturnExport({ vatReturnId: vatReturn.id, exportDirectory })
      setVatReturn(exported)
      setStatus(t(locale, "vat.exported"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "vat.exportFailed")))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="app-shell">
      <AppSidebar current="vat" />

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{t(locale, "vat.eyebrow")}</p>
            <h2>
              {t(locale, helpTopics.vat.title)}
              <HelpTip label={t(locale, helpTopics.vat.title)}>
                {t(locale, helpTopics.vat.help)}
              </HelpTip>
            </h2>
            <p className="status-line" aria-live="polite">
              {status}
            </p>
          </div>
        </header>

        <section className="dashboard-grid" aria-label={t(locale, "vat.title")}>
          <article className="metric metric-neutral">
            <span>{t(locale, "vat.period")}</span>
            <strong>{cashflow?.vatPeriodKey ?? "—"}</strong>
          </article>
          <article className="metric metric-neutral">
            <span>Turnover (year)</span>
            <strong>{threshold ? formatSek(threshold.annualTurnoverMinor) : "—"}</strong>
          </article>
          <article
            className={`metric metric-${threshold?.warning === "breached" ? "red" : threshold?.warning === "approaching" ? "amber" : "neutral"}`}
          >
            <span>{t(locale, "vat.threshold")}</span>
            <strong>
              {threshold
                ? threshold.warning === "none"
                  ? t(locale, "vat.thresholdBelow")
                  : threshold.warning
                : "—"}
            </strong>
          </article>
          <article className="metric metric-neutral">
            <span>VAT reserve</span>
            <strong>{cashflow ? formatSek(cashflow.vatReserveMinor) : "—"}</strong>
          </article>
          <article className="metric metric-neutral">
            <span>Spendable cash</span>
            <strong>{cashflow ? formatSek(cashflow.spendableCashMinor) : "—"}</strong>
          </article>
        </section>

        <section className="workbench">
          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "vat.draft")}</p>
              <h3>{t(locale, "vat.period")}</h3>
            </header>
            <label>
              Period key
              <select
                value={periodKey}
                onChange={(e) => setPeriodKey(e.target.value)}
                disabled={periodOptions.length === 0}
              >
                {periodOptions.map((key) => (
                  <option key={key} value={key}>
                    {key}
                  </option>
                ))}
              </select>
            </label>
            {vatProfile ? (
              <p className="status-line">
                Reporting: {vatProfile.reportingPeriod} · Status: {vatProfile.vatStatus}
              </p>
            ) : null}
            <div className="workspace-create">
              <button
                type="button"
                onClick={handleDraftCreate}
                disabled={busy || !periodKey.trim() || !vatRegistered}
                aria-busy={busy}
              >
                {t(locale, "vat.createDraft")}
              </button>
              <button
                type="button"
                className="secondary"
                onClick={handleApprove}
                disabled={busy || !vatReturn || vatReturn.status === "approved"}
                aria-busy={busy}
              >
                {t(locale, "vat.approve")}
              </button>
              <button
                type="button"
                className="secondary"
                onClick={handleExport}
                disabled={busy || !vatReturn || vatReturn.status !== "approved"}
                aria-busy={busy}
              >
                {t(locale, "vat.export")}
              </button>
            </div>
            {vatReturn ? (
              <>
                <dl>
                  <div>
                    <dt>Status</dt>
                    <dd>{vatReturn.status}</dd>
                  </div>
                  <div>
                    <dt>Box 49</dt>
                    <dd>{formatSek(vatReturn.box49AmountMinor)}</dd>
                  </div>
                  <div>
                    <dt>Zero return</dt>
                    <dd>{vatReturn.zeroReturn ? "Yes" : "No"}</dd>
                  </div>
                </dl>
                {vatReturn.boxes.length > 0 ? (
                  <table>
                    <thead>
                      <tr>
                        <th scope="col">Box</th>
                        <th scope="col">Amount</th>
                      </tr>
                    </thead>
                    <tbody>
                      {vatReturn.boxes.map((box) => (
                        <tr key={box.boxCode}>
                          <td>{box.boxCode}</td>
                          <td>{formatSek(box.amountMinor)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                ) : null}
              </>
            ) : null}
          </div>
        </section>
      </section>
    </main>
  )
}
