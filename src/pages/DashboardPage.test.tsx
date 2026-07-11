import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { MemoryRouter } from "react-router-dom"
import { describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { SimpleModeProvider } from "../context/SimpleModeContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import { dashboardTourMarkComplete, workspaceSettingsGet } from "../lib/commands"
import { DashboardPage } from "./DashboardPage"

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
  cashflowOverviewGet: vi.fn().mockResolvedValue({ spendableCashMinor: 250000 }),
  complianceProfileCheck: vi.fn().mockResolvedValue({
    scenarioIds: ["vat-exempt-below-threshold"],
    passed: true,
    outcomes: {},
    ruleYear: 2026,
  }),
  invoiceOpenCount: vi.fn().mockResolvedValue(2),
  ruleVersionGet: vi.fn().mockResolvedValue({
    taxYear: 2026,
    sourceUrl: "https://example.com/rules",
  }),
  stagedTransactionsCount: vi.fn().mockResolvedValue(3),
  taxProfileGetCurrent: vi.fn().mockResolvedValue({ taxStatus: "f_skatt" }),
  vatProfileGetCurrent: vi.fn().mockResolvedValue({ vatStatus: "exempt_low_turnover" }),
  vatThresholdStatusGet: vi.fn().mockResolvedValue({ warning: "none" }),
  workspaceSettingsGet: vi.fn().mockResolvedValue({
    id: "settings-1",
    locale: "sv",
    updaterEnabled: false,
    defaultExportDirectory: null,
    defaultBackupDirectory: null,
    dashboardTourCompleted: true,
    simpleMode: true,
  }),
  dashboardTourMarkComplete: vi.fn().mockResolvedValue({
    dashboardTourCompleted: true,
  }),
  yearEndReadinessGet: vi.fn().mockResolvedValue({
    readyToApprove: true,
    items: [],
  }),
  workspaceBackupCreate: vi.fn(),
  workspaceClose: vi.fn(),
}))

vi.mock("../lib/dialogs", () => ({
  pickSaveBackupFile: vi.fn(),
}))

function renderDashboard() {
  return render(
    <MemoryRouter>
      <WorkspaceProvider>
        <LocaleProvider initialLocale="sv">
          <SimpleModeProvider>
            <DashboardPage />
          </SimpleModeProvider>
        </LocaleProvider>
      </WorkspaceProvider>
    </MemoryRouter>,
  )
}

describe("DashboardPage", () => {
  it("renders checklist counts for staged imports and open invoices", async () => {
    renderDashboard()

    await waitFor(() => {
      expect(screen.getByText("3 omatchade rader under Dokument")).toBeInTheDocument()
      expect(screen.getByText("2 fakturor väntar på betalning")).toBeInTheDocument()
    })
  })

  it("persists tour completion when the user skips the guided tour", async () => {
    vi.mocked(workspaceSettingsGet).mockResolvedValueOnce({
      id: "settings-1",
      locale: "sv",
      updaterEnabled: false,
      defaultExportDirectory: null,
      defaultBackupDirectory: null,
      dashboardTourCompleted: false,
      simpleMode: true,
    })

    renderDashboard()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Hoppa över rundtur" })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole("button", { name: "Hoppa över rundtur" }))

    await waitFor(() => {
      expect(dashboardTourMarkComplete).toHaveBeenCalledTimes(1)
    })
  })
})
