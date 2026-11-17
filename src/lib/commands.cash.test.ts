import { beforeEach, expect, it, vi } from "vitest"
import { invoke } from "@tauri-apps/api/core"
import { cashBusinessOriginGet, invoiceCashHistoryGet, invoiceCashIssue, invoiceCashIssuePreflight, invoiceCashReceiptRecord } from "./commands"
import { emptyCashIssueFacts, emptyCashReceiptFacts } from "./cashPresentation"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
beforeEach(() => vi.clearAllMocks())
it("dispatches all five additive cash commands with exact envelopes and unwraps data", async () => {
  vi.mocked(invoke).mockResolvedValue({ data: null })
  expect(await cashBusinessOriginGet()).toBeNull()
  expect(invoke).toHaveBeenLastCalledWith("cash_business_origin_get")
  expect(await invoiceCashHistoryGet({ invoiceId: "i" })).toBeNull()
  expect(invoke).toHaveBeenLastCalledWith("invoice_cash_history_get", { input: { invoiceId: "i" } })
  const preflight = { invoiceId: "i", issueDate: "2026-06-01", facts: emptyCashIssueFacts(), originReview: null }
  await invoiceCashIssuePreflight(preflight)
  expect(invoke).toHaveBeenLastCalledWith("invoice_cash_issue_preflight", { input: preflight })
  const issue = { ...preflight, idempotencyKey: "key", reviewFingerprint: "review" }
  await invoiceCashIssue(issue)
  expect(invoke).toHaveBeenLastCalledWith("invoice_cash_issue", { input: issue })
  const receipt = { invoiceId: "i", documentId: "pdf", idempotencyKey: "key", facts: emptyCashReceiptFacts() }
  await invoiceCashReceiptRecord(receipt)
  expect(invoke).toHaveBeenLastCalledWith("invoice_cash_receipt_record", { input: receipt })
  expect(invoke).toHaveBeenCalledTimes(5)
})
it("propagates native rejection instead of falling back to invoice payment", async () => {
  vi.mocked(invoke).mockRejectedValue(new Error("blocked"))
  await expect(invoiceCashReceiptRecord({ invoiceId: "i", documentId: "pdf", idempotencyKey: "key", facts: emptyCashReceiptFacts() })).rejects.toThrow("blocked")
  expect(invoke).toHaveBeenCalledTimes(1)
})
