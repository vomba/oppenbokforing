import { createContext, useContext, useMemo, useState, type ReactNode } from "react"
import type { WorkspaceSummary } from "../lib/commands"

type WorkspaceContextValue = {
  workspace: WorkspaceSummary | null
  setWorkspace: (workspace: WorkspaceSummary | null) => void
}

const WorkspaceContext = createContext<WorkspaceContextValue | null>(null)

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [workspace, setWorkspace] = useState<WorkspaceSummary | null>(null)
  const value = useMemo(() => ({ workspace, setWorkspace }), [workspace])
  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>
}

export function useWorkspace() {
  const context = useContext(WorkspaceContext)
  if (!context) {
    throw new Error("useWorkspace must be used within WorkspaceProvider")
  }
  return context
}
