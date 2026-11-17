//! Additive cash contracts. Unanswered owner facts remain unknown, not permission.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{invoicing::InvoiceIssueReviewSnapshot, reconciliation::ReconciliationMatchResult};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashIssueFacts {
    pub ordinary_domestic_service: Option<bool>,
    pub service_completed_on: Option<String>,
    pub prior_payment_received: Option<bool>,
    pub no_eu_trade_in_year: Option<bool>,
    pub annual_vat_registration_confirmed: Option<bool>,
    pub expected_taxable_base_within_limit: Option<bool>,
    pub expected_net_turnover_within_cash_limit: Option<bool>,
    pub ordinary_timely_sending_confirmed: Option<bool>,
    pub registered_bokslutsmetoden_confirmed: Option<bool>,
    pub cash_treatment_consistent_confirmed: Option<bool>,
    pub short_year_turnover_adjustment_reviewed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashBusinessOriginFacts {
    pub new_business_started_in_2026: Option<bool>,
    pub business_started_on: Option<String>,
    pub first_financial_year_ends_on: Option<String>,
    pub zero_opening_balances_confirmed: Option<bool>,
    pub no_opening_unpaid_invoices_or_bills_confirmed: Option<bool>,
    pub no_prior_unrecorded_activity_confirmed: Option<bool>,
    pub outside_workspace_history_reviewed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BankReceiptIdentity {
    pub bank_id: String,
    pub receiving_account_reference: String,
    pub transaction_reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashReceiptFacts {
    pub actual_received_amount_minor: Option<i64>,
    pub actual_received_on: Option<String>,
    pub currency: Option<String>,
    pub received_already: Option<bool>,
    pub bank_receipt_identity: Option<BankReceiptIdentity>,
    pub source_entry_locator: Option<String>,
    pub bank_pdf_matches_receipt_confirmed: Option<bool>,
    pub issued_case_still_applies_confirmed: Option<bool>,
    pub no_other_receipts_or_adjustments_confirmed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashIssuePreflightInput {
    pub invoice_id: String,
    pub issue_date: String,
    pub facts: CashIssueFacts,
    pub origin_review: Option<CashBusinessOriginFacts>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashIssueInput {
    pub invoice_id: String,
    pub issue_date: String,
    pub facts: CashIssueFacts,
    pub origin_review: Option<CashBusinessOriginFacts>,
    pub idempotency_key: String,
    pub review_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashReceiptRecordInput {
    pub invoice_id: String,
    pub document_id: String,
    pub idempotency_key: String,
    pub facts: CashReceiptFacts,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashHistoryGetInput {
    pub invoice_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashBusinessOrigin {
    pub id: String,
    pub facts: CashBusinessOriginFacts,
    pub first_invoice_id: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashIssueAssessment {
    pub validation_id: String,
    pub evidence_digest: String,
    pub review_fingerprint: String,
    pub rule_version_id: String,
    pub origin_id: String,
    pub accounting_year: i32,
    pub accounting_date: Option<String>,
    pub completed_on: String,
    pub issued_on: String,
    pub period_key: String,
    pub due_on: String,
    pub deadline_regime: String,
    pub coverage_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashIssuePreflight {
    pub invoice_id: String,
    pub issue_date: String,
    pub assessment: CashIssueAssessment,
    pub reviewed_invoice: InvoiceIssueReviewSnapshot,
    pub current_taxable_base_minor: i64,
    pub projected_taxable_base_minor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashIssueRecord {
    pub invoice_id: String,
    pub facts: CashIssueFacts,
    pub assessment: CashIssueAssessment,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashReceiptAssessment {
    pub validation_id: String,
    pub evidence_digest: String,
    pub review_fingerprint: String,
    pub rule_version_id: String,
    pub origin_id: String,
    pub accounting_year: i32,
    pub accounting_date: String,
    pub period_key: String,
    pub due_on: String,
    pub coverage_summary: String,
    pub document_id: String,
    pub document_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashReceiptRecord {
    pub invoice_id: String,
    pub facts: CashReceiptFacts,
    pub assessment: CashReceiptAssessment,
    pub result: ReconciliationMatchResult,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashInvoiceHistory {
    pub invoice_id: String,
    pub origin: CashBusinessOrigin,
    pub issue: CashIssueRecord,
    pub receipt: Option<CashReceiptRecord>,
}
