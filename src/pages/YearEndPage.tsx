import { Link, useLocation, useSearchParams } from "react-router-dom"
import { useEffect, useRef, useState } from "react"
import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { ActionReviewDialog } from "../components/ActionReviewDialog"
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
  yearEndPackageRegenerate,
  yearEndReadinessGet,
  workspaceSettingsGet,
  type TaxProfile,
  type YearEndPackageSummary,
  type YearEndReadiness,
} from "../lib/commands"
import { resolveExportDirectory } from "../lib/exportDirectory"
import { formatSekMinor } from "../lib/money"
import {
  yearEndPackageStatusLabel,
  yearEndReadinessDestination,
  yearEndReadinessLabel,
} from "../lib/domainStatus"

export function YearEndPage() {
  const { workspace } = useWorkspace()
  const { locale } = useLocale()
  const location = useLocation()
  const [searchParams] = useSearchParams()
  const requestedFiscalYear = searchParams.get("fiscalYear")
  const taxTaskFiscalYear =
    requestedFiscalYear && /^\d{4}$/.test(requestedFiscalYear) ? Number(requestedFiscalYear) : null
  const [taxProfile, setTaxProfile] = useState<TaxProfile | null>(null)
  const [fiscalYear, setFiscalYear] = useState(taxTaskFiscalYear ?? new Date().getFullYear())
  const [yearPackage, setYearPackage] = useState<YearEndPackageSummary | null>(null)
  const [readiness, setReadiness] = useState<YearEndReadiness | null>(null)
  const [yearEndDataAvailable, setYearEndDataAvailable] = useState(false)
  const [status, setStatus] = useState(t(locale, "yearEnd.status"))
  const [busy, setBusy] = useState(false)
  const [defaultExportDirectory, setDefaultExportDirectory] = useState<string | null>(null)
  const [approveReviewOpen, setApproveReviewOpen] = useState(false)
  const [regenerateReviewOpen, setRegenerateReviewOpen] = useState(false)
  const createKeyRef = useRef<Record<number, string>>({})
  const exportKeyRef = useRef<Record<string, string>>({})

  useEffect(() => {
    if (taxTaskFiscalYear !== null) {
      setFiscalYear(taxTaskFiscalYear)
    }
  }, [taxTaskFiscalYear])
  const approveKeyRef = useRef<Record<string, string>>({})
  const regenerateKeyRef = useRef<Record<string, string>>({})

  useEffect(() => {
    if (!workspace) return
    taxProfileGetCurrent()
      .then((profile) => {
        setTaxProfile(profile)
        if (profile?.activeRuleYear && taxTaskFiscalYear === null) {
          setFiscalYear(profile.activeRuleYear)
        }
      })
      .catch(() => setTaxProfile(null))
    workspaceSettingsGet()
      .then((settings) => setDefaultExportDirectory(settings.defaultExportDirectory))
      .catch(() => setDefaultExportDirectory(null))
  }, [workspace, location.key, taxTaskFiscalYear])

  useEffect(() => {
    if (!workspace) {
      setYearEndDataAvailable(false)
      setStatus(t(locale, "yearEnd.noWorkspace"))
      return
    }
    let active = true
    void Promise.all([
      yearEndPackageFindByFiscalYear({ fiscalYear }),
      yearEndReadinessGet({ fiscalYear }),
    ])
      .then(([existing, nextReadiness]) => {
        if (!active) return
        setYearPackage(existing)
        setReadiness(nextReadiness)
        setYearEndDataAvailable(true)
        setStatus(
          existing
            ? `${t(locale, "yearEnd.packageStatus")}: ${yearEndPackageStatusLabel(locale, existing.status)}`
            : t(locale, "yearEnd.status"),
        )
      })
      .catch(() => {
        if (!active) return
        setYearEndDataAvailable(false)
        setStatus(t(locale, "yearEnd.dataUnavailable"))
      })
    return () => {
      active = false
    }
  }, [workspace, fiscalYear, location.key, locale])

  async function handleCreate() {
    if (busy || !yearEndDataAvailable) return
    setBusy(true)
    const idempotencyKey = createKeyRef.current[fiscalYear] ??= crypto.randomUUID()
    try {
      const created = await yearEndPackageCreate({
        fiscalYear,
        idempotencyKey,
      })
      delete createKeyRef.current[fiscalYear]
      setYearPackage(created)
      setStatus(
        `${t(locale, "yearEnd.packageStatus")}: ${yearEndPackageStatusLabel(locale, created.status)}`,
      )
      const nextReadiness = await yearEndReadinessGet({ fiscalYear })
      setReadiness(nextReadiness)
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "yearEnd.createFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleExport() {
    if (busy || !yearEndDataAvailable || !yearPackage) return
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
    if (!yearEndDataAvailable || !yearPackage) return
    try {
      const refreshed = await yearEndPackageGet({ packageId: yearPackage.id })
      setYearPackage(refreshed)
    } catch {
      setStatus(t(locale, "yearEnd.refreshFailed"))
    }
  }

  async function handleRegenerate() {
    if (busy || !yearEndDataAvailable || !yearPackage || yearPackage.status !== "draft") return
    setBusy(true)
    const idempotencyKey = regenerateKeyRef.current[yearPackage.id] ??= crypto.randomUUID()
    try {
      const regenerated = await yearEndPackageRegenerate({
        packageId: yearPackage.id,
        idempotencyKey,
      })
      delete regenerateKeyRef.current[yearPackage.id]
      setYearPackage(regenerated)
      const nextReadiness = await yearEndReadinessGet({ fiscalYear })
      setReadiness(nextReadiness)
      setStatus(t(locale, "yearEnd.regenerateDone"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "yearEnd.regenerateFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleApprove() {
    if (busy || !yearEndDataAvailable || !yearPackage || yearPackage.status === "approved") return
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

  function openApproveReview() {
    if (
      yearEndDataAvailable &&
      yearPackage &&
      yearPackage.status !== "approved" &&
      readiness?.readyToApprove &&
      !busy
    ) {
      setApproveReviewOpen(true)
    }
  }

  function confirmApproveReview() {
    setApproveReviewOpen(false)
    void handleApprove()
  }

  function openRegenerateReview() {
    if (
      yearEndDataAvailable &&
      yearPackage?.status === "draft" &&
      !busy
    ) {
      setRegenerateReviewOpen(true)
    }
  }

  function confirmRegenerateReview() {
    setRegenerateReviewOpen(false)
    void handleRegenerate()
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

        {!yearEndDataAvailable ? (
          <section className="panel">
            <p>{t(locale, "yearEnd.dataUnavailable")}</p>
          </section>
        ) : null}

        <section className="dashboard-grid" aria-label={t(locale, "yearEnd.overview")}>
          <article className={`metric metric-${yearEndDataAvailable && yearPackage?.k1Allowed ? "neutral" : "amber"}`}>
            <span>{t(locale, "yearEnd.k1Framework")}</span>
            <strong>
              {!yearEndDataAvailable
                ? t(locale, "yearEnd.dataUnavailable")
                : yearPackage?.k1Allowed === true
                  ? t(locale, "yearEnd.allowed")
                  : yearPackage?.k1Allowed === false
                    ? t(locale, "yearEnd.notAllowed")
                    : t(locale, "yearEnd.dataUnavailable")}
            </strong>
          </article>
          <article className="metric metric-neutral">
            <span>{t(locale, "yearEnd.neDraft")}</span>
            <strong>
              {yearEndDataAvailable && yearPackage?.neDraftPresent
                ? t(locale, "yearEnd.present")
                : t(locale, "yearEnd.dataUnavailable")}
            </strong>
          </article>
          <article className="metric metric-neutral">
            <span>{t(locale, "yearEnd.localStorage")}</span>
            <strong>
              {yearEndDataAvailable && yearPackage?.storedLocally
                ? t(locale, "yearEnd.yes")
                : t(locale, "yearEnd.dataUnavailable")}
            </strong>
          </article>
          <article className={`metric metric-${yearEndDataAvailable && yearPackage?.fiscalYearLocked ? "amber" : "neutral"}`}>
            <span>{t(locale, "yearEnd.fiscalYear")}</span>
            <strong>
              {!yearEndDataAvailable
                ? t(locale, "yearEnd.dataUnavailable")
                : yearPackage?.fiscalYearLocked
                  ? t(locale, "yearEnd.locked")
                  : t(locale, "yearEnd.open")}
            </strong>
          </article>
        </section>
        <p className="status-line">{t(locale, "yearEnd.preparationOnly")}</p>

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
                disabled={!yearEndDataAvailable || busy || Boolean(yearPackage?.fiscalYearLocked)}
              >
                {yearOptions.map((year) => (
                  <option key={year} value={year}>
                    {year}
                  </option>
                ))}
              </select>
            </label>
            <div className="button-row">
              <button type="button" onClick={handleCreate} disabled={!yearEndDataAvailable || busy || Boolean(yearPackage)}>
                {t(locale, "yearEnd.createPackage")}
              </button>
              {yearPackage ? (
                <>
                  <button
                    type="button"
                    onClick={openApproveReview}
                    disabled={
                      !yearEndDataAvailable ||
                      busy ||
                      yearPackage.status === "approved" ||
                      readiness?.readyToApprove === false
                    }
                  >
                    {t(locale, "yearEnd.approve")}
                  </button>
                  {yearPackage.status === "draft" ? (
                    <button type="button" onClick={openRegenerateReview} disabled={!yearEndDataAvailable || busy}>
                      {t(locale, "yearEnd.regenerate")}
                    </button>
                  ) : null}
                  <button type="button" onClick={handleExport} disabled={!yearEndDataAvailable || busy}>
                    {t(locale, "yearEnd.reexport")}
                  </button>
                  <button type="button" onClick={handleRefresh} disabled={!yearEndDataAvailable || busy}>
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
                {readiness.items.map((item) => {
                  const destination = yearEndReadinessDestination(item.code)
                  return (
                    <li key={item.code}>
                      {item.satisfied ? "✓" : "○"} {yearEndReadinessLabel(locale, item.code)}
                      {!item.satisfied && destination ? <Link to={destination}>{t(locale, "yearEnd.readiness")}</Link> : null}
                      {item.detail ? (
                        <details>
                          <summary>{t(locale, "yearEnd.trace")}</summary>
                          <code>{item.code}: {item.detail}</code>
                        </details>
                      ) : null}
                    </li>
                  )
                })}
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
                {t(locale, "yearEnd.packageStatusLabel")}:{" "}
                {yearEndPackageStatusLabel(locale, yearPackage.status)}
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
                      <td>{formatSekMinor(field.amountMinor)}</td>
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
              <p className="status-line">{t(locale, "yearEnd.preparationOnly")}</p>
            </div>
          ) : null}
        </section>
      </section>
        {yearPackage ? (
          <ActionReviewDialog
            open={approveReviewOpen}
            title={t(locale, "actionReview.yearEnd.title")}
            summary={tVars(locale, "actionReview.yearEnd.summary", {
              year: yearPackage.fiscalYear,
              k1: t(
                locale,
                yearPackage.k1Allowed
                  ? "actionReview.readiness.ready"
                  : "actionReview.readiness.blocked",
              ),
              ne: t(
                locale,
                yearPackage.neDraftPresent
                  ? "actionReview.readiness.ready"
                  : "actionReview.readiness.missing",
              ),
            })}
            consequences={[
              t(locale, "actionReview.yearEnd.consequence"),
              t(locale, "yearEnd.preparationOnly"),
              ...(readiness?.items
                .filter((item) => !item.satisfied)
                .map((item) => yearEndReadinessLabel(locale, item.code)) ?? []),
            ]}
            correction={null}
            confirmLabel={t(locale, "actionReview.yearEnd.confirm")}
            cancelLabel={t(locale, "actionReview.cancel")}
            busy={busy}
            onConfirm={confirmApproveReview}
            onCancel={() => setApproveReviewOpen(false)}
          />
        ) : null}
        {yearPackage?.status === "draft" ? (
          <ActionReviewDialog
            open={regenerateReviewOpen}
            title={t(locale, "yearEnd.regenerateReviewTitle")}
            summary={t(locale, "yearEnd.regenerateReviewSummary")}
            consequences={[
              t(locale, "yearEnd.regenerateReviewConsequence"),
              t(locale, "yearEnd.preparationOnly"),
            ]}
            correction={null}
            confirmLabel={t(locale, "yearEnd.regenerateReviewConfirm")}
            cancelLabel={t(locale, "actionReview.cancel")}
            busy={busy}
            onConfirm={confirmRegenerateReview}
            onCancel={() => setRegenerateReviewOpen(false)}
          />
        ) : null}
    </main>
  )
}
