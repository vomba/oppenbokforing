//! Bounded cash identity issuance and full retained-PDF receipt recognition.
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::validation_registry::CASH_V1;
use crate::{
    error::AppError, invoicing::InvoiceSummary, reconciliation::ReconciliationMatchResult,
};

mod pdf_process;
mod policy;
mod receipt;
pub mod types;
pub use types::*;

const COVERAGE: &str = "2026 cash-method identity only: new Swedish sole trader, reviewed zero opening and December-ending first year, ordinary completed domestic SEK service, exact 25% VAT, registered consistent bokslutsmetoden and annual/no-EU VAT. No issue-date recognition, cash VAT/year-end approval, credit or unsupported receipt coverage.";

fn blocked(message: impl Into<String>) -> AppError {
    AppError::validation(message, "eligibility")
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn encode<T: Serialize>(v: &T) -> Result<String, AppError> {
    serde_json::to_string(v).map_err(|_| blocked("Cash provenance cannot be encoded"))
}
fn decode<T: DeserializeOwned>(raw: &str) -> Result<T, AppError> {
    serde_json::from_str(raw).map_err(|_| blocked("Cash provenance is invalid"))
}
fn value<T: Serialize>(v: &T) -> Result<Value, AppError> {
    serde_json::to_value(v).map_err(|_| blocked("Cash provenance cannot be encoded"))
}
fn origin_id(
    workspace: &str,
    invoice: &str,
    facts: &CashBusinessOriginFacts,
) -> Result<String, AppError> {
    Ok(format!(
        "cash-origin-{}",
        hash(
            encode(&json!({"workspaceId":workspace,"firstInvoiceId":invoice,"facts":facts}))?
                .as_bytes()
        )
    ))
}
fn assessment(
    issue: &str,
    completion: &str,
    origin: &str,
    fingerprint: String,
) -> CashIssueAssessment {
    CashIssueAssessment {
        validation_id: CASH_V1.id.into(),
        evidence_digest: CASH_V1.digest(),
        review_fingerprint: fingerprint,
        rule_version_id: CASH_V1.rule_id.into(),
        origin_id: origin.into(),
        accounting_year: 2026,
        accounting_date: None,
        completed_on: completion.into(),
        issued_on: issue.into(),
        period_key: "2026".into(),
        due_on: "2027-05-12".into(),
        deadline_regime: "annual_may_12".into(),
        coverage_summary: COVERAGE.into(),
    }
}
fn decode_origin(
    row: &sqlx::sqlite::SqliteRow,
    workspace: &str,
) -> Result<CashBusinessOrigin, AppError> {
    let origin = CashBusinessOrigin {
        id: row.try_get("id")?,
        facts: decode(&row.try_get::<String, _>("facts_json")?)?,
        first_invoice_id: row.try_get("first_invoice_id")?,
        recorded_at: row.try_get("recorded_at")?,
    };
    if origin.id != origin_id(workspace, &origin.first_invoice_id, &origin.facts)? {
        return Err(blocked("Retained origin identity is inconsistent"));
    }
    Ok(origin)
}
async fn origin_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
) -> Result<Option<CashBusinessOrigin>, AppError> {
    sqlx::query("SELECT id,facts_json,first_invoice_id,recorded_at FROM cash_business_origins WHERE workspace_id=?1").bind(workspace).fetch_optional(&mut **tx).await?.map(|r|decode_origin(&r,workspace)).transpose()
}

pub async fn invoice_cash_issue_preflight(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &CashIssuePreflightInput,
) -> Result<CashIssuePreflight, AppError> {
    let mut tx = pool.begin().await?;
    let evaluation = policy::evaluate_tx(&mut tx, workspace_id, input).await?;
    tx.commit().await?;
    Ok(evaluation.preflight)
}
fn request(input: &CashIssueInput) -> Result<Value, AppError> {
    let mut request = value(input)?;
    request
        .as_object_mut()
        .ok_or_else(|| blocked("Invalid issue request"))?
        .remove("idempotencyKey");
    Ok(request)
}
async fn replay_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
    input: &CashIssueInput,
    key: &str,
) -> Result<Option<InvoiceSummary>, AppError> {
    let rows=sqlx::query("SELECT invoice_id,request_json,result_json FROM cash_invoice_issues WHERE workspace_id=?1 AND (invoice_id=?2 OR idempotency_key=?3)").bind(workspace).bind(&input.invoice_id).bind(key).fetch_all(&mut **tx).await?;
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() != 1
        || rows[0].try_get::<String, _>("invoice_id")? != input.invoice_id
        || decode::<Value>(&rows[0].try_get::<String, _>("request_json")?)? != request(input)?
    {
        return Err(blocked(
            "Cash issue replay input differs from the completed request",
        ));
    }
    Ok(Some(decode(&rows[0].try_get::<String, _>("result_json")?)?))
}

pub async fn invoice_cash_issue(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &CashIssueInput,
) -> Result<InvoiceSummary, AppError> {
    let key = crate::idempotency::normalize_idempotency_key(&input.idempotency_key)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    // Completed result recovery must precede profile, activation and clock guards.
    if let Some(invoice) = replay_tx(&mut tx, workspace_id, input, key).await? {
        tx.commit().await?;
        return Ok(invoice);
    }
    let evaluation = policy::evaluate_tx(
        &mut tx,
        workspace_id,
        &CashIssuePreflightInput {
            invoice_id: input.invoice_id.clone(),
            issue_date: input.issue_date.clone(),
            facts: input.facts.clone(),
            origin_review: input.origin_review.clone(),
        },
    )
    .await?;
    if input.review_fingerprint != evaluation.preflight.assessment.review_fingerprint {
        return Err(blocked("Cash review is missing or stale; review the current invoice, origin, profiles, rules and annual commitments"));
    }
    if evaluation.new_origin {
        let start = evaluation
            .origin
            .facts
            .business_started_on
            .as_deref()
            .unwrap();
        sqlx::query("UPDATE fiscal_years SET starts_on=?1,ends_on='2026-12-31',updated_at=CURRENT_TIMESTAMP WHERE workspace_id=?2 AND id=?3 AND status='open'").bind(start).bind(workspace_id).bind(&evaluation.fiscal_year_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO cash_business_origins(id,workspace_id,first_invoice_id,fiscal_year_id,facts_json) VALUES(?1,?2,?3,?4,?5)").bind(&evaluation.origin.id).bind(workspace_id).bind(&input.invoice_id).bind(&evaluation.fiscal_year_id).bind(encode(&evaluation.origin.facts)?).execute(&mut *tx).await?;
        crate::audit::record_event_tx(&mut *tx,workspace_id,"cash_business_origin","fiscal_year",Some(&evaluation.fiscal_year_id),&encode(&json!({"originId":evaluation.origin.id,"businessStartedOn":start,"firstFinancialYearEndsOn":"2026-12-31","previousStart":evaluation.context["storedYearStart"]}))?).await?;
    }
    let snapshot = super::capture_issued_invoice_snapshot_tx(
        &mut tx,
        workspace_id,
        &input.invoice_id,
        &input.issue_date,
        evaluation.tax_status,
        evaluation.vat_status,
    )
    .await?;
    let sequence=sqlx::query("SELECT id,prefix,next_number FROM invoice_sequences WHERE workspace_id=?1 AND fiscal_year_id=?2").bind(workspace_id).bind(&evaluation.fiscal_year_id).fetch_optional(&mut *tx).await?.ok_or_else(||blocked("Invoice sequence missing"))?;
    let number: i64 = sequence.try_get("next_number")?;
    let next = number
        .checked_add(1)
        .filter(|_| number > 0)
        .ok_or_else(|| blocked("Invoice number is out of range"))?;
    let invoice_number = format!("{}{number:04}", sequence.try_get::<String, _>("prefix")?);
    sqlx::query("UPDATE invoice_sequences SET next_number=?1 WHERE id=?2")
        .bind(next)
        .bind(sequence.try_get::<String, _>("id")?)
        .execute(&mut *tx)
        .await?;
    let pdf_job_id =
        super::enqueue_pdf_job_tx(&mut tx, workspace_id, &input.invoice_id, &invoice_number)
            .await?;
    let updated=sqlx::query("UPDATE invoices SET status='issued',fiscal_year_id=?1,invoice_number=?2,issue_date=?3,pdf_job_id=?4,updated_at=CURRENT_TIMESTAMP WHERE workspace_id=?5 AND id=?6 AND status='draft' AND voucher_id IS NULL").bind(&evaluation.fiscal_year_id).bind(&invoice_number).bind(&input.issue_date).bind(&pdf_job_id).bind(workspace_id).bind(&input.invoice_id).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(blocked("Invoice changed during issuance"));
    }
    let invoice = super::fetch_invoice_summary_tx(&mut tx, workspace_id, &input.invoice_id).await?;
    sqlx::query("INSERT INTO cash_invoice_issues(invoice_id,workspace_id,origin_id,facts_json,assessment_json,context_json,request_json,idempotency_key,result_json,net_minor) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)").bind(&input.invoice_id).bind(workspace_id).bind(&evaluation.origin.id).bind(encode(&input.facts)?).bind(encode(&evaluation.preflight.assessment)?).bind(encode(&evaluation.context)?).bind(encode(&request(input)?)?).bind(key).bind(encode(&invoice)?).bind(invoice.total_ex_vat_minor).execute(&mut *tx).await?;
    super::record_idempotency_tx(
        &mut tx,
        workspace_id,
        "invoice_cash_issue",
        key,
        &invoice,
        None,
    )
    .await?;
    crate::audit::record_event_tx(&mut *tx,workspace_id,"invoice_issue","invoice",Some(&input.invoice_id),&encode(&json!({"invoiceNumber":invoice_number,"idempotencyKey":key,"voucherId":null,"pdfJobId":pdf_job_id,"originId":evaluation.origin.id,"validationId":CASH_V1.id,"evidenceDigest":CASH_V1.digest(),"reviewFingerprint":input.review_fingerprint,"businessName":snapshot.business_name,"ownerName":snapshot.owner_name,"taxStatus":snapshot.tax_status,"vatStatus":snapshot.vat_status,"ruleVersionId":snapshot.rule_version_id,"taxYear":snapshot.tax_year,"sourceUrl":snapshot.source_url}))?).await?;
    // Intentionally no call to post_invoice_voucher_tx or any journal writer.
    tx.commit().await?;
    Ok(invoice)
}

pub async fn cash_business_origin_get(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Option<CashBusinessOrigin>, AppError> {
    sqlx::query("SELECT id,facts_json,first_invoice_id,recorded_at FROM cash_business_origins WHERE workspace_id=?1").bind(workspace_id).fetch_optional(pool).await?.map(|r|decode_origin(&r,workspace_id)).transpose()
}

pub async fn invoice_cash_history_get(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &CashHistoryGetInput,
) -> Result<Option<CashInvoiceHistory>, AppError> {
    let mut tx = pool.begin().await?;
    let history = history_tx(&mut tx, workspace_id, &input.invoice_id).await?;
    tx.commit().await?;
    Ok(history)
}

async fn history_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<Option<CashInvoiceHistory>, AppError> {
    let input = CashHistoryGetInput {
        invoice_id: invoice_id.into(),
    };
    let row=sqlx::query("SELECT c.facts_json,c.assessment_json,c.context_json,c.recorded_at,c.origin_id,i.issue_date,i.status FROM cash_invoice_issues c JOIN invoices i ON i.workspace_id=c.workspace_id AND i.id=c.invoice_id WHERE c.workspace_id=?1 AND c.invoice_id=?2").bind(workspace_id).bind(&input.invoice_id).fetch_optional(&mut **tx).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let origin = origin_tx(tx, workspace_id)
        .await?
        .ok_or_else(|| blocked("Cash issue origin missing"))?;
    let facts: CashIssueFacts = decode(&row.try_get::<String, _>("facts_json")?)?;
    let stored: CashIssueAssessment = decode(&row.try_get::<String, _>("assessment_json")?)?;
    let context: Value = decode(&row.try_get::<String, _>("context_json")?)?;
    policy::validate_facts(&facts, &stored.issued_on, &origin.facts)?;
    let expected = assessment(
        &stored.issued_on,
        facts.service_completed_on.as_deref().unwrap(),
        &origin.id,
        hash(encode(&context)?.as_bytes()),
    );
    if value(&stored)? != value(&expected)?
        || row.try_get::<String, _>("origin_id")? != origin.id
        || row.try_get::<Option<String>, _>("issue_date")?.as_deref()
            != Some(stored.issued_on.as_str())
        || row.try_get::<String, _>("status")? == "draft"
        || context["workspaceId"] != workspace_id
        || context["invoiceId"] != input.invoice_id
        || context["facts"] != value(&facts)?
        || context["originId"] != origin.id
        || context["originFacts"] != value(&origin.facts)?
        || context["firstInvoiceId"] != origin.first_invoice_id
        || context["validationId"] != CASH_V1.id
        || context["evidenceDigest"] != CASH_V1.digest()
        || context["issueDate"] != stored.issued_on
    {
        return Err(blocked("Retained cash issue provenance is inconsistent"));
    }
    let issue = CashIssueRecord {
        invoice_id: input.invoice_id.clone(),
        facts,
        assessment: stored,
        recorded_at: row.try_get("recorded_at")?,
    };
    receipt::validate_issue_snapshot_tx(tx, workspace_id, &input.invoice_id, &context).await?;
    let receipt = receipt::history_receipt_tx(
        tx,
        workspace_id,
        &input.invoice_id,
        &origin,
        &issue,
        &context,
    )
    .await?;
    Ok(Some(CashInvoiceHistory {
        invoice_id: input.invoice_id.clone(),
        origin,
        issue,
        receipt,
    }))
}

/// Legacy invoice-timed routes must consult immutable issued identity, not today's profile.
pub(crate) async fn ensure_not_cash_issue<'e, E>(
    executor: E,
    workspace: &str,
    invoice: &str,
) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let cash: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM cash_invoice_issues WHERE workspace_id=?1 AND invoice_id=?2)",
    )
    .bind(workspace)
    .bind(invoice)
    .fetch_one(executor)
    .await?;
    if cash {
        return Err(AppError::validation(
            "Issued cash identity cannot use invoice-timed settlement or credits",
            "accountingMethod",
        ));
    }
    Ok(())
}

pub async fn invoice_cash_receipt_record(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &CashReceiptRecordInput,
) -> Result<ReconciliationMatchResult, AppError> {
    receipt::record(pool, workspace_id, input).await
}
