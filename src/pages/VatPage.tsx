import { AppSidebar } from "../components/AppSidebar"
import { HelpTip } from "../components/HelpTip"
import { ActionReviewDialog } from "../components/ActionReviewDialog"
import { VoucherTraceLink } from "../components/VoucherTraceLink"
import { useEffect, useRef, useState } from "react"
import { useLocation } from "react-router-dom"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import { helpTopics } from "../lib/helpTopics"
import {
  appErrorMessage,
  cashflowOverviewGet,
  taxProfileGetCurrent,
  vatProfileGetCurrent,
  vatReturnApprove,
  vatReturnDraftCreate,
  vatReturnExport,
  vatReturnTrace,
  vatThresholdStatusGet,
  workspaceSettingsGet,
  type CashflowOverview,
  type VatProfile,
  type VatReturnSummary,
  type VatThresholdStatus,
} from "../lib/commands"
import { resolveExportDirectory } from "../lib/exportDirectory"
import { formatSekMinor } from "../lib/money"
import { vatReturnStatusLabel } from "../lib/domainStatus"
import type { VatReturnTrace } from "../lib/bindings"

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
  const [approveReviewOpen, setApproveReviewOpen] = useState(false)
  const [vatTrace, setVatTrace] = useState<VatReturnTrace | null>(null)
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

  useEffect(() => {
    setVatReturn(null)
    setVatTrace(null)
    draftKeyRef.current = {}
    approveKeyRef.current = {}
  }, [periodKey])

  useEffect(() => {
    if (!vatReturn) return
    vatReturnTrace({ vatReturnId: vatReturn.id })
      .then(setVatTrace)
      .catch(() => setVatTrace(null))
  }, [vatReturn])

  async function handleDraftCreate() {
    if (busy) return
    if (!periodKey.trim()) {
      setStatus(t(locale, "vat.selectPeriod"))
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
    if (vatReturn.periodKey !== periodKey.trim()) {
      setStatus(t(locale, "vat.periodMismatch"))
      return
    }
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

  function openApproveReview() {
    if (vatReturn && vatReturn.status !== "approved" && !busy) {
      setApproveReviewOpen(true)
    }
  }

  function confirmApproveReview() {
    setApproveReviewOpen(false)
    void handleApprove()
  }

  async function handleExport() {
    if (busy || !vatReturn || vatReturn.status !== "approved") return
    setBusy(true)
    try {
      const exportDirectory = await resolveExportDirectory(
        t(locale, "vat.chooseExportFolder"),
        defaultExportDirectory,
      )
      if (!exportDirectory) {
        setStatus(t(locale, "vat.exportCancelled"))
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
  const displayedBoxes = vatTrace
    ? vatTrace.boxes.map(({ boxNumber, amountMinor }) => ({ boxCode: boxNumber, amountMinor }))
    : vatReturn?.boxes ?? []

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
            <span>{t(locale, "vat.turnoverYear")}</span>
            <strong>{threshold ? formatSekMinor(threshold.annualTurnoverMinor) : "—"}</strong>
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
            <span>{t(locale, "vat.vatReserve")}</span>
            <strong>{cashflow ? formatSekMinor(cashflow.vatReserveMinor) : "—"}</strong>
          </article>
          <article className="metric metric-neutral">
            <span>{t(locale, "vat.spendableCash")}</span>
            <strong>{cashflow ? formatSekMinor(cashflow.spendableCashMinor) : "—"}</strong>
          </article>
        </section>

        <section className="workbench">
          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "vat.draft")}</p>
              <h3>{t(locale, "vat.period")}</h3>
            </header>
            <label>
              {t(locale, "vat.periodKey")}
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
                {tVars(locale, "vat.reportingStatus", {
                  period: vatProfile.reportingPeriod,
                  status: vatProfile.vatStatus,
                })}
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
                onClick={openApproveReview}
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
                    <dt>{t(locale, "vat.returnStatus")}</dt>
                    <dd>{vatReturnStatusLabel(locale, vatReturn.status)}</dd>
                  </div>
                  <div>
                    <dt>{t(locale, "vat.box49")}</dt>
                    <dd>{formatSekMinor(vatReturn.box49AmountMinor)}</dd>
                  </div>
                  <div>
                    <dt>{t(locale, "vat.zeroReturn")}</dt>
                    <dd>{vatReturn.zeroReturn ? t(locale, "vat.yes") : t(locale, "vat.no")}</dd>
                  </div>
                </dl>
                {displayedBoxes.length > 0 ? (
                  <table>
                    <thead>
                      <tr>
                        <th scope="col">{t(locale, "vat.boxColumn")}</th>
                        <th scope="col">{t(locale, "vat.amountColumn")}</th>
                        <th scope="col">{t(locale, "vat.traceColumn")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {displayedBoxes.map((box) => (
                        <tr key={box.boxCode}>
                          <td>{box.boxCode}</td>
                          <td>{formatSekMinor(box.amountMinor)}</td>
                          <td>
                            {vatTrace?.boxes
                              .find((trace) => trace.boxNumber === box.boxCode)
                              ?.voucherIds.map((voucherId) => (
                                <VoucherTraceLink
                                  key={voucherId}
                                  voucherId={voucherId}
                                  label={t(locale, "vat.traceVoucher")}
                                />
                              ))}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                ) : null}
              </>
            ) : null}
          </div>
        </section>
        {vatReturn ? (
          <ActionReviewDialog
            open={approveReviewOpen}
            title={t(locale, "actionReview.vat.title")}
            summary={tVars(locale, "actionReview.vat.summary", {
              period: vatReturn.periodKey,
              amount: formatSekMinor(vatReturn.box49AmountMinor),
              status: vatReturnStatusLabel(locale, vatReturn.status),
            })}
            consequences={[t(locale, "actionReview.vat.consequence")]}
            correction={null}
            confirmLabel={t(locale, "actionReview.vat.confirm")}
            cancelLabel={t(locale, "actionReview.cancel")}
            busy={busy}
            onConfirm={confirmApproveReview}
            onCancel={() => setApproveReviewOpen(false)}
          />
        ) : null}
      </section>
    </main>
  )
}
