import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { MemoryRouter } from "react-router-dom"
import { describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { SimpleModeProvider } from "../context/SimpleModeContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import { workspaceSettingsSave } from "../lib/commands"
import { SettingsPage } from "./SettingsPage"

const workspace = {
  id: "ws-1",
  name: "Testfirma",
  dataDir: "/tmp/data",
  databasePath: "/tmp/workspace.sqlite",
}

vi.mock("../context/WorkspaceContext", async () => {
  const actual = await vi.importActual<typeof import("../context/WorkspaceContext")>(
    "../context/WorkspaceContext",
  )
  return {
    ...actual,
    useWorkspace: () => ({
      workspace,
      setWorkspace: vi.fn(),
    }),
  }
})

vi.mock("../components/AppSidebar", () => ({
  AppSidebar: () => <nav aria-label="sidebar" />,
}))

vi.mock("../lib/commands", () => ({
  appErrorMessage: (_error: unknown, fallback: string) => fallback,
  accountantPackageExportCreate: vi.fn(),
  accountantPackageImportValidate: vi.fn(),
  integrationStatusGet: vi.fn().mockResolvedValue(null),
  sieExportCreate: vi.fn(),
  taxProfileGetCurrent: vi.fn().mockResolvedValue({ activeRuleYear: 2026 }),
  workspaceSettingsGet: vi.fn().mockResolvedValue({
    id: "settings-1",
    locale: "sv",
    updaterEnabled: false,
    defaultExportDirectory: null,
    defaultBackupDirectory: null,
    dashboardTourCompleted: true,
    simpleMode: false,
  }),
  workspaceSettingsSave: vi.fn().mockImplementation(async (input) => ({
    id: "settings-1",
    locale: "sv",
    updaterEnabled: false,
    defaultExportDirectory: null,
    defaultBackupDirectory: null,
    dashboardTourCompleted: true,
    simpleMode: input.simpleMode ?? false,
  })),
}))

vi.mock("../lib/dialogs", () => ({
  fileNameFromPath: (path: string) => path,
  pickDirectory: vi.fn(),
  pickOpenFile: vi.fn(),
}))

function renderSettings() {
  return render(
    <MemoryRouter>
      <WorkspaceProvider>
        <LocaleProvider initialLocale="sv">
          <SimpleModeProvider>
            <SettingsPage />
          </SimpleModeProvider>
        </LocaleProvider>
      </WorkspaceProvider>
    </MemoryRouter>,
  )
}

describe("SettingsPage", () => {
  it("shows workspace database and data directory paths when advanced mode is on", async () => {
    renderSettings()
    await waitFor(() => {
      expect(screen.getByText("/tmp/workspace.sqlite")).toBeInTheDocument()
      expect(screen.getByText("/tmp/data")).toBeInTheDocument()
    })
  })

  it("persists simple mode when the toggle changes", async () => {
    renderSettings()

    await waitFor(() => {
      expect(screen.getByRole("checkbox", { name: "Behåll enkelt läge" })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole("checkbox", { name: "Behåll enkelt läge" }))

    await waitFor(() => {
      expect(workspaceSettingsSave).toHaveBeenCalledWith(
        expect.objectContaining({ simpleMode: true }),
      )
    })
  })
})
