import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Link, useNavigate } from "react-router-dom"
import { HelpTip } from "../components/HelpTip"
import { useLocale } from "../context/LocaleContext"
import { useWorkspace } from "../context/WorkspaceContext"
import { t } from "../i18n"
import { profileComplianceFailureMessages } from "../lib/compliancePresentation"
import {
  appErrorMessage,
  businessProfileSaveCurrent,
  complianceProfileCheck,
  ruleVersionGet,
  taxProfileSaveCurrent,
  vatProfileSaveCurrent,
  type ComplianceProfileCheckResult,
  type RuleVersionSummary,
} from "../lib/commands"
import { humanAppError } from "../lib/errorPresentation"
import { parseSekToMinorUnits } from "../lib/money"
import {
  ONBOARDING_STEPS,
  canAdvanceOnboardingStep,
  canVisitOnboardingStep,
  nextOnboardingStep,
  previousOnboardingStep,
  type OnboardingDraft,
  type OnboardingStep,
} from "../lib/onboardingWizard"

export function OnboardingPage() {
  const navigate = useNavigate()
  const { workspace } = useWorkspace()
  const { locale } = useLocale()
  const [step, setStep] = useState<OnboardingStep>("business")
  const [draft, setDraft] = useState<OnboardingDraft>({
    businessName: "",
    ownerName: "",
    sniCode: "",
    taxStatus: "f_skatt",
    salarySek: "0",
    businessProfitSek: "0",
    vatStatus: "exempt_low_turnover",
    reportingPeriod: "quarterly",
    accountingMethod: "invoice_method",
  })
  const [ruleVersion, setRuleVersion] = useState<RuleVersionSummary | null>(null)
  const [compliance, setCompliance] = useState<ComplianceProfileCheckResult | null>(null)
  const [status, setStatus] = useState("")
  const [busy, setBusy] = useState(false)
  const reviewPreviewKey = useRef<string | null>(null)

  const activeRuleYear = ruleVersion?.taxYear ?? new Date().getFullYear()

  const sekValid = useMemo(
    () => ({
      salary: parseSekToMinorUnits(draft.salarySek) !== null,
      profit: parseSekToMinorUnits(draft.businessProfitSek) !== null,
    }),
    [draft.salarySek, draft.businessProfitSek],
  )

  const canAdvance = canAdvanceOnboardingStep(step, draft, sekValid)
  const complianceMessages =
    compliance && !compliance.passed ? profileComplianceFailureMessages(compliance) : []

  useEffect(() => {
    if (!workspace) {
      navigate("/")
    }
  }, [navigate, workspace])

  useEffect(() => {
    setStatus("")
  }, [locale])

  useEffect(() => {
    ruleVersionGet()
      .then(setRuleVersion)
      .catch(() => setRuleVersion(null))
  }, [])

  function updateDraft<K extends keyof OnboardingDraft>(key: K, value: OnboardingDraft[K]) {
    setDraft((current) => ({ ...current, [key]: value }))
  }

  async function previewCompliance() {
    const businessProfit = parseSekToMinorUnits(draft.businessProfitSek)
    const salaryIncome = parseSekToMinorUnits(draft.salarySek)
    if (businessProfit === null || salaryIncome === null) {
      return null
    }

    const result = await complianceProfileCheck({
      taxStatus: draft.taxStatus,
      vatStatus: draft.vatStatus,
      expectedBusinessProfitMinor: businessProfit,
      expectedSalaryIncomeMinor: salaryIncome,
      ruleYear: activeRuleYear,
    })
    setCompliance(result)
    return result
  }

  const persistProfiles = useCallback(async () => {
    if (!workspace) {
      return
    }

    const businessProfit = parseSekToMinorUnits(draft.businessProfitSek)
    const salaryIncome = parseSekToMinorUnits(draft.salarySek)
    if (businessProfit === null || salaryIncome === null) {
      throw new Error("invalid amounts")
    }

    await businessProfileSaveCurrent({
      businessName: draft.businessName,
      ownerName: draft.ownerName,
      residencyCountry: "SE",
      sniCode: draft.sniCode || null,
    })
    await taxProfileSaveCurrent({
      taxStatus: draft.taxStatus,
      expectedBusinessProfitMinor: businessProfit,
      expectedSalaryIncomeMinor: salaryIncome,
      activeRuleYear,
    })
    await vatProfileSaveCurrent({
      vatStatus: draft.vatStatus,
      reportingPeriod: draft.reportingPeriod,
      accountingMethod: draft.accountingMethod,
      voluntaryRegistrationDate: null,
    })
  }, [workspace, draft, activeRuleYear])

  useEffect(() => {
    if (step !== "review" || !workspace || !canVisitOnboardingStep("review", draft, sekValid)) {
      return
    }

    const previewKey = JSON.stringify({ draft, activeRuleYear })
    if (reviewPreviewKey.current === previewKey) {
      return
    }
    reviewPreviewKey.current = previewKey

    let active = true
    setBusy(true)
    void previewCompliance()
      .catch(() => {
        if (active) {
          setCompliance(null)
        }
      })
      .finally(() => {
        if (active) {
          setBusy(false)
        }
      })

    return () => {
      active = false
    }
  }, [step, workspace, draft, sekValid, activeRuleYear])

  async function handleSave() {
    if (!workspace || busy) return
    if (!canVisitOnboardingStep("review", draft, sekValid)) {
      setStatus(t(locale, "onboarding.status.invalidAmounts"))
      return
    }

    setBusy(true)
    setStatus(t(locale, "onboarding.status.saving"))
    try {
      await persistProfiles()
      const result = await previewCompliance()
      if (!result) {
        setStatus(t(locale, "onboarding.status.invalidAmounts"))
        return
      }

      if (result.passed) {
        setStatus(t(locale, "onboarding.status.saved"))
        navigate("/dashboard")
      } else {
        setStatus(t(locale, "onboarding.status.savedReview"))
      }
    } catch (error) {
      const message = humanAppError(error, "onboarding.status.saveFailed")
      setStatus(typeof message === "string" ? message : t(locale, message))
    } finally {
      setBusy(false)
    }
  }

  function handleStepSelect(target: OnboardingStep) {
    if (busy || !canVisitOnboardingStep(target, draft, sekValid)) {
      return
    }
    setStep(target)
  }

  function handleNext() {
    if (!canAdvance || busy) return
    const next = nextOnboardingStep(step)
    if (next) {
      setStep(next)
    }
  }

  function handleBack() {
    const previous = previousOnboardingStep(step)
    if (previous) {
      setStep(previous)
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div>
          <p className="eyebrow">{t(locale, "onboarding.eyebrow")}</p>
          <h1>{t(locale, "onboarding.title")}</h1>
        </div>
        <nav aria-label={t(locale, "onboarding.eyebrow")}>
          {ONBOARDING_STEPS.map((item) => {
            const visitable = canVisitOnboardingStep(item, draft, sekValid)
            return (
              <button
                key={item}
                type="button"
                className={`wizard-step${step === item ? " wizard-step-active" : ""}`}
                aria-current={step === item ? "step" : undefined}
                disabled={!visitable || busy}
                onClick={() => handleStepSelect(item)}
              >
                {t(locale, `onboarding.step.${item}`)}
              </button>
            )
          })}
        </nav>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{workspace?.name ?? "Workspace"}</p>
            <h2>{t(locale, `onboarding.${step}.title`)}</h2>
          </div>
          <Link to="/">{t(locale, "onboarding.switchWorkspace")}</Link>
        </header>

        {status ? (
          <p className="status-line" role="status" aria-live="polite">
            {status}
          </p>
        ) : null}

        {step === "business" ? (
          <section className="panel onboarding-panel">
            <div className="field-label-row">
              <label htmlFor="onboarding-business-name">{t(locale, "onboarding.business.name")}</label>
              <HelpTip label={t(locale, "onboarding.business.name")}>
                {t(locale, "onboarding.business.nameHelp")}
              </HelpTip>
            </div>
            <input
              id="onboarding-business-name"
              value={draft.businessName}
              onChange={(event) => updateDraft("businessName", event.target.value)}
            />
            <div className="field-label-row">
              <label htmlFor="onboarding-owner-name">{t(locale, "onboarding.business.owner")}</label>
              <HelpTip label={t(locale, "onboarding.business.owner")}>
                {t(locale, "onboarding.business.ownerHelp")}
              </HelpTip>
            </div>
            <input
              id="onboarding-owner-name"
              value={draft.ownerName}
              onChange={(event) => updateDraft("ownerName", event.target.value)}
            />
            <div className="field-label-row">
              <label htmlFor="onboarding-sni-code">{t(locale, "onboarding.business.sni")}</label>
              <HelpTip label={t(locale, "onboarding.business.sni")}>
                {t(locale, "onboarding.business.sniHelp")}
              </HelpTip>
            </div>
            <input
              id="onboarding-sni-code"
              value={draft.sniCode}
              onChange={(event) => updateDraft("sniCode", event.target.value)}
            />
          </section>
        ) : null}

        {step === "tax" ? (
          <section className="panel onboarding-panel">
            <label>
              {t(locale, "onboarding.tax.status")}
              <HelpTip label={t(locale, "onboarding.tax.status")}>
                {t(locale, "onboarding.tax.statusHelp")}
              </HelpTip>
              <select
                value={draft.taxStatus}
                onChange={(event) =>
                  updateDraft("taxStatus", event.target.value as OnboardingDraft["taxStatus"])
                }
              >
                <option value="f_skatt">{t(locale, "onboarding.tax.fSkatt")}</option>
                <option value="fa_skatt">{t(locale, "onboarding.tax.faSkatt")}</option>
              </select>
            </label>
            <label>
              {t(locale, "onboarding.tax.salary")}
              <HelpTip label={t(locale, "onboarding.tax.salary")}>
                {t(locale, "onboarding.tax.salaryHelp")}
              </HelpTip>
              <input
                inputMode="decimal"
                value={draft.salarySek}
                onChange={(event) => updateDraft("salarySek", event.target.value)}
              />
            </label>
            <label>
              {t(locale, "onboarding.tax.profit")}
              <HelpTip label={t(locale, "onboarding.tax.profit")}>
                {t(locale, "onboarding.tax.profitHelp")}
              </HelpTip>
              <input
                inputMode="decimal"
                value={draft.businessProfitSek}
                onChange={(event) => updateDraft("businessProfitSek", event.target.value)}
              />
            </label>
          </section>
        ) : null}

        {step === "vat" ? (
          <section className="panel onboarding-panel">
            <label>
              {t(locale, "onboarding.vat.status")}
              <HelpTip label={t(locale, "onboarding.vat.status")}>
                {t(locale, "onboarding.vat.statusHelp")}
              </HelpTip>
              <select
                value={draft.vatStatus}
                onChange={(event) =>
                  updateDraft("vatStatus", event.target.value as OnboardingDraft["vatStatus"])
                }
              >
                <option value="exempt_low_turnover">{t(locale, "onboarding.vat.exempt")}</option>
                <option value="registered">{t(locale, "onboarding.vat.registered")}</option>
              </select>
            </label>
            <label>
              {t(locale, "onboarding.vat.period")}
              <select
                value={draft.reportingPeriod}
                onChange={(event) =>
                  updateDraft(
                    "reportingPeriod",
                    event.target.value as OnboardingDraft["reportingPeriod"],
                  )
                }
              >
                <option value="monthly">{t(locale, "onboarding.vat.monthly")}</option>
                <option value="quarterly">{t(locale, "onboarding.vat.quarterly")}</option>
                <option value="yearly">{t(locale, "onboarding.vat.yearly")}</option>
              </select>
            </label>
            <label>
              {t(locale, "onboarding.vat.method")}
              <select
                value={draft.accountingMethod}
                onChange={(event) =>
                  updateDraft(
                    "accountingMethod",
                    event.target.value as OnboardingDraft["accountingMethod"],
                  )
                }
              >
                <option value="invoice_method">{t(locale, "onboarding.vat.invoiceMethod")}</option>
                <option value="cash_method">{t(locale, "onboarding.vat.cashMethod")}</option>
              </select>
            </label>
          </section>
        ) : null}

        {step === "review" ? (
          <>
            <section className="panel onboarding-panel">
              <p>{t(locale, "onboarding.review.summary")}</p>
              <dl className="review-summary">
                <div>
                  <dt>{t(locale, "onboarding.business.name")}</dt>
                  <dd>{draft.businessName}</dd>
                </div>
                <div>
                  <dt>{t(locale, "onboarding.tax.status")}</dt>
                  <dd>
                    {draft.taxStatus === "fa_skatt"
                      ? t(locale, "onboarding.tax.faSkatt")
                      : t(locale, "onboarding.tax.fSkatt")}
                  </dd>
                </div>
                <div>
                  <dt>{t(locale, "onboarding.vat.status")}</dt>
                  <dd>
                    {draft.vatStatus === "registered"
                      ? t(locale, "onboarding.vat.registered")
                      : t(locale, "onboarding.vat.exempt")}
                  </dd>
                </div>
              </dl>
              {ruleVersion ? (
                <p className="muted">
                  {t(locale, "onboarding.review.rules")}: {ruleVersion.taxYear} ·{" "}
                  <a href={ruleVersion.sourceUrl} target="_blank" rel="noreferrer">
                    {t(locale, "onboarding.review.source")}
                  </a>
                </p>
              ) : null}
            </section>

            {complianceMessages.length > 0 ? (
              <section className="panel compliance-errors" aria-live="polite">
                <h3>{t(locale, "onboarding.compliance.title")}</h3>
                <ul>
                  {complianceMessages.map((key) => (
                    <li key={key}>{t(locale, key)}</li>
                  ))}
                </ul>
              </section>
            ) : null}
          </>
        ) : null}

        <div className="form-row">
          {previousOnboardingStep(step) ? (
            <button type="button" className="secondary" onClick={handleBack} disabled={busy}>
              {t(locale, "onboarding.action.back")}
            </button>
          ) : null}
          {step !== "review" ? (
            <button type="button" onClick={handleNext} disabled={!canAdvance || busy}>
              {t(locale, "onboarding.action.next")}
            </button>
          ) : (
            <button type="button" onClick={() => void handleSave()} disabled={busy}>
              {busy ? t(locale, "onboarding.action.saving") : t(locale, "onboarding.action.save")}
            </button>
          )}
        </div>
      </section>
    </main>
  )
}
