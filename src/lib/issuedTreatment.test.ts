import { expect, it } from "vitest"
import { canUseExistingSettlement, classifyIssuedTreatment } from "./issuedTreatment"
import { cashHistory } from "../test/cashFixtures"
import type { InvoiceIssueValidation } from "./bindings"
const id = cashHistory.invoiceId
const invoiceProof: InvoiceIssueValidation = { invoiceId: id, facts: { ordinaryDomesticService: true, serviceCompletedOn: "2026-05-31", priorPaymentReceived: false, noEuTradeInYear: true, annualVatRegistrationConfirmed: true, expectedTaxableBaseWithinLimit: true, ordinaryInvoiceTimingConfirmed: true }, assessment: { ...cashHistory.issue.assessment, accountingDate: "2026-06-01", validationId: "invoice-recognition-se-service25-2026-exact-v2" }, recordedAt: "2026-06-01" }
it("uses immutable supported cash proof and refuses malformed/cross-invoice history", () => {
  expect(classifyIssuedTreatment(id, cashHistory, null)).toBe(cashHistory)
  expect(classifyIssuedTreatment(id, cashHistory, invoiceProof)).toBe(cashHistory)
  expect(classifyIssuedTreatment("different", cashHistory, null)).toBe("blocked")
  expect(classifyIssuedTreatment(id, { ...cashHistory, issue: { ...cashHistory.issue, assessment: { ...cashHistory.issue.assessment, accountingDate: "2026-06-01" } } }, null)).toBe("blocked")
})
it("requires invoice v1/v2 provenance, never invents treatment from a voucher or profile", () => {
  expect(classifyIssuedTreatment(id, null, null)).toBe("legacy")
  expect(classifyIssuedTreatment(id, null, invoiceProof)).toBe("invoice")
  expect(classifyIssuedTreatment("other-invoice", null, invoiceProof)).toBe("blocked")
  expect(classifyIssuedTreatment(id, null, { ...invoiceProof, assessment: { ...invoiceProof.assessment, validationId: "invoice-recognition-se-service25-2026-exact-v1" } })).toBe("invoice")
  expect(classifyIssuedTreatment(id, null, { ...invoiceProof, assessment: { ...invoiceProof.assessment, validationId: "unknown" } })).toBe("blocked")
})

it("keeps legacy routing separate from validation and requires an explicit current invoice method", () => {
  const legacy = classifyIssuedTreatment(id, null, null)
  expect(legacy).toBe("legacy")
  expect(legacy).not.toBe("invoice")
  expect(canUseExistingSettlement(legacy, "invoice_method")).toBe(true)
  for (const method of ["cash_method", null, undefined, "unknown"]) expect(canUseExistingSettlement(legacy, method)).toBe(false)
  expect(canUseExistingSettlement("blocked", "invoice_method")).toBe(false)
  expect(canUseExistingSettlement(cashHistory, "invoice_method")).toBe(false)
  expect(canUseExistingSettlement("invoice", null)).toBe(true)
})
