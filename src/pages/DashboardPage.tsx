import { Link, useLocation } from "react-router-dom"
import { useEffect, useMemo, useRef, useState } from "react"
import { AppSidebar } from "../components/AppSidebar"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { t } from "../i18n"
import {
  appErrorMessage,
  cashflowOverviewGet,
  complianceProfileCheck,
  invoiceOpenCount,
  ruleVersionGet,
  stagedTransactionsCount,
  taxProfileGetCurrent,
  vatProfileGetCurrent,
  vatThresholdStatusGet,
  workspaceBackupCreate,
  workspaceClose,
  dashboardTourMarkComplete,
  workspaceSettingsGet,
  yearEndReadinessGet,
  taxTasksList,
  type CashflowOverview,
  type ComplianceProfileCheckResult,
  type RuleVersionSummary,
  type VatThresholdStatus,
  type YearEndReadiness,
} from "../lib/commands"
import { buildDashboardChecklist } from "../lib/dashboardChecklist"
import { checklistItemDetail } from "../lib/dashboardChecklistDetail"
import { dashboardTourSteps } from "../lib/dashboardTour"
import { pickSaveBackupFile } from "../lib/dialogs"
import { formatSekMinor } from "../lib/money"
import { GuidedTour } from "../components/GuidedTour"
import { formatIsoDate } from "../lib/date"
import { presentTaxTask } from "../lib/taxTaskPresentation"
import type { TaxTask } from "../lib/bindings"

export function DashboardPage() {
  const { workspace, setWorkspace } = useWorkspace()
  const { locale } = useLocale()
  const location = useLocation()
  const fiscalYear = new Date().getFullYear()
  const [ruleVersion, setRuleVersion] = useState<RuleVersionSummary | null>(null)
  const [compliance, setCompliance] = useState<ComplianceProfileCheckResult | null>(null)
  const [openInvoices, setOpenInvoices] = useState(0)
  const [stagedCount, setStagedCount] = useState(0)
  const [cashflow, setCashflow] = useState<CashflowOverview | null>(null)
  const [threshold, setThreshold] = useState<VatThresholdStatus | null>(null)
  const [yearEndReadiness, setYearEndReadiness] = useState<YearEndReadiness | null>(null)
  const [status, setStatus] = useState("")
  const [busy, setBusy] = useState(false)
  const [backupPassphrase, setBackupPassphrase] = useState("")
  const [backupPassphraseConfirm, setBackupPassphraseConfirm] = useState("")
  const [defaultBackupDirectory, setDefaultBackupDirectory] = useState<string | null>(null)
  const [tourActive, setTourActive] = useState(false)
  const [taxTasks, setTaxTasks] = useState<TaxTask[]>([])
  const [dashboardDataAvailable, setDashboardDataAvailable] = useState(false)
  const [dashboardReload, setDashboardReload] = useState(0)
  const backupIdempotencyKey = useRef<string | null>(null)
  const backupDestinationPath = useRef<string | null>(null)

  useEffect(() => {
    if (!workspace) {
      setDashboardDataAvailable(false)
      setStatus(t(locale, "yearEnd.noWorkspace"))
      return
    }
    workspaceSettingsGet()
      .then((settings) => {
        setDefaultBackupDirectory(settings.defaultBackupDirectory)
        if (!settings.dashboardTourCompleted) {
          setTourActive(true)
        }
      })
      .catch(() => setDefaultBackupDirectory(null))

    let active = true
    void Promise.all([
      ruleVersionGet(),
      taxProfileGetCurrent(),
      vatProfileGetCurrent(),
      invoiceOpenCount(),
      stagedTransactionsCount("staged"),
      cashflowOverviewGet(),
      vatThresholdStatusGet(),
      yearEndReadinessGet({ fiscalYear }),
      taxTasksList({ asOfDate: new Date().toISOString().slice(0, 10) }),
    ])
      .then(
        ([
          nextRuleVersion,
          taxProfile,
          vatProfile,
          nextOpenInvoices,
          nextStagedCount,
          nextCashflow,
          nextThreshold,
          nextYearEndReadiness,
          nextTaxTasks,
        ]) =>
          complianceProfileCheck({
            taxStatus: taxProfile?.taxStatus ?? "f_skatt",
            vatStatus: vatProfile?.vatStatus ?? "exempt_low_turnover",
            expectedSalaryIncomeMinor: taxProfile?.expectedSalaryIncomeMinor ?? null,
            expectedBusinessProfitMinor: taxProfile?.expectedBusinessProfitMinor ?? null,
            ruleYear: taxProfile?.activeRuleYear ?? null,
          }).then((nextCompliance) => ({
            nextRuleVersion,
            nextCompliance,
            nextOpenInvoices,
            nextStagedCount,
            nextCashflow,
            nextThreshold,
            nextYearEndReadiness,
            nextTaxTasks,
          })),
      )
      .then((data) => {
        if (!active) return
        setRuleVersion(data.nextRuleVersion)
        setCompliance(data.nextCompliance)
        setOpenInvoices(data.nextOpenInvoices)
        setStagedCount(data.nextStagedCount)
        setCashflow(data.nextCashflow)
        setThreshold(data.nextThreshold)
        setYearEndReadiness(data.nextYearEndReadiness)
        setTaxTasks(data.nextTaxTasks)
        setDashboardDataAvailable(true)
        setStatus("")
      })
      .catch(() => {
        if (!active) return
        setDashboardDataAvailable(false)
        setStatus(t(locale, "dashboard.dataUnavailable"))
      })
    return () => {
      active = false
    }
  }, [workspace, location.key, fiscalYear, locale, dashboardReload])

  const checklistInput = useMemo(
    () => ({
      dataUnavailable: !dashboardDataAvailable,
      compliancePassed: compliance?.passed ?? null,
      vatWarning: threshold?.warning,
      stagedCount,
      openInvoices,
      yearEndReady: yearEndReadiness?.readyToApprove ?? null,
      unsatisfiedYearEndCodes:
        yearEndReadiness?.items.filter((item) => !item.satisfied).map((item) => item.code) ?? [],
    }),
    [dashboardDataAvailable, compliance, threshold, stagedCount, openInvoices, yearEndReadiness],
  )

  const checklist = useMemo(() => buildDashboardChecklist(checklistInput), [checklistInput])

  async function handleBackup() {
    if (busy) return
    if (backupPassphrase.trim().length < 12) {
      setStatus(t(locale, "dashboard.backup.passphraseTooShort"))
      return
    }
    if (backupPassphrase !== backupPassphraseConfirm) {
      setStatus(t(locale, "dashboard.backup.passphraseMismatch"))
      return
    }
    setBusy(true)
    setStatus(t(locale, "dashboard.backup.creating"))
    try {
      const suggestedName = `${workspace?.name ?? "workspace"}-${new Date().toISOString().slice(0, 10)}.skatbackup`
      const defaultPath = defaultBackupDirectory
        ? `${defaultBackupDirectory}/${suggestedName}`
        : suggestedName
      const backupFilePath = await pickSaveBackupFile(t(locale, "dashboard.backup.saveAs"), defaultPath)
      if (!backupFilePath) {
        setStatus(t(locale, "dashboard.backup.cancelled"))
        return
      }
      if (backupDestinationPath.current !== backupFilePath) {
        backupDestinationPath.current = backupFilePath
        backupIdempotencyKey.current = crypto.randomUUID()
      }
      const idempotencyKey = backupIdempotencyKey.current ?? crypto.randomUUID()
      const backup = await workspaceBackupCreate({
        idempotencyKey,
        destinationPath: null,
        backupFilePath,
        passphrase: backupPassphrase,
      })
      backupIdempotencyKey.current = null
      backupDestinationPath.current = null
      setBackupPassphrase("")
      setBackupPassphraseConfirm("")
      setStatus(`${t(locale, "dashboard.backup.created")}: ${backup.backupPath}`)
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "dashboard.backup.failed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleClose() {
    if (busy) return
    setBusy(true)
    try {
      await workspaceClose()
      setWorkspace(null)
      setStatus(t(locale, "dashboard.workspaceClosed"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "dashboard.closeFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function finishTour() {
    setTourActive(false)
    try {
      await dashboardTourMarkComplete()
    } catch {
      // Tour is non-blocking UX; persistence failure should not trap the user.
    }
  }

  return (
    <main className="app-shell">
      <GuidedTour
        locale={locale}
        steps={dashboardTourSteps}
        active={tourActive}
        onComplete={() => void finishTour()}
        onSkip={() => void finishTour()}
      />
      <AppSidebar current="dashboard" />

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{t(locale, "dashboard.eyebrow")}</p>
            <h2>{workspace?.name ?? t(locale, "yearEnd.noWorkspace")}</h2>
            <p className="status-line" role="status" aria-live="polite">
              {status}
            </p>
          </div>
          <div className="workspace-create" data-tour="backup">
            <input
              aria-label={t(locale, "dashboard.backup.passphrase")}
              type="password"
              placeholder={t(locale, "dashboard.backup.passphrasePlaceholder")}
              value={backupPassphrase}
              onChange={(event) => setBackupPassphrase(event.target.value)}
              disabled={busy || !workspace}
            />
            <input
              aria-label={t(locale, "dashboard.backup.passphraseConfirm")}
              type="password"
              placeholder={t(locale, "dashboard.backup.passphraseConfirm")}
              value={backupPassphraseConfirm}
              onChange={(event) => setBackupPassphraseConfirm(event.target.value)}
              disabled={busy || !workspace}
            />
            <button type="button" onClick={() => void handleBackup()} disabled={busy || !workspace}>
              {t(locale, "dashboard.backup")}
            </button>
            <button type="button" className="secondary" onClick={handleClose} disabled={busy}>
              {t(locale, "dashboard.close")}
            </button>
            <Link to="/onboarding">{t(locale, "dashboard.editProfiles")}</Link>
          </div>
        </header>

        <section className="dashboard-grid" aria-label={t(locale, "dashboard.title")}>
          <article className={`metric metric-${dashboardDataAvailable ? "neutral" : "amber"}`} data-tour="spendable-cash">
            <span>{t(locale, "dashboard.spendableCash")}</span>
            <strong>
              {dashboardDataAvailable && cashflow
                ? formatSekMinor(cashflow.spendableCashMinor)
                : t(locale, "dashboard.dataUnavailable")}
            </strong>
          </article>
        </section>

        <section className="workbench">
          <div className="panel" data-tour="checklist">
            <header>
              <p className="eyebrow">{t(locale, "dashboard.checklist.title")}</p>
              <h3>{t(locale, "dashboard.title")}</h3>
            </header>
            {!dashboardDataAvailable ? (
              <button type="button" className="secondary" onClick={() => setDashboardReload((value) => value + 1)}>
                {t(locale, "dashboard.retry")}
              </button>
            ) : null}
            <ul className="checklist">
              {checklist.map((item) => {
                const detail = checklistItemDetail(locale, item, checklistInput)
                return (
                  <li key={item.id} className={`checklist-item checklist-${item.tone}`}>
                    <Link to={item.href}>{t(locale, item.labelKey)}</Link>
                    {detail ? <span className="muted">{detail}</span> : null}
                  </li>
                )
              })}
            </ul>
          </div>

          {dashboardDataAvailable && taxTasks.length > 0 ? (
            <div className="panel" data-tour="tax-tasks">
              <header>
                <p className="eyebrow">{t(locale, "taxTasks.title")}</p>
                <h3>{t(locale, "taxTasks.title")}</h3>
              </header>
              <p className="muted">{t(locale, "taxTasks.submitOutside")}</p>
              <ul className="checklist">
                {taxTasks.map((task) => {
                  const presentation = presentTaxTask(task)
                  return (
                    <li key={task.id} className="checklist-item checklist-amber">
                      <Link to={`${presentation.route}${presentation.search}`}>
                        {t(locale, presentation.actionKey)}
                      </Link>
                      <span className="muted">
                        {task.periodKey} · {t(locale, presentation.statusKey)}
                        {task.dueOn ? ` · ${formatIsoDate(locale, task.dueOn)}` : ""}
                      </span>
                      <a href={task.sourceUrl} target="_blank" rel="noreferrer">
                        {t(locale, "dashboard.rulesSource")}
                      </a>
                    </li>
                  )
                })}
              </ul>
            </div>
          ) : null}

          <div className="panel" data-tour="rules">
            <header>
              <p className="eyebrow">{t(locale, "dashboard.rules")}</p>
              <h3>{dashboardDataAvailable && ruleVersion ? String(ruleVersion.taxYear) : t(locale, "dashboard.dataUnavailable")}</h3>
            </header>
            {dashboardDataAvailable && ruleVersion ? (
              <p>
                <a href={ruleVersion.sourceUrl} target="_blank" rel="noreferrer">
                  {t(locale, "dashboard.rulesSource")}
                </a>
              </p>
            ) : null}
            {dashboardDataAvailable && compliance ? (
              <p>
                {compliance.passed
                  ? t(locale, "dashboard.compliance.passed")
                  : t(locale, "dashboard.compliance.failed")}
              </p>
            ) : null}
          </div>
        </section>
      </section>
    </main>
  )
}
