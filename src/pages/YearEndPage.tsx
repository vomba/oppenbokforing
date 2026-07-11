import { useLocation } from "react-router-dom"
import { useEffect, useRef, useState } from "react"
import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import { helpTopics } from "../lib/helpTopics"
import {
  appErrorMessage,
  taxProfileGetCurrent,
  yearEndPackageApprove,
  yearEndPackageCreate,
  yearEndPackageExport,
  yearEndPackageFindByFiscalYear,
  yearEndPackageGet,
  yearEndReadinessGet,
  workspaceSettingsGet,
  type TaxProfile,
  type YearEndPackageSummary,
  type YearEndReadiness,
} from "../lib/commands"
import { resolveExportDirectory } from "../lib/exportDirectory"

function formatSek(minor: number) {
  return `${(minor / 100).toLocaleString("sv-SE", { minimumFractionDigits: 2 })} kr`
}

export function YearEndPage() {
  const { workspace } = useWorkspace()
  const { locale } = useLocale()
  const location = useLocation()
  const [taxProfile, setTaxProfile] = useState<TaxProfile | null>(null)
  const [fiscalYear, setFiscalYear] = useState(new Date().getFullYear())
  const [yearPackage, setYearPackage] = useState<YearEndPackageSummary | null>(null)
  const [readiness, setReadiness] = useState<YearEndReadiness | null>(null)
  const [status, setStatus] = useState(t(locale, "yearEnd.status"))
  const [busy, setBusy] = useState(false)
  const [defaultExportDirectory, setDefaultExportDirectory] = useState<string | null>(null)
  const createKeyRef = useRef<Record<number, string>>({})
  const exportKeyRef = useRef<Record<string, string>>({})
  const approveKeyRef = useRef<Record<string, string>>({})

  useEffect(() => {
    if (!workspace) return
    taxProfileGetCurrent()
      .then((profile) => {
        setTaxProfile(profile)
        if (profile?.activeRuleYear) {
          setFiscalYear(profile.activeRuleYear)
        }
      })
      .catch(() => setTaxProfile(null))
    workspaceSettingsGet()
      .then((settings) => setDefaultExportDirectory(settings.defaultExportDirectory))
      .catch(() => setDefaultExportDirectory(null))
  }, [workspace, location.key])

  useEffect(() => {
    if (!workspace) {
      setYearPackage(null)
      return
    }
    yearEndPackageFindByFiscalYear({ fiscalYear })
      .then((existing) => {
        setYearPackage(existing)
        if (existing) {
          setStatus(`${t(locale, "yearEnd.packageStatus")}: ${existing.status}`)
        } else {
          setStatus(t(locale, "yearEnd.status"))
        }
      })
      .catch(() => setYearPackage(null))

    yearEndReadinessGet({ fiscalYear })
      .then(setReadiness)
      .catch(() => setReadiness(null))
  }, [workspace, fiscalYear, location.key, locale])

  async function handleCreate() {
    if (busy) return
    setBusy(true)
    const idempotencyKey = createKeyRef.current[fiscalYear] ??= crypto.randomUUID()
    try {
      const created = await yearEndPackageCreate({
        fiscalYear,
        idempotencyKey,
      })
      delete createKeyRef.current[fiscalYear]
      setYearPackage(created)
      setStatus(`${t(locale, "yearEnd.packageStatus")}: ${created.status}`)
      const nextReadiness = await yearEndReadinessGet({ fiscalYear })
      setReadiness(nextReadiness)
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "yearEnd.createFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleExport() {
    if (busy || !yearPackage) return
    setBusy(true)
    try {
      const exportDirectory = await resolveExportDirectory(
        t(locale, "settings.defaultExportDirectory"),
        defaultExportDirectory,
      )
      if (!exportDirectory) {
        setStatus(t(locale, "settings.exportCancelled"))
        return
      }
      const idempotencyKey = exportKeyRef.current[yearPackage.id] ??= crypto.randomUUID()
      const exported = await yearEndPackageExport({
        packageId: yearPackage.id,
        idempotencyKey,
        exportDirectory,
      })
      delete exportKeyRef.current[yearPackage.id]
      setYearPackage(exported)
      setStatus(`${t(locale, "yearEnd.exportSaved")}: ${exported.exportPath ?? "exports/year-end"}`)
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "yearEnd.exportFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleRefresh() {
    if (!yearPackage) return
    try {
      const refreshed = await yearEndPackageGet({ packageId: yearPackage.id })
      setYearPackage(refreshed)
    } catch {
      setStatus(t(locale, "yearEnd.refreshFailed"))
    }
  }

  async function handleApprove() {
    if (busy || !yearPackage || yearPackage.status === "approved") return
    const confirmed = window.confirm(t(locale, "yearEnd.approveConfirm"))
    if (!confirmed) return

    setBusy(true)
    const idempotencyKey = approveKeyRef.current[yearPackage.id] ??= crypto.randomUUID()
    try {
      const approved = await yearEndPackageApprove({
        packageId: yearPackage.id,
        idempotencyKey,
      })
      delete approveKeyRef.current[yearPackage.id]
      setYearPackage(approved)
      const nextReadiness = await yearEndReadinessGet({ fiscalYear })
      setReadiness(nextReadiness)
      setStatus(t(locale, "yearEnd.approveDone"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "yearEnd.approveFailed")))
    } finally {
      setBusy(false)
    }
  }

  const yearOptions = [fiscalYear - 1, fiscalYear, fiscalYear + 1].filter(
    (year, index, arr) => arr.indexOf(year) === index,
  )

  return (
    <main className="app-shell">
      <AppSidebar current="yearEnd" />

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{t(locale, "yearEnd.eyebrow")}</p>
            <h2>
              {t(locale, helpTopics.yearEnd.title)}
              <HelpTip label={t(locale, helpTopics.yearEnd.title)}>
                {t(locale, helpTopics.yearEnd.help)}
              </HelpTip>
            </h2>
            <p className="status-line" aria-live="polite">
              {status}
            </p>
          </div>
        </header>

        <section className="dashboard-grid" aria-label={t(locale, "yearEnd.overview")}>
          <article className={`metric metric-${yearPackage?.k1Allowed ? "neutral" : "amber"}`}>
            <span>{t(locale, "yearEnd.k1Framework")}</span>
            <strong>
              {yearPackage?.k1Allowed === true
                ? t(locale, "yearEnd.allowed")
                : yearPackage?.k1Allowed === false
                  ? t(locale, "yearEnd.notAllowed")
                  : "—"}
            </strong>
          </article>
          <article className="metric metric-neutral">
            <span>{t(locale, "yearEnd.neDraft")}</span>
            <strong>
              {yearPackage?.neDraftPresent
                ? t(locale, "yearEnd.present")
                : "—"}
            </strong>
          </article>
          <article className="metric metric-neutral">
            <span>{t(locale, "yearEnd.localStorage")}</span>
            <strong>
              {yearPackage?.storedLocally ? t(locale, "yearEnd.yes") : "—"}
            </strong>
          </article>
          <article
            className={`metric metric-${yearPackage?.fiscalYearLocked ? "amber" : "neutral"}`}
          >
            <span>{t(locale, "yearEnd.fiscalYear")}</span>
            <strong>
              {yearPackage?.fiscalYearLocked
                ? t(locale, "yearEnd.locked")
                : t(locale, "yearEnd.open")}
            </strong>
          </article>
        </section>

        <section className="workbench">
          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "yearEnd.packageEyebrow")}</p>
              <h3>{t(locale, "yearEnd.fiscalYear")}</h3>
            </header>
            <label>
              {t(locale, "yearEnd.yearLabel")}
              <select
                value={fiscalYear}
                onChange={(e) => setFiscalYear(Number(e.target.value))}
                disabled={busy || Boolean(yearPackage?.fiscalYearLocked)}
              >
                {yearOptions.map((year) => (
                  <option key={year} value={year}>
                    {year}
                  </option>
                ))}
              </select>
            </label>
            <div className="button-row">
              <button type="button" onClick={handleCreate} disabled={busy || Boolean(yearPackage)}>
                {t(locale, "yearEnd.createPackage")}
              </button>
              {yearPackage ? (
                <>
                  <button
                    type="button"
                    onClick={() => void handleApprove()}
                    disabled={
                      busy ||
                      yearPackage.status === "approved" ||
                      readiness?.readyToApprove === false
                    }
                  >
                    {t(locale, "yearEnd.approve")}
                  </button>
                  <button type="button" onClick={handleExport} disabled={busy}>
                    {t(locale, "yearEnd.reexport")}
                  </button>
                  <button type="button" onClick={handleRefresh} disabled={busy}>
                    {t(locale, "yearEnd.refresh")}
                  </button>
                </>
              ) : null}
            </div>
          </div>

          {readiness ? (
            <div className="panel">
              <header>
                <p className="eyebrow">{t(locale, "yearEnd.checklist")}</p>
                <h3>{t(locale, "yearEnd.readiness")}</h3>
              </header>
              <ul>
                {readiness.items.map((item) => (
                  <li key={item.code}>
                    {item.satisfied ? "✓" : "○"} {item.code}
                    {item.detail ? ` — ${item.detail}` : ""}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {yearPackage ? (
            <div className="panel">
              <header>
                <p className="eyebrow">{t(locale, "yearEnd.neFieldsEyebrow")}</p>
                <h3>{t(locale, "yearEnd.ledgerMapping")}</h3>
              </header>
              <p className="status-line">
                {t(locale, "yearEnd.ruleVersion")}: {yearPackage.ruleVersionId} ·{" "}
                {t(locale, "yearEnd.packageStatusLabel")}: {yearPackage.status}
              </p>
              <table className="data-table">
                <thead>
                  <tr>
                    <th scope="col">{t(locale, "yearEnd.fieldCol")}</th>
                    <th scope="col">{t(locale, "yearEnd.amountCol")}</th>
                    <th scope="col">{t(locale, "yearEnd.sourceCol")}</th>
                  </tr>
                </thead>
                <tbody>
                  {yearPackage.neFields.map((field) => (
                    <tr key={field.fieldCode}>
                      <td>{field.fieldCode}</td>
                      <td>{formatSek(field.amountMinor)}</td>
                      <td>{field.sourceRef ?? field.sourceType}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {yearPackage.exportPath ? (
                <p className="status-line">
                  {t(locale, "yearEnd.exportPath")}: {yearPackage.exportPath}
                </p>
              ) : null}
            </div>
          ) : null}
        </section>
      </section>
    </main>
  )
}
