import { useEffect, useRef } from "react"
import { useWorkspace } from "./WorkspaceContext"
import { localeFromSettings, useLocale } from "./LocaleContext"
import { workspaceSettingsGet } from "../lib/commands"

export function WorkspaceLocaleHydrator() {
  const { workspace } = useWorkspace()
  const { setLocale } = useLocale()
  const workspaceRef = useRef(workspace)
  workspaceRef.current = workspace

  useEffect(() => {
    if (!workspace) {
      setLocale("en")
      return
    }

    const workspaceId = workspace.id
    let active = true

    workspaceSettingsGet()
      .then((settings) => {
        if (!active || workspaceRef.current?.id !== workspaceId) {
          return
        }
        setLocale(localeFromSettings(settings.locale))
      })
      .catch(() => {
        if (!active || workspaceRef.current?.id !== workspaceId) {
          return
        }
        setLocale("en")
      })

    return () => {
      active = false
    }
  }, [workspace, setLocale])

  return null
}
