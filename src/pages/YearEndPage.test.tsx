import { render, screen, waitFor } from "@testing-library/react"
import { MemoryRouter } from "react-router-dom"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import type { YearEndPackageSummary, YearEndReadiness } from "../lib/commands"
import { YearEndPage } from "./YearEndPage"

const workspace = {
  id: "ws-1",
  name: "Testfirma",
  dataDir: "/tmp/data",
  databasePath: "/tmp/workspace.sqlite",
}

const draftPackage: YearEndPackageSummary = {
  id: "pkg-1",
  fiscalYearId: "fy-2026",
  fiscalYear: 2026,
  status: "draft",
  ruleVersionId: "rv-1",
  k1Allowed: true,
  neDraftPresent: true,
  storedLocally: true,
  exportPath: null,
  fiscalYearLocked: false,
  neFields: [],
}

const {
  taxProfileGetCurrent,
  workspaceSettingsGet,
  yearEndPackageFindByFiscalYear,
  yearEndReadinessGet,
} = vi.hoisted(() => ({
  taxProfileGetCurrent: vi.fn(),
  workspaceSettingsGet: vi.fn(),
  yearEndPackageFindByFiscalYear: vi.fn(),
  yearEndReadinessGet: vi.fn(),
}))

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
  taxProfileGetCurrent,
  workspaceSettingsGet,
  yearEndPackageApprove: vi.fn(),
  yearEndPackageCreate: vi.fn(),
  yearEndPackageExport: vi.fn(),
  yearEndPackageFindByFiscalYear,
  yearEndPackageGet: vi.fn(),
  yearEndReadinessGet,
}))

function renderYearEnd() {
  return render(
    <MemoryRouter>
      <WorkspaceProvider>
        <LocaleProvider initialLocale="sv">
          <YearEndPage />
        </LocaleProvider>
      </WorkspaceProvider>
    </MemoryRouter>,
  )
}

describe("YearEndPage", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    taxProfileGetCurrent.mockResolvedValue({ taxStatus: "f_skatt", activeRuleYear: 2026 })
    workspaceSettingsGet.mockResolvedValue({ defaultExportDirectory: null })
    yearEndPackageFindByFiscalYear.mockResolvedValue(draftPackage)
  })

  it("disables approve when readiness is not satisfied", async () => {
    const blockedReadiness: YearEndReadiness = {
      readyToApprove: false,
      items: [{ code: "vat_periods_filed", satisfied: false, detail: "Open VAT period" }],
    }
    yearEndReadinessGet.mockResolvedValue(blockedReadiness)

    renderYearEnd()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Godkänn och lås år" })).toBeDisabled()
    })
  })

  it("enables approve when package is draft and readiness passes", async () => {
    yearEndReadinessGet.mockResolvedValue({
      readyToApprove: true,
      items: [{ code: "vat_periods_filed", satisfied: true, detail: null }],
    })

    renderYearEnd()

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Godkänn och lås år" })).toBeEnabled()
    })
  })
})
