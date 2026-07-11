import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { MemoryRouter } from "react-router-dom"
import { afterEach, describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import { OnboardingPage } from "./OnboardingPage"

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

vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>("react-router-dom")
  return {
    ...actual,
    useNavigate: () => vi.fn(),
  }
})

vi.mock("../lib/commands", () => ({
  ruleVersionGet: vi.fn().mockResolvedValue({
    taxYear: 2026,
    sourceUrl: "https://example.com/rules",
  }),
  businessProfileSaveCurrent: vi.fn().mockResolvedValue({}),
  taxProfileSaveCurrent: vi.fn().mockResolvedValue({}),
  vatProfileSaveCurrent: vi.fn().mockResolvedValue({}),
  complianceCheckRun: vi.fn().mockResolvedValue({
    scenarioId: "vat-exempt-below-threshold",
    passed: true,
    outcomes: {
      mustChargeVat: false,
      mustRegisterForVat: false,
      invoiceMustStateVatExemption: true,
    },
    ruleYear: 2026,
  }),
}))

function renderOnboarding() {
  return render(
    <MemoryRouter>
      <WorkspaceProvider>
        <LocaleProvider initialLocale="sv">
          <OnboardingPage />
        </LocaleProvider>
      </WorkspaceProvider>
    </MemoryRouter>,
  )
}

afterEach(() => {
  cleanup()
})

describe("OnboardingPage", () => {
  it("disables later wizard steps until business details are complete", async () => {
    renderOnboarding()
    expect(screen.getByRole("button", { name: "Moms" })).toBeDisabled()
    expect(screen.getByRole("button", { name: "Granska" })).toBeDisabled()
  })

  it("previews compliance on the review step", async () => {
    const user = userEvent.setup()
    const commands = await import("../lib/commands")

    renderOnboarding()

    await user.type(screen.getByRole("textbox", { name: "Företagsnamn" }), "Test AB")
    await user.type(screen.getByRole("textbox", { name: "Ägarens namn" }), "Anna")
    await user.click(screen.getByRole("button", { name: "Fortsätt" }))
    await user.click(screen.getByRole("button", { name: "Fortsätt" }))
    await user.click(screen.getByRole("button", { name: "Fortsätt" }))

    await waitFor(() => {
      expect(commands.complianceCheckRun).toHaveBeenCalled()
    })
    expect(screen.getByRole("heading", { name: "Granska och spara" })).toBeInTheDocument()
  })
})
