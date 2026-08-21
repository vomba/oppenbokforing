import type * as WorkspaceContextModule from "../context/WorkspaceContext"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { MemoryRouter } from "react-router-dom"
import { describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import { VatPage } from "./VatPage"

const { vatReturnDraftCreate, vatReturnApprove } = vi.hoisted(() => ({
  vatReturnDraftCreate: vi.fn(),
  vatReturnApprove: vi.fn(),
}))

vi.mock("../context/WorkspaceContext", async () => {
  const actual = await vi.importActual<typeof WorkspaceContextModule>(
    "../context/WorkspaceContext",
  )
  return {
    ...actual,
    useWorkspace: () => ({
      workspace: { id: "ws-1", name: "Testfirma", dataDir: "/tmp/data", databasePath: "/tmp/ws.sqlite" },
      setWorkspace: vi.fn(),
    }),
  }
})

vi.mock("../components/AppSidebar", () => ({ AppSidebar: () => <nav aria-label="sidebar" /> }))
vi.mock("../lib/commands", () => ({
  appErrorMessage: (_error: unknown, fallback: string) => fallback,
  cashflowOverviewGet: vi.fn().mockResolvedValue(null),
  taxProfileGetCurrent: vi.fn().mockResolvedValue({ activeRuleYear: 2026 }),
  vatProfileGetCurrent: vi.fn().mockResolvedValue({ vatStatus: "registered", reportingPeriod: "quarterly" }),
  vatReturnApprove,
  vatReturnDraftCreate,
  vatReturnExport: vi.fn(),
  vatReturnTrace: vi.fn().mockResolvedValue({ boxes: [] }),
  vatThresholdStatusGet: vi.fn().mockResolvedValue(null),
  workspaceSettingsGet: vi.fn().mockResolvedValue({ defaultExportDirectory: null }),
}))

describe("VatPage", () => {
  it("reviews VAT approval before its idempotent mutation", async () => {
    const vatReturn = {
      id: "vat-1",
      periodKey: "2026-Q1",
      status: "approved",
      box49AmountMinor: 250000,
      zeroReturn: false,
      boxes: [],
    }
    vatReturnDraftCreate.mockImplementation(({ periodKey }: { periodKey: string }) =>
      Promise.resolve({ ...vatReturn, periodKey, status: "draft" }),
    )
    vatReturnApprove.mockResolvedValue(vatReturn)

    render(
      <MemoryRouter>
        <WorkspaceProvider>
          <LocaleProvider initialLocale="sv">
            <VatPage />
          </LocaleProvider>
        </WorkspaceProvider>
      </MemoryRouter>,
    )

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Skapa utkast" })).toBeEnabled()
    })
    fireEvent.click(screen.getByRole("button", { name: "Skapa utkast" }))
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Godkänn period" })).toBeEnabled()
    })
    fireEvent.click(screen.getByRole("button", { name: "Godkänn period" }))

    expect(vatReturnApprove).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "Avbryt" }))
    expect(vatReturnApprove).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole("button", { name: "Godkänn period" }))
    fireEvent.click(screen.getByRole("button", { name: "Godkänn momsrapport" }))
    await waitFor(() => expect(vatReturnApprove).toHaveBeenCalledTimes(1))
  })
})
