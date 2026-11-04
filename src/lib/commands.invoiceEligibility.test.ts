import { beforeEach, describe, expect, it, vi } from "vitest"
import { invoke } from "@tauri-apps/api/core"
import { invoiceIssueValidationGet } from "./commands"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

describe("invoiceIssueValidationGet", () => {
  beforeEach(() => vi.clearAllMocks())

  it("reads historical provenance through the typed command facade and unwraps absence", async () => {
    vi.mocked(invoke).mockResolvedValue({ data: null })
    expect(await invoiceIssueValidationGet({ invoiceId: "historical-invoice" })).toBeNull()
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke).toHaveBeenCalledWith("invoice_issue_validation_get", {
      input: { invoiceId: "historical-invoice" },
    })
  })
})
