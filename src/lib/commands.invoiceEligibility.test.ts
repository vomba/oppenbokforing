import { beforeEach, describe, expect, it, vi } from "vitest"
import { invoke } from "@tauri-apps/api/core"
import { invoiceIssueValidationGet, invoiceValidationSourceOpen } from "./commands"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

describe("invoice eligibility command facade", () => {
  beforeEach(() => vi.clearAllMocks())

  it("dispatches source IDs through the constrained native opener", async () => {
    vi.mocked(invoke).mockResolvedValue({ data: null })
    await invoiceValidationSourceOpen({
      validationId: "invoice-recognition-se-service25-2026-exact-v1", sourceId: "bfn-timing",
    })
    expect(invoke).toHaveBeenCalledWith("invoice_validation_source_open", {
      input: { validationId: "invoice-recognition-se-service25-2026-exact-v1", sourceId: "bfn-timing" },
    })
    expect(invoke).toHaveBeenCalledTimes(1)
  })

  it("reads historical provenance through the typed command facade and unwraps absence", async () => {
    vi.mocked(invoke).mockResolvedValue({ data: null })
    expect(await invoiceIssueValidationGet({ invoiceId: "historical-invoice" })).toBeNull()
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke).toHaveBeenCalledWith("invoice_issue_validation_get", {
      input: { invoiceId: "historical-invoice" },
    })
  })
})
