import { render, screen, waitFor } from "@testing-library/react"
import { MemoryRouter, Route, Routes } from "react-router-dom"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { LocaleProvider, useLocale } from "./LocaleContext"
import { WorkspaceProvider, useWorkspace } from "./WorkspaceContext"
import { WorkspaceLocaleHydrator } from "./WorkspaceLocaleHydrator"
import { WorkspacePickerPage } from "../pages/WorkspacePickerPage"
import { recentWorkspacesList, workspaceSettingsGet } from "../lib/commands"
import { useEffect } from "react"

vi.mock("../lib/commands", () => ({
  appErrorMessage: (_error: unknown, fallback: string) => fallback,
  recentWorkspacesList: vi.fn(),
  workspaceBackupRestore: vi.fn(),
  workspaceCreate: vi.fn(),
  workspaceOpen: vi.fn(),
  workspaceSettingsGet: vi.fn(),
}))

const workspace = {
  id: "ws-1",
  name: "Testfirma",
  dataDir: "/tmp/data",
  databasePath: "/tmp/workspace.sqlite",
}
const englishSettings = {
  id: "settings-1",
  locale: "en",
  updaterEnabled: false,
  defaultExportDirectory: null,
  defaultBackupDirectory: null,
  dashboardTourCompleted: false,
  simpleMode: false,
}

function LocaleProbe() {
  const { locale } = useLocale()
  return <output data-testid="locale">{locale}</output>
}

function WorkspaceSelection() {
  const { setWorkspace } = useWorkspace()

  useEffect(() => {
    setWorkspace(workspace)
  }, [setWorkspace])

  return null
}

function renderPicker() {
  return render(
    <WorkspaceProvider>
      <LocaleProvider>
        <WorkspaceLocaleHydrator />
        <MemoryRouter initialEntries={["/"]}>
          <Routes>
            <Route path="/" element={<WorkspacePickerPage />} />
          </Routes>
        </MemoryRouter>
      </LocaleProvider>
    </WorkspaceProvider>,
  )
}

function renderHydratorWithWorkspace(initialLocale: "sv" | "en" = "sv") {
  return render(
    <WorkspaceProvider>
      <LocaleProvider initialLocale={initialLocale}>
        <WorkspaceLocaleHydrator />
        <WorkspaceSelection />
        <LocaleProbe />
      </LocaleProvider>
    </WorkspaceProvider>,
  )
}

describe("WorkspaceLocaleHydrator", () => {
  beforeEach(() => {
    vi.mocked(recentWorkspacesList).mockResolvedValue([])
    vi.mocked(workspaceSettingsGet).mockReset()
  })

  it("keeps the first-run workspace picker in Swedish without a workspace", async () => {
    renderPicker()

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Välj arbetsyta" })).toBeInTheDocument()
    })
    expect(screen.getByRole("heading", { name: "Skapa arbetsyta" })).toBeInTheDocument()
    expect(screen.queryByRole("heading", { name: "Workspace picker" })).not.toBeInTheDocument()
  })

  it("falls back to Swedish when workspace settings cannot be loaded", async () => {
    vi.mocked(workspaceSettingsGet).mockRejectedValue(new Error("settings unavailable"))
    renderHydratorWithWorkspace("en")

    await waitFor(() => {
      expect(workspaceSettingsGet).toHaveBeenCalledTimes(1)
      expect(screen.getByTestId("locale")).toHaveTextContent("sv")
    })
  })

  it("applies an explicit English saved workspace setting", async () => {
    vi.mocked(workspaceSettingsGet).mockResolvedValue(englishSettings)
    renderHydratorWithWorkspace()

    await waitFor(() => {
      expect(screen.getByTestId("locale")).toHaveTextContent("en")
    })
  })
})
