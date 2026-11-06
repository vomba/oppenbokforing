import type * as WorkspaceContextModule from "../context/WorkspaceContext"
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { MemoryRouter } from "react-router-dom"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { LocaleProvider } from "../context/LocaleContext"
import type { VatReturnSummary, VatReturnTrace } from "../lib/bindings"
import { formatSekMinor } from "../lib/money"
import { VatPage } from "./VatPage"

const mocks = vi.hoisted(() => ({
  vatReturnDraftCreate: vi.fn(),
  vatReturnApprove: vi.fn(),
  vatReturnTrace: vi.fn(),
  workspace: { id: "ws-1", name: "Testfirma", dataDir: "/tmp/data", databasePath: "/tmp/ws.sqlite" },
}))
const { vatReturnDraftCreate, vatReturnApprove, vatReturnTrace } = mocks

vi.mock("../context/WorkspaceContext", async () => {
  const actual = await vi.importActual<typeof WorkspaceContextModule>("../context/WorkspaceContext")
  return {
    ...actual,
    useWorkspace: () => ({ workspace: mocks.workspace, setWorkspace: vi.fn() }),
  }
})

vi.mock("../components/AppSidebar", () => ({ AppSidebar: () => <nav aria-label="sidebar" /> }))
vi.mock("../lib/commands", () => ({
  appErrorMessage: (error: unknown, fallback: string) => error instanceof Error ? error.message : fallback,
  cashflowOverviewGet: vi.fn().mockResolvedValue({
    vatPeriodKey: "2026-Q1",
    vatReserveMinor: 0,
    spendableCashMinor: 0,
  }),
  taxProfileGetCurrent: vi.fn().mockResolvedValue({ activeRuleYear: 2026 }),
  vatProfileGetCurrent: vi.fn().mockResolvedValue({ vatStatus: "registered", reportingPeriod: "quarterly" }),
  vatReturnApprove: mocks.vatReturnApprove,
  vatReturnDraftCreate: mocks.vatReturnDraftCreate,
  vatReturnExport: vi.fn(),
  vatReturnTrace: mocks.vatReturnTrace,
  vatThresholdStatusGet: vi.fn().mockResolvedValue({ warning: "none", annualTurnoverMinor: 0 }),
  workspaceSettingsGet: vi.fn().mockResolvedValue({ defaultExportDirectory: null }),
}))

function draft(overrides: Partial<VatReturnSummary> = {}): VatReturnSummary {
  return {
    id: "vat-1",
    fiscalPeriodId: "period-1",
    periodKey: "2026-Q1",
    status: "draft",
    reviewRevision: "revision-1",
    ruleVersionId: "rules-1",
    box49AmountMinor: 250000,
    zeroReturn: false,
    boxes: [{ boxCode: "49", amountMinor: 250000, sourceQueryHash: "source-1" }],
    exportPath: null,
    ...overrides,
  }
}

function trace(overrides: Partial<VatReturnTrace> = {}): VatReturnTrace {
  return {
    reviewRevision: "revision-1",
    ruleVersionId: "rules-1",
    taxYear: 2026,
    sourceUrl: "https://example.com/rules",
    boxes: [],
    ...overrides,
  }
}

function amountText(amount: number) {
  return formatSekMinor(amount).replace(/\s/g, " ")
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej })
  return { promise, resolve, reject }
}

function page(route = "/vat") {
  return (
    <MemoryRouter initialEntries={[route]}>
      <LocaleProvider initialLocale="sv">
        <VatPage />
      </LocaleProvider>
    </MemoryRouter>
  )
}

async function createDraft() {
  await waitFor(() => expect(screen.getByRole("button", { name: "Skapa utkast" })).toBeEnabled())
  fireEvent.click(screen.getByRole("button", { name: "Skapa utkast" }))
  await waitFor(() => expect(screen.getByRole("button", { name: "Godkänn period" })).toBeEnabled())
}

function openReview() {
  fireEvent.click(screen.getByRole("button", { name: "Godkänn period" }))
  return screen.getByRole("dialog")
}

function confirmReview() {
  fireEvent.click(screen.getByRole("button", { name: "Godkänn momsrapport" }))
}

beforeEach(() => {
  vi.clearAllMocks()
  vatReturnDraftCreate.mockReset()
  vatReturnApprove.mockReset()
  vatReturnTrace.mockReset()
  // Reset only the variable command mocks; the read-only setup mocks keep their defaults.
  mocks.workspace = { ...mocks.workspace, id: "ws-1" }
  vatReturnDraftCreate.mockImplementation(({ periodKey }: { periodKey: string }) => Promise.resolve(draft({ periodKey })))
  vatReturnApprove.mockResolvedValue(draft({ status: "approved" }))
  vatReturnTrace.mockResolvedValue(trace())
})

describe("VatPage", () => {
  it("reviews and cancels before approving the exact displayed revision", async () => {
    render(page())
    await createDraft()
    const dialog = openReview()
    expect(dialog).toHaveTextContent("2026-Q1")
    expect(dialog).toHaveTextContent(amountText(250000))
    expect(vatReturnApprove).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "Avbryt" }))
    expect(vatReturnApprove).not.toHaveBeenCalled()
    openReview()
    confirmReview()
    await waitFor(() => expect(vatReturnApprove).toHaveBeenCalledTimes(1))
    expect(vatReturnApprove).toHaveBeenCalledWith({
      vatReturnId: "vat-1", reviewedRevision: "revision-1", idempotencyKey: expect.any(String),
    })
  })

  it("leaves stale review failure visible without refreshing or automatically approving a newer revision", async () => {
    vatReturnApprove.mockRejectedValue(new Error("Regenerate and review the VAT draft"))
    render(page())
    await createDraft()
    await waitFor(() => expect(vatReturnTrace).toHaveBeenCalledTimes(1))
    openReview()
    confirmReview()
    await screen.findByText("Regenerate and review the VAT draft")
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(vatReturnDraftCreate).toHaveBeenCalledTimes(1)
    expect(vatReturnTrace).toHaveBeenCalledTimes(1)
    expect(vatReturnApprove).toHaveBeenCalledTimes(1)

    // An explicit retry of the same review retains its key.
    openReview()
    confirmReview()
    await waitFor(() => expect(vatReturnApprove).toHaveBeenCalledTimes(2))
    expect(vatReturnApprove.mock.calls[1][0]).toEqual(vatReturnApprove.mock.calls[0][0])
    await screen.findByText("Regenerate and review the VAT draft")

    // Regeneration may reuse the return ID, but must require fresh review and a fresh key.
    vatReturnDraftCreate.mockResolvedValue(draft({ reviewRevision: "revision-2", box49AmountMinor: 300000 }))
    await createDraft()
    expect(vatReturnApprove).toHaveBeenCalledTimes(2)
    expect(openReview()).toHaveTextContent(amountText(300000))
    confirmReview()
    await waitFor(() => expect(vatReturnApprove).toHaveBeenCalledTimes(3))
    expect(vatReturnApprove.mock.calls[2][0]).toEqual({
      vatReturnId: "vat-1", reviewedRevision: "revision-2", idempotencyKey: expect.any(String),
    })
    expect(vatReturnApprove.mock.calls[2][0].idempotencyKey).not.toBe(vatReturnApprove.mock.calls[0][0].idempotencyKey)
  })

  it("recovers from a lost draft response and a superseded key without navigating away", async () => {
    vatReturnDraftCreate
      .mockRejectedValueOnce(new Error("Response lost"))
      .mockRejectedValueOnce({ code: "vat_draft_superseded", message: "Regenerate and review with a new request key" })
      .mockResolvedValueOnce(draft({ reviewRevision: "fresh-revision" }))
    render(page())
    const create = await screen.findByRole("button", { name: "Skapa utkast" })
    await waitFor(() => expect(create).toBeEnabled())
    fireEvent.click(create)
    await screen.findByText("Response lost")
    fireEvent.click(create)
    await waitFor(() => expect(vatReturnDraftCreate).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(create).toBeEnabled())
    expect(vatReturnDraftCreate.mock.calls[1][0].idempotencyKey).toBe(vatReturnDraftCreate.mock.calls[0][0].idempotencyKey)
    expect(vatReturnDraftCreate).toHaveBeenCalledTimes(2)
    await createDraft()
    expect(vatReturnDraftCreate).toHaveBeenCalledTimes(3)
    expect(vatReturnDraftCreate.mock.calls[2][0].idempotencyKey).not.toBe(vatReturnDraftCreate.mock.calls[0][0].idempotencyKey)
    expect(vatReturnApprove).not.toHaveBeenCalled()
    openReview()
    confirmReview()
    await waitFor(() => expect(vatReturnApprove).toHaveBeenCalledWith(expect.objectContaining({ reviewedRevision: "fresh-revision" })))
  })

  it("uses stored box amounts even when the trace supplies different amounts or extra boxes", async () => {
    vatReturnTrace.mockResolvedValue(trace({ boxes: [
      { boxNumber: "49", amountMinor: 999999, voucherIds: ["voucher-1"] },
      { boxNumber: "99", amountMinor: 888888, voucherIds: [] },
    ] }))
    render(page())
    await createDraft()
    const link = await screen.findByRole("link", { name: "Öppna verifikation" })
    const row = link.closest("tr")!
    expect(within(row).getByText(amountText(250000))).toBeInTheDocument()
    expect(screen.queryByText(amountText(999999))).not.toBeInTheDocument()
    expect(screen.queryByText("99")).not.toBeInTheDocument()
    expect(openReview()).toHaveTextContent(amountText(250000))
  })

  it("discards a trace from a different review revision", async () => {
    vatReturnTrace.mockResolvedValue(trace({ reviewRevision: "revision-older", boxes: [
      { boxNumber: "49", amountMinor: 999999, voucherIds: ["wrong-voucher"] },
    ] }))
    render(page())
    await createDraft()
    await waitFor(() => expect(vatReturnTrace).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole("link", { name: "Öppna verifikation" })).not.toBeInTheDocument()
    expect(screen.getAllByText(amountText(250000))).toHaveLength(2)
  })

  it("requires regeneration of legacy drafts without a review revision", async () => {
    vatReturnDraftCreate.mockResolvedValue(draft({ reviewRevision: null }))
    render(page())
    await waitFor(() => expect(screen.getByRole("button", { name: "Skapa utkast" })).toBeEnabled())
    fireEvent.click(screen.getByRole("button", { name: "Skapa utkast" }))
    await screen.findByText(amountText(250000), { selector: "dd" })
    expect(screen.getByRole("button", { name: "Godkänn period" })).toBeDisabled()
    expect(vatReturnApprove).not.toHaveBeenCalled()
  })

  it("uses the backend-selected tax-task period from the route", async () => {
    render(page("/vat?periodKey=2026-Q2"))
    await createDraft()
    expect(vatReturnDraftCreate).toHaveBeenLastCalledWith(expect.objectContaining({ periodKey: "2026-Q2" }))
  })

  it.each(["period", "workspace"])("invalidates an open confirmation on %s changes", async (context) => {
    const view = render(page())
    await createDraft()
    openReview()
    if (context === "workspace") {
      mocks.workspace = { ...mocks.workspace, id: "ws-2" }
      view.rerender(page())
    } else {
      // The real dialog traps interaction; simulate an external selection update.
      fireEvent.change(screen.getByRole("combobox", { hidden: true }), { target: { value: "2026-Q2" } })
    }
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Godkänn period" })).toBeDisabled()
    expect(screen.queryByRole("table")).not.toBeInTheDocument()
    expect(vatReturnApprove).not.toHaveBeenCalled()
    await createDraft()
    openReview()
    confirmReview()
    await waitFor(() => expect(vatReturnApprove).toHaveBeenCalledTimes(1))
  })

  it.each([
    ["period", "resolve"], ["period", "reject"], ["workspace", "resolve"], ["workspace", "reject"],
  ])("ignores delayed draft results after a %s round trip (%s)", async (context, outcome) => {
    const pending = deferred<VatReturnSummary>()
    vatReturnDraftCreate.mockReturnValueOnce(pending.promise)
    const view = render(page())
    await waitFor(() => expect(screen.getByRole("button", { name: "Skapa utkast" })).toBeEnabled())
    fireEvent.click(screen.getByRole("button", { name: "Skapa utkast" }))
    for (const value of context === "workspace" ? ["ws-2", "ws-1"] : ["2026-Q2", "2026-Q1"]) {
      if (context === "workspace") {
        mocks.workspace = { ...mocks.workspace, id: value }
        view.rerender(page())
        await waitFor(() => expect(screen.getByRole("button", { name: "Skapa utkast" })).toBeEnabled())
      } else {
        fireEvent.change(screen.getByRole("combobox"), { target: { value } })
      }
    }
    await act(async () => {
      if (outcome === "resolve") pending.resolve(draft())
      else pending.reject(new Error("Old draft error"))
    })
    expect(screen.queryByText("Old draft error")).not.toBeInTheDocument()
    expect(screen.queryByRole("table")).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Godkänn period" })).toBeDisabled()
    expect(vatReturnTrace).not.toHaveBeenCalled()
  })

  it.each(["period", "workspace"])("discards a pending trace across %s changes without fetching the old return again", async (context) => {
    const pending = deferred<VatReturnTrace>()
    vatReturnTrace.mockReturnValueOnce(pending.promise)
    const view = render(page())
    await createDraft()
    await waitFor(() => expect(vatReturnTrace).toHaveBeenCalledTimes(1))
    if (context === "workspace") {
      mocks.workspace = { ...mocks.workspace, id: "ws-2" }
      view.rerender(page())
    } else {
      fireEvent.change(screen.getByRole("combobox"), { target: { value: "2026-Q2" } })
    }
    await act(async () => { pending.resolve(trace({ boxes: [
      { boxNumber: "49", amountMinor: 999999, voucherIds: ["old-voucher"] },
    ] })) })
    expect(vatReturnTrace).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole("table")).not.toBeInTheDocument()
    expect(screen.queryByRole("link", { name: "Öppna verifikation" })).not.toBeInTheDocument()
  })

  it("scopes approval retry keys to the workspace even for the same return and revision", async () => {
    vatReturnApprove.mockRejectedValue(new Error("Approval failed"))
    const view = render(page())
    await createDraft()
    openReview()
    confirmReview()
    await screen.findByText("Approval failed")
    const firstKey = vatReturnApprove.mock.calls[0][0].idempotencyKey
    mocks.workspace = { ...mocks.workspace, id: "ws-2" }
    view.rerender(page())
    expect(screen.queryByText("Approval failed")).not.toBeInTheDocument()
    await createDraft()
    openReview()
    confirmReview()
    await waitFor(() => expect(vatReturnApprove).toHaveBeenCalledTimes(2))
    expect(vatReturnApprove.mock.calls[1][0].idempotencyKey).not.toBe(firstKey)
  })

  it("does not allow an older delayed trace to replace the regenerated revision's trace", async () => {
    const pending = deferred<VatReturnTrace>()
    vatReturnTrace.mockReturnValueOnce(pending.promise)
    render(page())
    await createDraft()
    await waitFor(() => expect(vatReturnTrace).toHaveBeenCalledTimes(1))
    vatReturnDraftCreate.mockResolvedValue(draft({ reviewRevision: "revision-2" }))
    vatReturnTrace.mockResolvedValue(trace({ reviewRevision: "revision-2", boxes: [
      { boxNumber: "49", amountMinor: 250000, voucherIds: ["new-voucher"] },
    ] }))
    await createDraft()
    await waitFor(() => expect(screen.getByRole("link", { name: "Öppna verifikation" })).toHaveAttribute("href", "/ledger?voucherId=new-voucher"))
    await act(async () => { pending.resolve(trace({ boxes: [
      { boxNumber: "49", amountMinor: 999999, voucherIds: ["old-voucher"] },
    ] })) })
    expect(screen.getByRole("link", { name: "Öppna verifikation" })).toHaveAttribute("href", "/ledger?voucherId=new-voucher")
  })

  it.each([
    ["resolve", "workspace"], ["reject", "workspace"], ["resolve", "period"], ["reject", "period"],
  ])("ignores an old approval's delayed %s after a %s change without clearing newer busy state", async (outcome, context) => {
    const approval = deferred<VatReturnSummary>()
    const nextDraft = deferred<VatReturnSummary>()
    vatReturnApprove.mockReturnValueOnce(approval.promise)
    const view = render(page())
    await createDraft()
    openReview()
    confirmReview()
    if (context === "workspace") {
      mocks.workspace = { ...mocks.workspace, id: "ws-2" }
      view.rerender(page())
    } else {
      fireEvent.change(screen.getByRole("combobox"), { target: { value: "2026-Q2" } })
    }
    vatReturnDraftCreate.mockReturnValueOnce(nextDraft.promise)
    await waitFor(() => expect(screen.getByRole("button", { name: "Skapa utkast" })).toBeEnabled())
    fireEvent.click(screen.getByRole("button", { name: "Skapa utkast" }))
    await act(async () => {
      if (outcome === "resolve") approval.resolve(draft({ status: "approved" }))
      else approval.reject(new Error("Old workspace error"))
    })
    expect(screen.queryByText("Old workspace error")).not.toBeInTheDocument()
    expect(screen.queryByRole("table")).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Skapa utkast" })).toBeDisabled()
    await act(async () => { nextDraft.resolve(draft({ reviewRevision: "revision-2", periodKey: context === "period" ? "2026-Q2" : "2026-Q1" })) })
    expect(screen.getByRole("button", { name: "Godkänn period" })).toBeEnabled()
  })
})
