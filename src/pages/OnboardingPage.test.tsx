import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { MemoryRouter } from "react-router-dom"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import { WorkspaceProvider } from "../context/WorkspaceContext"
import type { AppError } from "../lib/bindings"
import { OnboardingPage } from "./OnboardingPage"

const { workspaceSettingsFixture, profileNotFoundError } = vi.hoisted(() => {
  const workspaceSettingsFixture = {
    id: "settings-1",
    locale: "sv",
    updaterEnabled: false,
    defaultExportDirectory: null,
    defaultBackupDirectory: null,
    dashboardTourCompleted: false,
    simpleMode: true,
  }

  function profileNotFoundError(field: string): AppError {
    return {
      code: "validation_error",
      message: "not found",
      details: [{ field, message: "not found", code: "invalid_value" }],
    }
  }

  return { workspaceSettingsFixture, profileNotFoundError }
})

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
  businessProfileGetCurrent: vi.fn().mockRejectedValue(profileNotFoundError("businessProfile")),
  taxProfileGetCurrent: vi.fn().mockRejectedValue(profileNotFoundError("taxProfile")),
  vatProfileGetCurrent: vi.fn().mockRejectedValue(profileNotFoundError("vatProfile")),
  businessProfileSaveCurrent: vi.fn().mockResolvedValue({}),
  taxProfileSaveCurrent: vi.fn().mockResolvedValue({}),
  vatProfileSaveCurrent: vi.fn().mockResolvedValue({}),
  workspaceSettingsSave: vi.fn().mockResolvedValue(workspaceSettingsFixture),
  complianceProfileCheck: vi.fn().mockResolvedValue({
    scenarioIds: ["vat-exempt-below-threshold"],
    passed: true,
    outcomes: {
      "vat-exempt-below-threshold": {
        mustChargeVat: false,
        mustRegisterForVat: false,
        invoiceMustStateVatExemption: true,
      },
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

beforeEach(async () => {
  const commands = await import("../lib/commands")
  vi.mocked(commands.businessProfileGetCurrent).mockRejectedValue(
    profileNotFoundError("businessProfile"),
  )
  vi.mocked(commands.taxProfileGetCurrent).mockRejectedValue(profileNotFoundError("taxProfile"))
  vi.mocked(commands.vatProfileGetCurrent).mockRejectedValue(profileNotFoundError("vatProfile"))
  vi.mocked(commands.workspaceSettingsSave).mockResolvedValue({
    ...workspaceSettingsFixture,
    locale: "sv",
  })
})

describe("OnboardingPage", () => {
  it("disables later wizard steps until business details are complete", async () => {
    renderOnboarding()
    await waitFor(() => {
      expect(screen.getByRole("textbox", { name: "Företagsnamn på fakturor" })).toBeEnabled()
    })
    expect(screen.getByRole("button", { name: "Moms" })).toBeDisabled()
    expect(screen.getByRole("button", { name: "Granska" })).toBeDisabled()
  })

  it("loads saved profiles when editing business details", async () => {
    const commands = await import("../lib/commands")
    vi.mocked(commands.businessProfileGetCurrent).mockResolvedValue({
      id: "bp-1",
      businessName: "Konsult AB",
      ownerName: "Anna",
      residencyCountry: "SE",
      sniCode: "62010",
    })
    vi.mocked(commands.taxProfileGetCurrent).mockResolvedValue({
      id: "tp-1",
      taxStatus: "f_skatt",
      expectedBusinessProfitMinor: 0,
      expectedSalaryIncomeMinor: 0,
      activeRuleYear: 2026,
    })
    vi.mocked(commands.vatProfileGetCurrent).mockResolvedValue({
      id: "vp-1",
      vatStatus: "exempt_low_turnover",
      reportingPeriod: "quarterly",
      accountingMethod: "invoice_method",
      voluntaryRegistrationDate: null,
    })

    renderOnboarding()

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Uppdatera företagsuppgifter" })).toBeInTheDocument()
    })
    expect(screen.getByRole("textbox", { name: "Företagsnamn på fakturor" })).toHaveValue("Konsult AB")
    expect(screen.getByRole("textbox", { name: "Ägarens namn" })).toHaveValue("Anna")
  })

  it("blocks editing when profile loading fails", async () => {
    const commands = await import("../lib/commands")
    vi.mocked(commands.businessProfileGetCurrent).mockRejectedValue({
      code: "storage_error",
      message: "Database failed",
    })

    renderOnboarding()

    await waitFor(() => {
      expect(
        screen.getByText(/Kunde inte ladda sparade uppgifter/i),
      ).toBeInTheDocument()
    })
    expect(screen.getByRole("button", { name: "Fortsätt" })).toBeDisabled()
  })

  it("switches onboarding language and persists workspace locale", async () => {
    const user = userEvent.setup()
    const commands = await import("../lib/commands")

    renderOnboarding()

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Ställ in ditt företag" })).toBeInTheDocument()
    })

    await user.selectOptions(screen.getByRole("combobox", { name: "Språk" }), "en")

    await waitFor(() => {
      expect(commands.workspaceSettingsSave).toHaveBeenCalledWith(
        expect.objectContaining({ locale: "en" }),
      )
    })
    expect(screen.getByRole("combobox", { name: "Language" })).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "Set up your business" })).toBeInTheDocument()
  })

  it("previews compliance on the review step", async () => {
    const user = userEvent.setup()
    const commands = await import("../lib/commands")

    renderOnboarding()

    await waitFor(() => {
      expect(screen.getByRole("textbox", { name: "Företagsnamn på fakturor" })).toBeEnabled()
    })

    await user.type(screen.getByRole("textbox", { name: "Företagsnamn på fakturor" }), "Test AB")
    await user.type(screen.getByRole("textbox", { name: "Ägarens namn" }), "Anna")
    await user.click(screen.getByRole("button", { name: "Fortsätt" }))
    await user.click(screen.getByRole("button", { name: "Fortsätt" }))
    await user.click(screen.getByRole("button", { name: "Fortsätt" }))

    await waitFor(() => {
      expect(commands.complianceProfileCheck).toHaveBeenCalled()
    })
    expect(screen.getByRole("heading", { name: "Granska och spara" })).toBeInTheDocument()
  })

  it("explains F-skatt on invoices when FA-skatt is selected", async () => {
    const user = userEvent.setup()

    renderOnboarding()

    await waitFor(() => {
      expect(screen.getByRole("textbox", { name: "Företagsnamn på fakturor" })).toBeEnabled()
    })

    await user.type(screen.getByRole("textbox", { name: "Företagsnamn på fakturor" }), "Test AB")
    await user.type(screen.getByRole("textbox", { name: "Ägarens namn" }), "Anna")
    await user.click(screen.getByRole("button", { name: "Fortsätt" }))
    await user.selectOptions(screen.getByDisplayValue("F-skatt"), "fa_skatt")

    expect(
      screen.getByText(/På fakturor till kunder ska det stå Godkänd för F-skatt/i),
    ).toBeInTheDocument()
  })
})
