import type { CashInvoiceHistory, InvoiceIssueValidation } from "./bindings"
import invoiceV1 from "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
import invoiceV2 from "../../fixtures/accounting-validation/2026-domestic-service25-exact-v2.json"
import cashV1 from "../../fixtures/accounting-validation/2026-cash-pdf-full.json"

export type IssuedTreatment = CashInvoiceHistory | "invoice" | "legacy" | "blocked"
/** Known issued proof wins. Absent proof is legacy, not validated recognition. */
export function classifyIssuedTreatment(invoiceId: string, cash: CashInvoiceHistory | null, invoice: InvoiceIssueValidation | null): IssuedTreatment {
  if (cash) return cash.invoiceId === invoiceId && cash.issue.invoiceId === invoiceId && cash.issue.assessment.validationId === cashV1.validationId && cash.issue.assessment.accountingDate === null && cash.origin.id === cash.issue.assessment.originId ? cash : "blocked"
  if (cash === null && invoice === null) return "legacy"
  return invoice?.invoiceId === invoiceId && [invoiceV1.validationId, invoiceV2.validationId].includes(invoice.assessment.validationId) ? "invoice" : "blocked"
}

/** Only selects the existing UI/API. Backend settlement guards remain authoritative.
 * A current invoice profile does not prove a legacy invoice's original method.
 */
export function canUseExistingSettlement(treatment: IssuedTreatment | undefined, currentMethod: string | null | undefined): boolean {
  return treatment === "invoice" || (treatment === "legacy" && currentMethod === "invoice_method")
}
