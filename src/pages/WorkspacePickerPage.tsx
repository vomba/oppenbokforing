import { Link, useNavigate } from "react-router-dom"
import { useEffect, useState } from "react"
import { useLocale } from "../context/LocaleContext"
import { t, tVars } from "../i18n"
import {
  appErrorMessage,
  recentWorkspacesList,
  workspaceBackupRestore,
  workspaceCreate,
  workspaceOpen,
  type RecentWorkspaceEntry,
} from "../lib/commands"
import { pickBackupFile } from "../lib/dialogs"
import { useWorkspace } from "../context/WorkspaceContext"

export function WorkspacePickerPage() {
  const navigate = useNavigate()
  const { locale } = useLocale()
  const { setWorkspace } = useWorkspace()
  const [workspaceName, setWorkspaceName] = useState(t(locale, "workspace.defaultName"))
  const [recent, setRecent] = useState<RecentWorkspaceEntry[]>([])
  const [backupPath, setBackupPath] = useState("")
  const [restorePassphrase, setRestorePassphrase] = useState("")
  const [confirmRestore, setConfirmRestore] = useState(false)
  const [status, setStatus] = useState(t(locale, "workspace.pickerStatus"))
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    setStatus(t(locale, "workspace.pickerStatus"))
    setWorkspaceName(t(locale, "workspace.defaultName"))
  }, [locale])

  useEffect(() => {
    recentWorkspacesList()
      .then(setRecent)
      .catch(() => setRecent([]))
  }, [])

  async function handleCreate() {
    if (busy) return
    setBusy(true)
    setStatus(t(locale, "workspace.creating"))
    try {
      const created = await workspaceCreate({ name: workspaceName })
      setWorkspace(created)
      setStatus(tVars(locale, "workspace.ready", { name: created.name }))
      navigate("/onboarding")
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "workspace.createFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handleOpen(databasePath: string) {
    if (busy) return
    setBusy(true)
    setStatus(t(locale, "workspace.opening"))
    try {
      const opened = await workspaceOpen({ databasePath })
      setWorkspace(opened)
      setStatus(tVars(locale, "workspace.opened", { name: opened.name }))
      navigate("/dashboard")
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "workspace.openFailed")))
    } finally {
      setBusy(false)
    }
  }

  async function handlePickBackup() {
    if (busy) return
    const selected = await pickBackupFile(t(locale, "workspace.pickRestoreFile"))
    if (selected) {
      setBackupPath(selected)
    }
  }

  async function handleRestore() {
    if (busy || !backupPath.trim()) return
    if (!confirmRestore) {
      setStatus(t(locale, "workspace.restoreConfirmRequired"))
      return
    }
    setBusy(true)
    setStatus(t(locale, "workspace.restoring"))
    try {
      const restored = await workspaceBackupRestore({
        backupPath: backupPath.trim(),
        confirmOverwrite: true,
        passphrase: restorePassphrase,
      })
      const opened = await workspaceOpen({ databasePath: restored.databasePath })
      setWorkspace(opened)
      setStatus(tVars(locale, "workspace.restored", { name: opened.name }))
      navigate("/dashboard")
    } catch (error) {
      setStatus(appErrorMessage(error, t(locale, "workspace.restoreFailed")))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="picker-shell">
      <section className="picker-panel">
        <p className="eyebrow">{t(locale, "app.title")}</p>
        <h1>{t(locale, "workspace.pickerTitle")}</h1>
        <p className="status-line">{status}</p>

        <div className="picker-block">
          <h2>{t(locale, "workspace.createTitle")}</h2>
          <div className="form-row">
            <input
              aria-label={t(locale, "workspace.nameLabel")}
              value={workspaceName}
              onChange={(event) => setWorkspaceName(event.target.value)}
              disabled={busy}
            />
            <button type="button" onClick={handleCreate} disabled={busy}>
              {t(locale, "workspace.createAction")}
            </button>
          </div>
          <p className="muted">{t(locale, "workspace.nameHelp")}</p>
        </div>

        <div className="picker-block">
          <h2>{t(locale, "workspace.recentTitle")}</h2>
          {recent.length === 0 ? (
            <p className="muted">{t(locale, "workspace.recentEmpty")}</p>
          ) : (
            <ul className="recent-list">
              {recent.map((entry) => (
                <li key={entry.databasePath}>
                  <div>
                    <strong>{entry.name}</strong>
                    <span>{entry.databasePath}</span>
                  </div>
                  <button type="button" onClick={() => handleOpen(entry.databasePath)} disabled={busy}>
                    {t(locale, "workspace.openAction")}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="picker-block">
          <h2>{t(locale, "workspace.restoreTitle")}</h2>
          <div className="form-row">
            <input
              aria-label={t(locale, "workspace.backupPathLabel")}
              placeholder={t(locale, "workspace.backupPathPlaceholder")}
              value={backupPath}
              readOnly
              disabled={busy}
              title={backupPath || undefined}
            />
            <button type="button" onClick={() => void handlePickBackup()} disabled={busy}>
              {t(locale, "workspace.chooseBackup")}
            </button>
            <input
              aria-label={t(locale, "workspace.passphraseLabel")}
              type="password"
              placeholder={t(locale, "workspace.passphrasePlaceholder")}
              value={restorePassphrase}
              onChange={(event) => setRestorePassphrase(event.target.value)}
              disabled={busy}
            />
          </div>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={confirmRestore}
              onChange={(event) => setConfirmRestore(event.target.checked)}
              disabled={busy}
            />
            {t(locale, "workspace.restoreConfirmLabel")}
          </label>
          <button type="button" className="secondary" onClick={handleRestore} disabled={busy}>
            {t(locale, "workspace.restoreAction")}
          </button>
        </div>

        <p className="muted">
          {t(locale, "workspace.alreadyOnboarded")}{" "}
          <Link to="/dashboard">{t(locale, "workspace.goDashboard")}</Link>
        </p>
      </section>
    </main>
  )
}
