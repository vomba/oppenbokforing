import { useEffect, useRef, useState } from "react"
import { useWorkspace } from "../context/WorkspaceContext"
import { useLocale } from "../context/LocaleContext"
import { useSimpleMode } from "../context/SimpleModeContext"
import { AppSidebar } from "../components/AppSidebar"
import { t } from "../i18n"
import {
  accountantPackageExportCreate,
  accountantPackageImportValidate,
  appErrorMessage,
  integrationStatusGet,
  sieExportCreate,
  taxProfileGetCurrent,
  workspaceSettingsGet,
  workspaceSettingsSave,
  type IntegrationStatusResponse,
  type WorkspaceSettings,
} from "../lib/commands"
import { fileNameFromPath, pickDirectory, pickOpenFile } from "../lib/dialogs"
import { resolveExportDirectory } from "../lib/exportDirectory"

export function SettingsPage() {
  const { workspace } = useWorkspace()
  const { locale, setLocale } = useLocale()
  const { simpleMode, setSimpleMode } = useSimpleMode()
  const [status, setStatus] = useState("")
  const [busy, setBusy] = useState(false)
  const [packagePath, setPackagePath] = useState("")
  const [fiscalYear, setFiscalYear] = useState(2026)
  const [integrations, setIntegrations] = useState<IntegrationStatusResponse | null>(null)
  const [settings, setSettings] = useState<WorkspaceSettings | null>(null)
  const exportKey = useRef<string | null>(null)

  useEffect(() => {
    if (!workspace) return
    taxProfileGetCurrent()
      .then((profile) => setFiscalYear(profile.activeRuleYear))
      .catch(() => setFiscalYear(2026))
    integrationStatusGet()
      .then(setIntegrations)
      .catch(() => setIntegrations(null))
    workspaceSettingsGet()
      .then(setSettings)
      .catch(() => setSettings(null))
  }, [workspace])

  async function chooseExportDirectory() {
    return resolveExportDirectory(
      t(locale, "settings.defaultExportDirectory"),
      settings?.defaultExportDirectory,
    )
  }

  async function handleLocaleChange(nextLocale: "en" | "sv") {
    if (busy) return
    setBusy(true)
    try {
      const saved = await workspaceSettingsSave({
        locale: nextLocale,
        updaterEnabled: null,
        defaultExportDirectory: settings?.defaultExportDirectory ?? null,
        defaultBackupDirectory: settings?.defaultBackupDirectory ?? null,
        simpleMode: null,
      })
      setSettings(saved)
      setLocale(nextLocale)
      setStatus(t(nextLocale, "settings.saved"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.saveFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleSimpleModeChange(enabled: boolean) {
    if (busy || !settings) return
    setBusy(true)
    try {
      const saved = await workspaceSettingsSave({
        locale: settings.locale,
        updaterEnabled: null,
        defaultExportDirectory: settings.defaultExportDirectory,
        defaultBackupDirectory: settings.defaultBackupDirectory,
        simpleMode: enabled,
      })
      setSettings(saved)
      setSimpleMode(saved.simpleMode)
      setStatus(t(locale, "settings.saved"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.saveFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleDefaultExportDirectoryPick() {
    if (busy) return
    const picked = await pickDirectory(
      t(locale, "settings.defaultExportDirectory"),
      settings?.defaultExportDirectory,
    )
    if (!picked) return
    setBusy(true)
    try {
      const saved = await workspaceSettingsSave({
        locale: settings?.locale ?? locale,
        updaterEnabled: null,
        defaultExportDirectory: picked,
        defaultBackupDirectory: settings?.defaultBackupDirectory ?? null,
        simpleMode: null,
      })
      setSettings(saved)
      setStatus(t(locale, "settings.saved"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.saveFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleDefaultBackupDirectoryPick() {
    if (busy) return
    const picked = await pickDirectory(
      t(locale, "settings.defaultBackupDirectory"),
      settings?.defaultBackupDirectory,
    )
    if (!picked) return
    setBusy(true)
    try {
      const saved = await workspaceSettingsSave({
        locale: settings?.locale ?? locale,
        updaterEnabled: null,
        defaultExportDirectory: settings?.defaultExportDirectory ?? null,
        defaultBackupDirectory: picked,
        simpleMode: null,
      })
      setSettings(saved)
      setStatus(t(locale, "settings.saved"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.saveFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleClearDefaultExportDirectory() {
    if (busy || !settings) return
    setBusy(true)
    try {
      const saved = await workspaceSettingsSave({
        locale: settings.locale,
        updaterEnabled: null,
        defaultExportDirectory: "",
        defaultBackupDirectory: settings.defaultBackupDirectory,
        simpleMode: null,
      })
      setSettings(saved)
      setStatus(t(locale, "settings.saved"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.saveFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleClearDefaultBackupDirectory() {
    if (busy || !settings) return
    setBusy(true)
    try {
      const saved = await workspaceSettingsSave({
        locale: settings.locale,
        updaterEnabled: null,
        defaultExportDirectory: settings.defaultExportDirectory,
        defaultBackupDirectory: "",
        simpleMode: null,
      })
      setSettings(saved)
      setStatus(t(locale, "settings.saved"))
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.saveFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleSieExport() {
    if (busy) return
    setBusy(true)
    try {
      const exportDirectory = await chooseExportDirectory()
      if (!exportDirectory) {
        setStatus(t(locale, "settings.exportCancelled"))
        return
      }
      const idempotencyKey = exportKey.current ?? crypto.randomUUID()
      exportKey.current = idempotencyKey
      const result = await sieExportCreate({ fiscalYear, idempotencyKey, exportDirectory })
      exportKey.current = null
      setStatus(`${t(locale, "settings.exportDone")}: ${result.exportPath}`)
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.sieExportFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleAccountantExport() {
    if (busy) return
    setBusy(true)
    try {
      const exportDirectory = await chooseExportDirectory()
      if (!exportDirectory) {
        setStatus(t(locale, "settings.exportCancelled"))
        return
      }
      const result = await accountantPackageExportCreate({
        fiscalYear,
        idempotencyKey: crypto.randomUUID(),
        exportDirectory,
      })
      setPackagePath(result.packagePath)
      setStatus(`${t(locale, "settings.exportDone")}: ${result.packagePath}`)
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.accountantExportFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleBrowsePackage() {
    if (busy) return
    setBusy(true)
    try {
      const selected = await pickOpenFile(t(locale, "settings.browsePackage"), {
        defaultPath: packagePath || settings?.defaultExportDirectory,
        filters: [{ name: "JSON", extensions: ["json"] }],
      })
      if (selected) {
        setPackagePath(selected)
      }
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.validationFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleValidatePackage() {
    if (busy || !packagePath.trim()) return
    setBusy(true)
    try {
      const result = await accountantPackageImportValidate({ packagePath: packagePath.trim() })
      setStatus(
        `${t(locale, "settings.validateDone")}: ${
          result.valid ? t(locale, "settings.validationOk") : result.manualFallbackHint
        }`,
      )
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "settings.validationFailed")))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="app-shell">
      <AppSidebar current="settings" />

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{workspace?.name ?? "—"}</p>
            <h2>{t(locale, "settings.title")}</h2>
          </div>
        </header>

        <section className="workbench">
          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "settings.locale")}</p>
              <h3>{t(locale, "settings.locale")}</h3>
            </header>
            <div className="button-row">
              <button
                type="button"
                disabled={busy || locale === "en"}
                onClick={() => void handleLocaleChange("en")}
              >
                {t(locale, "settings.locale.en")}
              </button>
              <button
                type="button"
                disabled={busy || locale === "sv"}
                onClick={() => void handleLocaleChange("sv")}
              >
                {t(locale, "settings.locale.sv")}
              </button>
            </div>
          </div>

          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "settings.simpleMode")}</p>
              <h3>{t(locale, "settings.simpleMode")}</h3>
            </header>
            <p className="muted">{t(locale, "settings.simpleModeHint")}</p>
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={simpleMode}
                onChange={(event) => void handleSimpleModeChange(event.target.checked)}
                disabled={busy}
              />
              {t(locale, "settings.simpleModeEnabled")}
            </label>
          </div>

          {!simpleMode ? (
            <>
          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "settings.workspacePaths")}</p>
              <h3>{t(locale, "settings.workspacePaths")}</h3>
            </header>
            <dl className="review-summary">
              <div>
                <dt>{t(locale, "settings.databasePath")}</dt>
                <dd>{workspace?.databasePath ?? "—"}</dd>
              </div>
              <div>
                <dt>{t(locale, "settings.dataDirectory")}</dt>
                <dd>{workspace?.dataDir ?? "—"}</dd>
              </div>
            </dl>
          </div>

          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "settings.paths")}</p>
              <h3>{t(locale, "settings.paths")}</h3>
            </header>
            <label>
              {t(locale, "settings.defaultExportDirectory")}
              <input
                id="settings-default-export-directory"
                readOnly
                value={settings?.defaultExportDirectory ?? ""}
                placeholder={t(locale, "settings.noFolderSelected")}
              />
            </label>
            <div className="form-row">
              <button type="button" disabled={busy} onClick={() => void handleDefaultExportDirectoryPick()}>
                {t(locale, "settings.chooseFolder")}
              </button>
              <button
                type="button"
                className="secondary"
                disabled={busy || !settings?.defaultExportDirectory}
                onClick={() => void handleClearDefaultExportDirectory()}
              >
                {t(locale, "settings.clearFolder")}
              </button>
            </div>
            <label>
              {t(locale, "settings.defaultBackupDirectory")}
              <input
                id="settings-default-backup-directory"
                readOnly
                value={settings?.defaultBackupDirectory ?? ""}
                placeholder={t(locale, "settings.noBackupFolderSelected")}
              />
            </label>
            <div className="form-row">
              <button type="button" disabled={busy} onClick={() => void handleDefaultBackupDirectoryPick()}>
                {t(locale, "settings.chooseFolder")}
              </button>
              <button
                type="button"
                className="secondary"
                disabled={busy || !settings?.defaultBackupDirectory}
                onClick={() => void handleClearDefaultBackupDirectory()}
              >
                {t(locale, "settings.clearFolder")}
              </button>
            </div>
          </div>

          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "settings.exports")}</p>
              <h3>{t(locale, "settings.exports")}</h3>
            </header>
            <p className="muted">
              {t(locale, "settings.fiscalYear")}: {fiscalYear}
            </p>
            <div className="button-row">
              <button type="button" disabled={busy} onClick={() => void handleSieExport()}>
                {t(locale, "settings.sieExport")}
              </button>
              <button type="button" disabled={busy} onClick={() => void handleAccountantExport()}>
                {t(locale, "settings.accountantPackage")}
              </button>
            </div>
            <label htmlFor="settings-package-path">{t(locale, "settings.packagePath")}</label>
            <div className="form-row">
              <input
                id="settings-package-path"
                readOnly
                value={packagePath ? fileNameFromPath(packagePath) : ""}
                placeholder="—"
                title={packagePath || undefined}
              />
              <button type="button" disabled={busy} onClick={() => void handleBrowsePackage()}>
                {t(locale, "settings.browsePackage")}
              </button>
            </div>
            <button type="button" disabled={busy || !packagePath.trim()} onClick={() => void handleValidatePackage()}>
              {t(locale, "settings.validatePackage")}
            </button>
          </div>

          <div className="panel">
            <header>
              <p className="eyebrow">{t(locale, "settings.integrations")}</p>
              <h3>{t(locale, "settings.integrations")}</h3>
            </header>
            <p className="muted">{t(locale, "settings.integrationsHint")}</p>
            {integrations ? (
              <ul>
                <li>Open Banking: {integrations.openBanking.manualFallbackHint}</li>
                <li>BankID: {integrations.bankid.manualFallbackHint}</li>
              </ul>
            ) : null}
          </div>
            </>
          ) : null}
        </section>

        {status ? (
          <p className="status-line" role="status" aria-live="polite">
            {t(locale, "settings.status")}: {status}
          </p>
        ) : null}
      </section>
    </main>
  )
}
