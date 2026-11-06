import type { CashBusinessOriginFacts, CashIssueFacts, CashReceiptFacts } from "./bindings"
import { t, type Locale, type MessageKey } from "../i18n"

export const emptyCashIssueFacts = (): CashIssueFacts => ({ ordinaryDomesticService: null, serviceCompletedOn: null, priorPaymentReceived: null, noEuTradeInYear: null, annualVatRegistrationConfirmed: null, expectedTaxableBaseWithinLimit: null, expectedNetTurnoverWithinCashLimit: null, ordinaryTimelySendingConfirmed: null, registeredBokslutsmetodenConfirmed: null, cashTreatmentConsistentConfirmed: null, shortYearTurnoverAdjustmentReviewed: null })
export const emptyCashOriginFacts = (): CashBusinessOriginFacts => ({ newBusinessStartedIn2026: null, businessStartedOn: null, firstFinancialYearEndsOn: null, zeroOpeningBalancesConfirmed: null, noOpeningUnpaidInvoicesOrBillsConfirmed: null, noPriorUnrecordedActivityConfirmed: null, outsideWorkspaceHistoryReviewed: null })
export const emptyCashReceiptFacts = (): CashReceiptFacts => ({ actualReceivedAmountMinor: null, actualReceivedOn: null, currency: null, receivedAlready: null, bankReceiptIdentity: null, sourceEntryLocator: null, bankPdfMatchesReceiptConfirmed: null, issuedCaseStillAppliesConfirmed: null, noOtherReceiptsOrAdjustmentsConfirmed: null })
export function cashFactText(value: unknown, locale: Locale = "en"): string {
  if (value === null || value === undefined || value === "") return t(locale, "cash.unknown")
  if (value === true) return t(locale, "cash.yes")
  if (value === false) return t(locale, "cash.no")
  if (typeof value === "object") return Object.entries(value).map(([key, item]) => `${cashFactLabels[key] ? t(locale, cashFactLabels[key]) : key}: ${cashFactText(item, locale)}`).join(" · ")
  return String(value)
}
type BooleanKeys<T> = { [K in keyof T]: T[K] extends boolean | null ? K : never }[keyof T]
export const cashIssueQuestions: ReadonlyArray<{ field: BooleanKeys<CashIssueFacts>; label: MessageKey }> = [
  { field: "ordinaryDomesticService", label: "cash.ordinaryDomesticService" },
  { field: "priorPaymentReceived", label: "cash.priorPaymentReceived" },
  { field: "noEuTradeInYear", label: "cash.noEuTradeInYear" },
  { field: "annualVatRegistrationConfirmed", label: "cash.annualVatRegistrationConfirmed" },
  { field: "expectedTaxableBaseWithinLimit", label: "cash.expectedTaxableBaseWithinLimit" },
  { field: "expectedNetTurnoverWithinCashLimit", label: "cash.expectedNetTurnoverWithinCashLimit" },
  { field: "ordinaryTimelySendingConfirmed", label: "cash.ordinaryTimelySendingConfirmed" },
  { field: "registeredBokslutsmetodenConfirmed", label: "cash.registeredBokslutsmetodenConfirmed" },
  { field: "cashTreatmentConsistentConfirmed", label: "cash.cashTreatmentConsistentConfirmed" },
  { field: "shortYearTurnoverAdjustmentReviewed", label: "cash.shortYearTurnoverAdjustmentReviewed" },
]
export const cashOriginQuestions: ReadonlyArray<{ field: BooleanKeys<CashBusinessOriginFacts>; label: MessageKey }> = [
  { field: "newBusinessStartedIn2026", label: "cash.newBusinessStartedIn2026" },
  { field: "zeroOpeningBalancesConfirmed", label: "cash.zeroOpeningBalancesConfirmed" },
  { field: "noOpeningUnpaidInvoicesOrBillsConfirmed", label: "cash.noOpeningUnpaidInvoicesOrBillsConfirmed" },
  { field: "noPriorUnrecordedActivityConfirmed", label: "cash.noPriorUnrecordedActivityConfirmed" },
  { field: "outsideWorkspaceHistoryReviewed", label: "cash.outsideWorkspaceHistoryReviewed" },
]
export const cashReceiptQuestions: ReadonlyArray<{ field: BooleanKeys<CashReceiptFacts>; label: MessageKey }> = [
  { field: "receivedAlready", label: "cash.receivedAlready" },
  { field: "bankPdfMatchesReceiptConfirmed", label: "cash.bankPdfMatchesReceiptConfirmed" },
  { field: "issuedCaseStillAppliesConfirmed", label: "cash.issuedCaseStillAppliesConfirmed" },
  { field: "noOtherReceiptsOrAdjustmentsConfirmed", label: "cash.noOtherReceiptsOrAdjustmentsConfirmed" },
]

/** Known business facts get guided labels; canonical identity values are not translated. */
export const cashFactLabels: Readonly<Record<string, MessageKey>> = {
  ...Object.fromEntries([...cashIssueQuestions, ...cashOriginQuestions, ...cashReceiptQuestions].map(({ field, label }) => [field, label])),
  serviceCompletedOn: "cash.serviceCompletedOn", businessStartedOn: "cash.businessStartedOn", firstFinancialYearEndsOn: "cash.firstFinancialYearEndsOn",
  actualReceivedAmountMinor: "cash.actualReceivedAmountMinor", actualReceivedOn: "cash.actualReceivedOn", currency: "cash.currency",
  bankReceiptIdentity: "cash.bankReceiptIdentity", bankId: "cash.bankId", receivingAccountReference: "cash.receivingAccountReference", transactionReference: "cash.transactionReference", sourceEntryLocator: "cash.sourceEntryLocator",
  facts: "cash.originFacts", id: "cash.originId", firstInvoiceId: "cash.firstInvoiceId", recordedAt: "cash.recordedAt",
  accountingYear: "cash.accountingYear", accountingDate: "cash.accountingDate", completedOn: "cash.serviceCompletedOn", issuedOn: "cash.issueDate", periodKey: "cash.periodKey", dueOn: "cash.dueOn", deadlineRegime: "cash.deadlineRegime",
  validationId: "cash.validationId", evidenceDigest: "cash.evidenceDigest", reviewFingerprint: "cash.reviewFingerprint", ruleVersionId: "cash.ruleVersionId", originId: "cash.originId", coverageSummary: "cash.coverageSummary", documentId: "cash.documentId", documentSha256: "cash.documentSha256",
}
