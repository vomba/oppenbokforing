import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react"
import { workspaceSettingsGet } from "../lib/commands"
import { useWorkspace } from "./WorkspaceContext"

type SimpleModeContextValue = {
  simpleMode: boolean
  setSimpleMode: (value: boolean) => void
  refreshSimpleMode: () => Promise<void>
}

const SimpleModeContext = createContext<SimpleModeContextValue | null>(null)

export function SimpleModeProvider({ children }: { children: ReactNode }) {
  const { workspace } = useWorkspace()
  const [simpleMode, setSimpleMode] = useState(true)

  const refreshSimpleMode = useCallback(async () => {
    if (!workspace) {
      setSimpleMode(true)
      return
    }
    try {
      const settings = await workspaceSettingsGet()
      setSimpleMode(settings.simpleMode)
    } catch {
      setSimpleMode(true)
    }
  }, [workspace])

  useEffect(() => {
    void refreshSimpleMode()
  }, [refreshSimpleMode])

  const value = useMemo(
    () => ({ simpleMode, setSimpleMode, refreshSimpleMode }),
    [simpleMode, refreshSimpleMode],
  )

  return (
    <SimpleModeContext.Provider value={value}>{children}</SimpleModeContext.Provider>
  )
}

export function useSimpleMode() {
  const context = useContext(SimpleModeContext)
  if (!context) {
    throw new Error("useSimpleMode must be used within SimpleModeProvider")
  }
  return context
}
