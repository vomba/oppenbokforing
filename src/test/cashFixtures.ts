import type { CashInvoiceHistory, InvoiceIssueValidation } from "../lib/bindings"
import { emptyCashOriginFacts, emptyCashIssueFacts } from "../lib/cashPresentation"
export const cashHistory: CashInvoiceHistory = {
  invoiceId: "i", origin: { id: "o", firstInvoiceId: "i", recordedAt: "2026-06-01", facts: { ...emptyCashOriginFacts(), businessStartedOn: "2026-04-01", firstFinancialYearEndsOn: "2026-12-31" } },
  issue: { invoiceId: "i", facts: { ...emptyCashIssueFacts(), serviceCompletedOn: "2026-05-31" }, recordedAt: "2026-06-01", assessment: { validationId: "cash-recognition-se-service25-2026-pdf-full-v1", evidenceDigest: "digest", reviewFingerprint: "fingerprint", ruleVersionId: "rules", originId: "o", accountingYear: 2026, accountingDate: null, completedOn: "2026-05-31", issuedOn: "2026-06-01", periodKey: "2026", dueOn: "2027-05-12", deadlineRegime: "annual", coverageSummary: "English coverage" } }, receipt: null,
}

export const invoiceIssueProof: InvoiceIssueValidation = {
  invoiceId: "inv-1",
  facts: { ordinaryDomesticService: true, serviceCompletedOn: "2026-01-14", priorPaymentReceived: false, noEuTradeInYear: true, annualVatRegistrationConfirmed: true, expectedTaxableBaseWithinLimit: true, ordinaryInvoiceTimingConfirmed: true },
  assessment: { ...cashHistory.issue.assessment, validationId: "invoice-recognition-se-service25-2026-exact-v1", accountingDate: "2026-01-15" },
  recordedAt: "2026-01-15",
}
