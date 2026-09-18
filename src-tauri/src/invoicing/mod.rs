use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    audit::{record_event, record_event_tx},
    documents,
    error::AppError,
    ledger::{
        net_revenue_minor_for_fiscal_year, post_invoice_voucher_tx, post_reversal_voucher_tx,
        vat_buckets_from_rate_lines, VatBucket,
    },
    profiles::get_vat_profile,
    rules::{get_active_rule_version_for_year, require_rule_i64, RuleVersionSummary},
    workspace::{ensure_fiscal_year_open_tx, fiscal_year_id_for_date},
};

const JOB_INVOICE_ISSUE: &str = "invoice_issue";
const JOB_INVOICE_CREDIT: &str = "invoice_credit";

pub mod pdf;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLineInput {
    pub description: String,
    pub quantity: i64,
    pub unit_price_minor: i64,
    pub vat_rate: f64,
    pub account_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLine {
    pub id: String,
    pub line_order: i64,
    pub description: String,
    pub quantity: i64,
    pub unit_price_minor: i64,
    pub vat_rate_bp: i64,
    pub account_number: String,
    pub line_ex_vat_minor: i64,
    pub line_vat_minor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceSummary {
    pub id: String,
    pub counterparty_id: String,
    pub counterparty_name: String,
    pub status: String,
    pub invoice_kind: String,
    pub invoice_number: Option<String>,
    pub source_invoice_id: Option<String>,
    pub issue_date: Option<String>,
    pub due_date: Option<String>,
    pub total_ex_vat_minor: i64,
    pub total_vat_minor: i64,
    pub total_inc_vat_minor: i64,
    pub pdf_job_id: Option<String>,
    pub pdf_document_id: Option<String>,
    pub voucher_id: Option<String>,
    pub payment_voucher_id: Option<String>,
    pub lines: Vec<InvoiceLine>,
}

const INVOICE_SUMMARY_SELECT: &str = r#"
        SELECT i.id, i.counterparty_id, c.name AS counterparty_name, i.status, i.invoice_kind,
               i.invoice_number, i.source_invoice_id, i.issue_date, i.due_date,
               i.total_ex_vat_minor, i.total_vat_minor, i.total_inc_vat_minor,
               i.pdf_job_id, i.pdf_document_id, i.voucher_id,
               (
                 SELECT rm.voucher_id FROM reconciliation_matches rm
                 WHERE rm.workspace_id = i.workspace_id AND rm.invoice_id = i.id
                 LIMIT 1
               ) AS payment_voucher_id
        FROM invoices i
        JOIN counterparties c ON c.id = i.counterparty_id
"#;

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceCreateDraftInput {
    pub counterparty_id: String,
    pub due_date: Option<String>,
    pub lines: Vec<InvoiceLineInput>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceUpdateDraftInput {
    pub invoice_id: String,
    pub due_date: Option<String>,
    pub lines: Vec<InvoiceLineInput>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceIssueInput {
    pub invoice_id: String,
    pub idempotency_key: String,
    pub issue_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceCreditInput {
    pub source_invoice_id: String,
    pub idempotency_key: String,
    pub reason: Option<String>,
    pub issue_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceIssuePreflightInput {
    pub invoice_id: String,
    pub issue_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceIssuePreflight {
    pub invoice_id: String,
    pub issue_date: String,
    pub current_turnover_minor: i64,
    pub projected_turnover_minor: i64,
    pub threshold_minor: Option<i64>,
    pub requires_vat_treatment_review: bool,
    pub next_action: Option<String>,
    pub rule_version_id: Option<String>,
    pub tax_year: i32,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceListInput {
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoicePdfStatusInput {
    pub invoice_id: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyIssuedInvoiceSnapshotRecoveryInput {
    pub invoice_id: String,
    pub document_id: String,
    pub business_name: String,
    pub owner_name: String,
    pub tax_status: String,
    pub vat_status: String,
    pub rule_version_id: String,
    pub attestation: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyIssuedInvoiceSnapshotRecoveryStatus {
    pub invoice_id: String,
    pub recovery_required: bool,
    pub preserved_pdf_document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditIdempotencyRequest {
    source_invoice_id: String,
    issue_date: String,
    reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentInvoicePayload {
    idempotency_key: String,
    invoice: InvoiceSummary,
    #[serde(default)]
    credit_request: Option<CreditIdempotencyRequest>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IssuedInvoiceSnapshot {
    pub business_name: String,
    pub owner_name: String,
    pub tax_status: String,
    pub vat_status: String,
    pub rule_version_id: String,
    pub tax_year: i32,
    pub source_url: String,
}

fn vat_rate_to_bp(rate: f64) -> i64 {
    (rate * 10_000.0).round() as i64
}

fn line_amounts(quantity: i64, unit_price_minor: i64, vat_rate_bp: i64) -> (i64, i64) {
    let ex_vat = quantity.saturating_mul(unit_price_minor);
    let vat = (ex_vat.saturating_mul(vat_rate_bp) + 5_000) / 10_000;
    (ex_vat, vat)
}

fn validate_lines(lines: &[InvoiceLineInput]) -> Result<(), AppError> {
    if lines.is_empty() {
        return Err(AppError::validation("At least one invoice line is required", "lines"));
    }
    for line in lines {
        if line.description.trim().is_empty() {
            return Err(AppError::validation("Line description is required", "lines"));
        }
        if line.quantity <= 0 {
            return Err(AppError::validation("Line quantity must be positive", "lines"));
        }
        if line.unit_price_minor <= 0 {
            return Err(AppError::validation("Line unit price must be positive", "lines"));
        }
        if line.vat_rate < 0.0 || line.vat_rate > 1.0 {
            return Err(AppError::validation("VAT rate must be between 0 and 1", "lines"));
        }
    }
    Ok(())
}

fn resolve_accounting_date(
    provided_date: Option<&str>,
    field: &str,
) -> Result<String, AppError> {
    let candidate = match provided_date {
        Some(date) => date.trim().to_string(),
        None => Utc::now().format("%Y-%m-%d").to_string(),
    };
    let date = NaiveDate::parse_from_str(&candidate, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", field))?;
    Ok(date.format("%Y-%m-%d").to_string())
}

fn credit_idempotency_request(
    input: &InvoiceCreditInput,
    issue_date: String,
) -> CreditIdempotencyRequest {
    CreditIdempotencyRequest {
        source_invoice_id: input.source_invoice_id.clone(),
        issue_date,
        reason: input.reason.as_ref().map(|reason| reason.trim().to_string()),
    }
}

async fn vat_threshold_rule(
    pool: &SqlitePool,
    issue_date: &str,
) -> Result<(RuleVersionSummary, i64), AppError> {
    let tax_year = NaiveDate::parse_from_str(issue_date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", "issueDate"))?
        .year();
    let rule_version = get_active_rule_version_for_year(pool, tax_year)
        .await?
        .ok_or_else(|| AppError::validation("No active rule version for issue date", "ruleVersion"))?;
    let threshold_minor = require_rule_i64(
        pool,
        "vat",
        "annual_turnover_threshold_minor",
        tax_year,
    )
    .await?;
    Ok((rule_version, threshold_minor))
}

async fn vat_threshold_rule_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    issue_date: &str,
) -> Result<(RuleVersionSummary, i64), AppError> {
    let tax_year = NaiveDate::parse_from_str(issue_date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", "issueDate"))?
        .year();
    let row = sqlx::query(
        r#"
        SELECT rv.id, rv.tax_year, rv.source_url, rv.status, tr.value_json
        FROM rule_versions rv
        JOIN tax_rules tr ON tr.rule_version_id = rv.id
        WHERE rv.status = 'active'
          AND rv.tax_year = ?1
          AND tr.family = 'vat'
          AND tr.key = 'annual_turnover_threshold_minor'
        LIMIT 1
        "#,
    )
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        AppError::validation(
            "Active tax rule configuration is missing or invalid",
            "ruleVersion",
        )
    })?;
    let threshold_minor = serde_json::from_str::<i64>(&row.get::<String, _>("value_json"))
        .map_err(|_| {
            AppError::validation(
                "Active tax rule configuration is missing or invalid",
                "ruleVersion",
            )
        })?;
    Ok((
        RuleVersionSummary {
            id: row.get("id"),
            tax_year: row.get("tax_year"),
            source_url: row.get("source_url"),
            status: row.get("status"),
        },
        threshold_minor,
    ))
}

fn build_invoice_issue_preflight(
    invoice_id: String,
    issue_date: String,
    current_turnover_minor: i64,
    invoice_total_ex_vat_minor: i64,
    vat_status: &str,
    total_vat_minor: i64,
    rule_version: Option<RuleVersionSummary>,
    threshold_minor: Option<i64>,
    tax_year: i32,
) -> Result<InvoiceIssuePreflight, AppError> {
    let projected_turnover_minor = current_turnover_minor
        .checked_add(invoice_total_ex_vat_minor)
        .ok_or_else(|| AppError::validation("Projected turnover is out of range", "lines"))?;
    let requires_vat_treatment_review = vat_status == "exempt_low_turnover"
        && total_vat_minor == 0
        && threshold_minor.is_some_and(|threshold| projected_turnover_minor > threshold);
    Ok(InvoiceIssuePreflight {
        invoice_id,
        issue_date,
        current_turnover_minor,
        projected_turnover_minor,
        threshold_minor,
        requires_vat_treatment_review,
        next_action: requires_vat_treatment_review.then(|| {
            "Review VAT registration and VAT treatment before issuing this invoice".to_string()
        }),
        rule_version_id: rule_version.as_ref().map(|rule| rule.id.clone()),
        tax_year,
        source_url: rule_version.map(|rule| rule.source_url),
    })
}

async fn net_revenue_minor_for_fiscal_year_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    fiscal_year_id: &str,
) -> Result<i64, AppError> {
    let turnover: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.credit_minor - jl.debit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        JOIN accounts a ON a.id = jl.account_id
        WHERE v.workspace_id = ?1
          AND v.fiscal_year_id = ?2
          AND v.status = 'posted'
          AND a.number = '3041'
        "#,
    )
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(turnover)
}

async fn ensure_counterparty_in_workspace(
    pool: &SqlitePool,
    workspace_id: &str,
    counterparty_id: &str,
) -> Result<(), AppError> {
    let exists: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM counterparties
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(counterparty_id)
    .fetch_optional(pool)
    .await?;

    if exists.is_none() {
        return Err(AppError::validation("Counterparty not found", "counterpartyId"));
    }
    Ok(())
}

async fn validate_vat_lines(
    pool: &SqlitePool,
    workspace_id: &str,
    lines: &[InvoiceLineInput],
) -> Result<(), AppError> {
    let status = get_vat_profile(pool, workspace_id)
        .await?
        .ok_or_else(|| AppError::validation("VAT profile is required before issuing", "vatProfile"))?
        .vat_status;

    let charges_vat = lines.iter().any(|line| line.vat_rate > 0.0);
    if status == "exempt_low_turnover" && charges_vat {
        return Err(AppError::validation(
            "VAT-exempt profile cannot issue VAT-charging invoices",
            "vatStatus",
        ));
    }
    Ok(())
}

async fn validate_issue_tax_status_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
) -> Result<String, AppError> {
    let tax_status: Option<String> =
        sqlx::query_scalar("SELECT tax_status FROM tax_profiles WHERE workspace_id = ?1 LIMIT 1")
            .bind(workspace_id)
            .fetch_optional(&mut **tx)
            .await?;
    let tax_status = tax_status
        .ok_or_else(|| AppError::validation("Tax profile is required before issuing", "taxProfile"))?;

    if !matches!(tax_status.as_str(), "f_skatt" | "fa_skatt") {
        return Err(AppError::validation(
            "Approved F-skatt or FA-skatt status is required before issuing",
            "taxStatus",
        ));
    }
    Ok(tax_status)
}

async fn active_rule_provenance_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    issue_date: &str,
) -> Result<(i32, RuleVersionSummary), AppError> {
    let tax_year = NaiveDate::parse_from_str(issue_date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", "issueDate"))?
        .year();
    let rule_version = sqlx::query(
        r#"
        SELECT id, tax_year, source_url, status
        FROM rule_versions
        WHERE status = 'active' AND tax_year = ?1
        LIMIT 1
        "#,
    )
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await?
    .map(|row| RuleVersionSummary {
        id: row.get("id"),
        tax_year: row.get("tax_year"),
        source_url: row.get("source_url"),
        status: row.get("status"),
    })
    .ok_or_else(|| {
        AppError::validation(
            "No active rule version for issue date",
            "ruleVersion",
        )
    })?;
    Ok((tax_year, rule_version))
}

async fn capture_issued_invoice_snapshot_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
    issue_date: &str,
    tax_status: String,
    vat_status: String,
) -> Result<IssuedInvoiceSnapshot, AppError> {
    let business = sqlx::query(
        r#"
        SELECT business_name, owner_name
        FROM sole_trader_profiles
        WHERE workspace_id = ?1
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::validation("Business profile is required before issuing", "businessProfile"))?;
    let (tax_year, rule_version) = active_rule_provenance_tx(tx, issue_date).await?;
    let snapshot = IssuedInvoiceSnapshot {
        business_name: business.get("business_name"),
        owner_name: business.get("owner_name"),
        tax_status,
        vat_status,
        rule_version_id: rule_version.id,
        tax_year,
        source_url: rule_version.source_url,
    };
    insert_issued_invoice_snapshot_tx(tx, workspace_id, invoice_id, &snapshot).await?;
    Ok(snapshot)
}

async fn insert_issued_invoice_snapshot_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
    snapshot: &IssuedInvoiceSnapshot,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO invoice_issue_snapshots (
          invoice_id, workspace_id, business_name, owner_name, tax_status, vat_status,
          rule_version_id, tax_year, source_url
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(invoice_id)
    .bind(workspace_id)
    .bind(&snapshot.business_name)
    .bind(&snapshot.owner_name)
    .bind(&snapshot.tax_status)
    .bind(&snapshot.vat_status)
    .bind(&snapshot.rule_version_id)
    .bind(snapshot.tax_year)
    .bind(&snapshot.source_url)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn copy_issued_invoice_snapshot_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    source_invoice_id: &str,
    invoice_id: &str,
) -> Result<IssuedInvoiceSnapshot, AppError> {
    let snapshot = sqlx::query(
        r#"
        SELECT business_name, owner_name, tax_status, vat_status, rule_version_id, tax_year, source_url
        FROM invoice_issue_snapshots
        WHERE workspace_id = ?1 AND invoice_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(source_invoice_id)
    .fetch_optional(&mut **tx)
    .await?
    .map(|row| IssuedInvoiceSnapshot {
        business_name: row.get("business_name"),
        owner_name: row.get("owner_name"),
        tax_status: row.get("tax_status"),
        vat_status: row.get("vat_status"),
        rule_version_id: row.get("rule_version_id"),
        tax_year: row.get("tax_year"),
        source_url: row.get("source_url"),
    })
    .ok_or_else(|| {
        AppError::validation(
            "Issued invoice snapshot is missing; explicit recovery is required before crediting",
            "sourceInvoiceId",
        )
    })?;
    insert_issued_invoice_snapshot_tx(tx, workspace_id, invoice_id, &snapshot).await?;
    Ok(snapshot)
}

pub async fn get_issued_invoice_snapshot(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<IssuedInvoiceSnapshot, AppError> {
    sqlx::query(
        r#"
        SELECT business_name, owner_name, tax_status, vat_status, rule_version_id, tax_year, source_url
        FROM invoice_issue_snapshots
        WHERE workspace_id = ?1 AND invoice_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_optional(pool)
    .await?
    .map(|row| IssuedInvoiceSnapshot {
        business_name: row.get("business_name"),
        owner_name: row.get("owner_name"),
        tax_status: row.get("tax_status"),
        vat_status: row.get("vat_status"),
        rule_version_id: row.get("rule_version_id"),
        tax_year: row.get("tax_year"),
        source_url: row.get("source_url"),
    })
    .ok_or_else(|| {
        AppError::validation(
            "Issued invoice snapshot is missing; explicit recovery is required before PDF generation",
            "invoiceId",
        )
    })
}

fn invoice_snapshot_from_row(row: sqlx::sqlite::SqliteRow) -> IssuedInvoiceSnapshot {
    IssuedInvoiceSnapshot {
        business_name: row.get("business_name"),
        owner_name: row.get("owner_name"),
        tax_status: row.get("tax_status"),
        vat_status: row.get("vat_status"),
        rule_version_id: row.get("rule_version_id"),
        tax_year: row.get("tax_year"),
        source_url: row.get("source_url"),
    }
}

async fn issued_invoice_snapshot_exists(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<bool, AppError> {
    let exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1 FROM invoice_issue_snapshots
        WHERE workspace_id = ?1 AND invoice_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_optional(pool)
    .await?;
    Ok(exists.is_some())
}

async fn record_legacy_snapshot_recovery_requirement_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<(), AppError> {
    let document = sqlx::query(
        r#"
        SELECT d.id, d.content_sha256
        FROM invoices i
        LEFT JOIN documents d
          ON d.workspace_id = i.workspace_id
         AND d.id = i.pdf_document_id
        WHERE i.workspace_id = ?1 AND i.id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "invoiceId"))?;
    let document_id: Option<String> = document.get("id");
    let content_sha256: Option<String> = document.get("content_sha256");
    let inserted = sqlx::query(
        r#"
        INSERT OR IGNORE INTO invoice_snapshot_recovery_requirements (
          invoice_id, workspace_id, preserved_pdf_document_id, preserved_pdf_content_sha256
        ) VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(invoice_id)
    .bind(workspace_id)
    .bind(&document_id)
    .bind(&content_sha256)
    .execute(&mut **tx)
    .await?;
    if inserted.rows_affected() == 1 {
        record_event_tx(
            &mut **tx,
            workspace_id,
            "invoice_snapshot_recovery_required",
            "invoice",
            Some(invoice_id),
            &serde_json::json!({
                "preservedPdfDocumentId": document_id,
                "preservedPdfContentSha256": content_sha256,
            })
            .to_string(),
        )
        .await?;
    }
    Ok(())
}

pub async fn record_legacy_issued_invoice_snapshot_recovery_requirement(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<(), AppError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    record_legacy_snapshot_recovery_requirement_tx(&mut tx, workspace_id, invoice_id).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn legacy_issued_invoice_snapshot_recovery_status(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<LegacyIssuedInvoiceSnapshotRecoveryStatus, AppError> {
    let invoice = get_invoice(pool, workspace_id, invoice_id).await?;
    let recovery_required =
        matches!(invoice.status.as_str(), "issued" | "credited")
            && !issued_invoice_snapshot_exists(pool, workspace_id, invoice_id).await?;
    let preserved_pdf_document_id = if recovery_required {
        sqlx::query_scalar(
            r#"
            SELECT preserved_pdf_document_id
            FROM invoice_snapshot_recovery_requirements
            WHERE workspace_id = ?1 AND invoice_id = ?2
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(invoice_id)
        .fetch_optional(pool)
        .await?
        .flatten()
        .or(invoice.pdf_document_id)
    } else {
        None
    };
    Ok(LegacyIssuedInvoiceSnapshotRecoveryStatus {
        invoice_id: invoice.id,
        recovery_required,
        preserved_pdf_document_id,
    })
}

fn recovery_snapshot(
    input: &LegacyIssuedInvoiceSnapshotRecoveryInput,
    rule_version_id: String,
    tax_year: i32,
    source_url: String,
) -> Result<IssuedInvoiceSnapshot, AppError> {
    let snapshot = IssuedInvoiceSnapshot {
        business_name: input.business_name.trim().to_string(),
        owner_name: input.owner_name.trim().to_string(),
        tax_status: input.tax_status.trim().to_string(),
        vat_status: input.vat_status.trim().to_string(),
        rule_version_id,
        tax_year,
        source_url,
    };
    if snapshot.business_name.is_empty() {
        return Err(AppError::validation(
            "Historical business identity is required for recovery",
            "businessName",
        ));
    }
    if !matches!(snapshot.tax_status.as_str(), "f_skatt" | "fa_skatt") {
        return Err(AppError::validation(
            "Historical recovery requires F-skatt or FA-skatt status",
            "taxStatus",
        ));
    }
    if !matches!(
        snapshot.vat_status.as_str(),
        "registered" | "voluntary_registered" | "exempt_low_turnover"
    ) {
        return Err(AppError::validation(
            "Historical recovery has an unsupported VAT status",
            "vatStatus",
        ));
    }
    Ok(snapshot)
}

pub async fn recover_legacy_issued_invoice_snapshot(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &LegacyIssuedInvoiceSnapshotRecoveryInput,
) -> Result<(), AppError> {
    const ATTESTATION: &str =
        "I attest that the business identity and displayed tax/VAT wording were transcribed from the retained original invoice PDF, and that any status distinctions and the rule version were checked against contemporaneous records.";
    if input.attestation.trim() != ATTESTATION {
        return Err(AppError::validation(
            "Recovery requires attestation that PDF-visible wording, contemporaneous status records, and the rule version were reviewed",
            "attestation",
        ));
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let invoice: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT pdf_document_id, issue_date FROM invoices
        WHERE workspace_id = ?1 AND id = ?2 AND status IN ('issued', 'credited')
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&input.invoice_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((linked_document, issue_date)) = invoice else {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Recovery requires a retained historical PDF on an issued or credited invoice",
            "invoiceId",
        ));
    };
    if linked_document.as_deref() != Some(input.document_id.trim()) {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Recovery must use the invoice's retained historical PDF",
            "documentId",
        ));
    }
    let issue_date = issue_date.ok_or_else(|| {
        AppError::validation("Issued invoice is missing issue date", "invoiceId")
    })?;
    let issue_year = NaiveDate::parse_from_str(&issue_date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Issued invoice has an invalid issue date", "invoiceId"))?
        .year();
    let rule_version_id = input.rule_version_id.trim();
    if rule_version_id.is_empty() {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Historical rule version is required for recovery",
            "ruleVersionId",
        ));
    }
    let source_url: Option<String> = sqlx::query_scalar(
        r#"
        SELECT source_url FROM rule_versions
        WHERE id = ?1 AND tax_year = ?2
        LIMIT 1
        "#,
    )
    .bind(rule_version_id)
    .bind(issue_year)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(source_url) = source_url else {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Historical rule version does not match the invoice issue year",
            "ruleVersionId",
        ));
    };
    let snapshot = recovery_snapshot(
        input,
        rule_version_id.to_string(),
        issue_year,
        source_url,
    )?;
    record_legacy_snapshot_recovery_requirement_tx(&mut tx, workspace_id, &input.invoice_id).await?;
    let requirement: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT preserved_pdf_document_id, preserved_pdf_content_sha256
        FROM invoice_snapshot_recovery_requirements
        WHERE workspace_id = ?1 AND invoice_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&input.invoice_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((preserved_document_id, preserved_content_sha256)) = requirement else {
        tx.rollback().await?;
        return Err(AppError::internal("Invoice snapshot recovery requirement was not recorded"));
    };
    if preserved_document_id.as_deref() != Some(input.document_id.trim()) {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Recovery document does not match the preserved historical PDF",
            "documentId",
        ));
    }
    let document = match documents::verify_retained_document_tx(
        &mut tx,
        workspace_id,
        input.document_id.trim(),
    )
    .await
    {
        Ok(document) => document,
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(error);
        }
    };
    if !documents::is_pdf_mime(&document.mime_type) {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Recovery document must be a PDF",
            "documentId",
        ));
    }
    let content_sha256 = document.content_sha256;
    if preserved_content_sha256.as_deref() != Some(content_sha256.as_str()) {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Recovery document bytes do not match the preserved historical PDF",
            "documentId",
        ));
    }
    let existing = sqlx::query(
        r#"
        SELECT business_name, owner_name, tax_status, vat_status, rule_version_id, tax_year, source_url
        FROM invoice_issue_snapshots
        WHERE workspace_id = ?1 AND invoice_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&input.invoice_id)
    .fetch_optional(&mut *tx)
    .await?
    .map(invoice_snapshot_from_row);
    if let Some(existing) = existing {
        tx.rollback().await?;
        return if existing == snapshot {
            Ok(())
        } else {
            Err(AppError::validation(
                "Invoice already has a different immutable issuance snapshot",
                "invoiceId",
            ))
        };
    }
    insert_issued_invoice_snapshot_tx(&mut tx, workspace_id, &input.invoice_id, &snapshot).await?;
    record_event_tx(
        &mut *tx,
        workspace_id,
        "invoice_snapshot_recovered",
        "invoice",
        Some(&input.invoice_id),
        &serde_json::json!({
            "documentId": input.document_id.trim(),
            "documentSha256": content_sha256,
            "businessName": snapshot.business_name,
            "ownerName": snapshot.owner_name,
            "taxStatus": snapshot.tax_status,
            "vatStatus": snapshot.vat_status,
            "ruleVersionId": snapshot.rule_version_id,
            "taxYear": snapshot.tax_year,
            "sourceUrl": snapshot.source_url,
        })
        .to_string(),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

fn vat_buckets_from_source_invoice(
    source: &InvoiceSummary,
) -> Result<Vec<VatBucket>, AppError> {
    let (line_ex_vat_minor, line_vat_minor) =
        source
            .lines
            .iter()
            .fold((0_i64, 0_i64), |totals, line| {
                (
                    totals.0.saturating_add(line.line_ex_vat_minor),
                    totals.1.saturating_add(line.line_vat_minor),
                )
            });
    if line_ex_vat_minor != source.total_ex_vat_minor
        || line_vat_minor != source.total_vat_minor
        || line_ex_vat_minor.saturating_add(line_vat_minor) != source.total_inc_vat_minor
    {
        return Err(AppError::validation(
            "Source invoice VAT treatment is inconsistent",
            "sourceInvoiceId",
        ));
    }
    vat_buckets_from_rate_lines(
        source
            .lines
            .iter()
            .map(|line| (line.quantity, line.unit_price_minor, line.vat_rate_bp)),
    )
}

pub async fn invoice_issue_preflight(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &InvoiceIssuePreflightInput,
) -> Result<InvoiceIssuePreflight, AppError> {
    let invoice = get_invoice(pool, workspace_id, &input.invoice_id).await?;
    if invoice.status != "draft" || invoice.invoice_kind != "standard" {
        return Err(AppError::validation(
            "Only standard draft invoices can be preflighted",
            "invoiceId",
        ));
    }
    let issue_date = resolve_accounting_date(input.issue_date.as_deref(), "issueDate")?;
    let tax_year = NaiveDate::parse_from_str(&issue_date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", "issueDate"))?
        .year();
    let fiscal_year_id = format!("fy-{workspace_id}-{tax_year}");
    let current_turnover_minor =
        net_revenue_minor_for_fiscal_year(pool, workspace_id, &fiscal_year_id).await?;
    let vat_status = get_vat_profile(pool, workspace_id)
        .await?
        .ok_or_else(|| AppError::validation("VAT profile is required before issuing", "vatProfile"))?
        .vat_status;
    let (rule_version, threshold_minor) =
        if vat_status == "exempt_low_turnover" && invoice.total_vat_minor == 0 {
            let (rule_version, threshold_minor) = vat_threshold_rule(pool, &issue_date).await?;
            (Some(rule_version), Some(threshold_minor))
        } else {
            (None, None)
        };

    build_invoice_issue_preflight(
        invoice.id,
        issue_date,
        current_turnover_minor,
        invoice.total_ex_vat_minor,
        &vat_status,
        invoice.total_vat_minor,
        rule_version,
        threshold_minor,
        tax_year,
    )
}

use crate::idempotency::normalize_idempotency_key;

fn validate_issue_idempotency_match(
    invoice_id: &str,
    cached: &InvoiceSummary,
) -> Result<(), AppError> {
    if cached.id != invoice_id {
        return Err(AppError::validation(
            "Idempotency key was already used for a different invoice",
            "idempotencyKey",
        ));
    }
    Ok(())
}

fn validate_credit_idempotency_match(
    request: &CreditIdempotencyRequest,
    cached: &IdempotentInvoicePayload,
) -> Result<(), AppError> {
    let cached_request = cached.credit_request.as_ref().ok_or_else(|| {
        AppError::validation(
            "Idempotency replay requires the original credit request details",
            "idempotencyKey",
        )
    })?;

    if cached_request.source_invoice_id != request.source_invoice_id {
        return Err(AppError::validation(
            "Idempotency key was already used for a different source invoice",
            "idempotencyKey",
        ));
    }
    if cached_request.issue_date != request.issue_date {
        return Err(AppError::validation(
            "Idempotency key was already used with a different credit issue date",
            "issueDate",
        ));
    }
    if cached_request.reason != request.reason {
        return Err(AppError::validation(
            "Idempotency key was already used with a different correction reason",
            "reason",
        ));
    }
    Ok(())
}

async fn insert_lines(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    invoice_id: &str,
    lines: &[InvoiceLineInput],
) -> Result<(i64, i64), AppError> {
    let mut total_ex = 0i64;
    let mut total_vat = 0i64;

    for (index, line) in lines.iter().enumerate() {
        let vat_rate_bp = vat_rate_to_bp(line.vat_rate);
        let (ex_vat, vat) = line_amounts(line.quantity, line.unit_price_minor, vat_rate_bp);
        total_ex += ex_vat;
        total_vat += vat;

        sqlx::query(
            r#"
            INSERT INTO invoice_lines (
              id, invoice_id, line_order, description, quantity, unit_price_minor,
              vat_rate_bp, account_number
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(invoice_id)
        .bind((index + 1) as i64)
        .bind(line.description.trim())
        .bind(line.quantity)
        .bind(line.unit_price_minor)
        .bind(vat_rate_bp)
        .bind(line.account_number.as_deref().unwrap_or("3041"))
        .execute(&mut **tx)
        .await?;
    }

    Ok((total_ex, total_vat))
}

async fn fetch_invoice_summary_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<InvoiceSummary, AppError> {
    let row = sqlx::query(&format!(
        "{INVOICE_SUMMARY_SELECT}
        WHERE i.workspace_id = ?1 AND i.id = ?2
        LIMIT 1"
    ))
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "invoiceId"))?;

    let lines_rows = sqlx::query(
        r#"
        SELECT id, line_order, description, quantity, unit_price_minor, vat_rate_bp, account_number
        FROM invoice_lines
        WHERE invoice_id = ?1
        ORDER BY line_order ASC
        "#,
    )
    .bind(invoice_id)
    .fetch_all(&mut **tx)
    .await?;

    let lines = lines_rows
        .into_iter()
        .map(|line_row| {
            let quantity: i64 = line_row.get("quantity");
            let unit_price_minor: i64 = line_row.get("unit_price_minor");
            let vat_rate_bp: i64 = line_row.get("vat_rate_bp");
            let (line_ex_vat_minor, line_vat_minor) =
                line_amounts(quantity, unit_price_minor, vat_rate_bp);
            InvoiceLine {
                id: line_row.get("id"),
                line_order: line_row.get("line_order"),
                description: line_row.get("description"),
                quantity,
                unit_price_minor,
                vat_rate_bp,
                account_number: line_row.get("account_number"),
                line_ex_vat_minor,
                line_vat_minor,
            }
        })
        .collect();

    Ok(map_invoice_row(row, lines))
}

async fn lines_as_input(pool: &SqlitePool, invoice_id: &str) -> Result<Vec<InvoiceLineInput>, AppError> {
    let lines = load_lines(pool, invoice_id).await?;
    Ok(lines
        .into_iter()
        .map(|line| InvoiceLineInput {
            description: line.description,
            quantity: line.quantity,
            unit_price_minor: line.unit_price_minor,
            vat_rate: line.vat_rate_bp as f64 / 10_000.0,
            account_number: Some(line.account_number),
        })
        .collect())
}

async fn lines_as_input_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    invoice_id: &str,
) -> Result<Vec<InvoiceLineInput>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT description, quantity, unit_price_minor, vat_rate_bp, account_number
        FROM invoice_lines
        WHERE invoice_id = ?1
        ORDER BY line_order ASC
        "#,
    )
    .bind(invoice_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| InvoiceLineInput {
            description: row.get("description"),
            quantity: row.get("quantity"),
            unit_price_minor: row.get("unit_price_minor"),
            vat_rate: row.get::<i64, _>("vat_rate_bp") as f64 / 10_000.0,
            account_number: Some(row.get("account_number")),
        })
        .collect())
}

async fn find_credit_invoice_by_source(
    pool: &SqlitePool,
    workspace_id: &str,
    source_invoice_id: &str,
) -> Result<Option<InvoiceSummary>, AppError> {
    let credit_invoice_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT credit_invoice_id FROM credit_notes
        WHERE workspace_id = ?1 AND source_invoice_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(source_invoice_id)
    .fetch_optional(pool)
    .await?;

    match credit_invoice_id {
        Some(id) => Ok(Some(get_invoice(pool, workspace_id, &id).await?)),
        None => Ok(None),
    }
}

async fn validate_existing_credit_request(
    pool: &SqlitePool,
    workspace_id: &str,
    request: &CreditIdempotencyRequest,
    existing: &InvoiceSummary,
) -> Result<(), AppError> {
    let source_invoice_id = existing.source_invoice_id.as_deref().ok_or_else(|| {
        AppError::internal("Existing credit invoice is missing its source invoice")
    })?;
    let issue_date = existing.issue_date.as_deref().ok_or_else(|| {
        AppError::internal("Existing credit invoice is missing its issue date")
    })?;
    let reason: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT reason
        FROM credit_notes
        WHERE workspace_id = ?1 AND credit_invoice_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&existing.id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::internal("Existing credit invoice is missing its credit note"))?;

    if source_invoice_id != request.source_invoice_id.as_str() {
        return Err(AppError::validation(
            "An existing credit note belongs to a different source invoice",
            "sourceInvoiceId",
        ));
    }
    if issue_date != request.issue_date.as_str() {
        return Err(AppError::validation(
            "An existing credit note has a different issue date",
            "issueDate",
        ));
    }
    if reason != request.reason {
        return Err(AppError::validation(
            "An existing credit note has a different correction reason",
            "reason",
        ));
    }
    Ok(())
}

pub async fn list_invoices(
    pool: &SqlitePool,
    workspace_id: &str,
    filter: &InvoiceListInput,
) -> Result<Vec<InvoiceSummary>, AppError> {
    let rows = if let Some(status) = filter.status.as_deref() {
        sqlx::query(
            r#"
            SELECT i.id
            FROM invoices i
            WHERE i.workspace_id = ?1 AND i.status = ?2
            ORDER BY i.created_at DESC
            "#,
        )
        .bind(workspace_id)
        .bind(status)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT i.id
            FROM invoices i
            WHERE i.workspace_id = ?1
            ORDER BY i.created_at DESC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(pool)
        .await?
    };

    let mut invoices = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        invoices.push(get_invoice(pool, workspace_id, &id).await?);
    }
    Ok(invoices)
}

pub async fn get_invoice(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<InvoiceSummary, AppError> {
    let row = sqlx::query(&format!(
        "{INVOICE_SUMMARY_SELECT}
        WHERE i.workspace_id = ?1 AND i.id = ?2
        LIMIT 1"
    ))
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "invoiceId"))?;

    let lines = load_lines(pool, invoice_id).await?;
    Ok(map_invoice_row(row, lines))
}

pub async fn find_invoice_by_number(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_number: &str,
) -> Result<InvoiceSummary, AppError> {
    let row = sqlx::query(&format!(
        "{INVOICE_SUMMARY_SELECT}
        WHERE i.workspace_id = ?1 AND i.invoice_number = ?2
        LIMIT 1"
    ))
    .bind(workspace_id)
    .bind(invoice_number)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "invoiceNumber"))?;

    let invoice_id: String = row.get("id");
    let lines = load_lines(pool, &invoice_id).await?;
    Ok(map_invoice_row(row, lines))
}

fn map_invoice_row(row: sqlx::sqlite::SqliteRow, lines: Vec<InvoiceLine>) -> InvoiceSummary {
    InvoiceSummary {
        id: row.get("id"),
        counterparty_id: row.get("counterparty_id"),
        counterparty_name: row.get("counterparty_name"),
        status: row.get("status"),
        invoice_kind: row.get("invoice_kind"),
        invoice_number: row.get("invoice_number"),
        source_invoice_id: row.get("source_invoice_id"),
        issue_date: row.get("issue_date"),
        due_date: row.get("due_date"),
        total_ex_vat_minor: row.get("total_ex_vat_minor"),
        total_vat_minor: row.get("total_vat_minor"),
        total_inc_vat_minor: row.get("total_inc_vat_minor"),
        pdf_job_id: row.get("pdf_job_id"),
        pdf_document_id: row.get("pdf_document_id"),
        voucher_id: row.get("voucher_id"),
        payment_voucher_id: row.get("payment_voucher_id"),
        lines,
    }
}

async fn load_lines(pool: &SqlitePool, invoice_id: &str) -> Result<Vec<InvoiceLine>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, line_order, description, quantity, unit_price_minor, vat_rate_bp, account_number
        FROM invoice_lines
        WHERE invoice_id = ?1
        ORDER BY line_order ASC
        "#,
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let quantity: i64 = row.get("quantity");
            let unit_price_minor: i64 = row.get("unit_price_minor");
            let vat_rate_bp: i64 = row.get("vat_rate_bp");
            let (line_ex_vat_minor, line_vat_minor) =
                line_amounts(quantity, unit_price_minor, vat_rate_bp);
            InvoiceLine {
                id: row.get("id"),
                line_order: row.get("line_order"),
                description: row.get("description"),
                quantity,
                unit_price_minor,
                vat_rate_bp,
                account_number: row.get("account_number"),
                line_ex_vat_minor,
                line_vat_minor,
            }
        })
        .collect())
}

pub async fn create_draft(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &InvoiceCreateDraftInput,
) -> Result<InvoiceSummary, AppError> {
    validate_lines(&input.lines)?;
    ensure_counterparty_in_workspace(pool, workspace_id, &input.counterparty_id).await?;
    validate_vat_lines(pool, workspace_id, &input.lines).await?;

    let draft_date = Utc::now().format("%Y-%m-%d").to_string();
    let fiscal_year_id = fiscal_year_id_for_date(pool, workspace_id, &draft_date).await?;
    let (total_ex, total_vat) = {
        let mut ex = 0i64;
        let mut vat = 0i64;
        for line in &input.lines {
            let bp = vat_rate_to_bp(line.vat_rate);
            let (line_ex, line_vat) = line_amounts(line.quantity, line.unit_price_minor, bp);
            ex += line_ex;
            vat += line_vat;
        }
        (ex, vat)
    };

    let invoice_id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO invoices (
          id, workspace_id, counterparty_id, fiscal_year_id, status, invoice_kind,
          due_date, total_ex_vat_minor, total_vat_minor, total_inc_vat_minor
        ) VALUES (?1, ?2, ?3, ?4, 'draft', 'standard', ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&invoice_id)
    .bind(workspace_id)
    .bind(&input.counterparty_id)
    .bind(&fiscal_year_id)
    .bind(input.due_date.as_deref())
    .bind(total_ex)
    .bind(total_vat)
    .bind(total_ex + total_vat)
    .execute(&mut *tx)
    .await?;

    insert_lines(&mut tx, &invoice_id, &input.lines).await?;
    tx.commit().await?;

    let invoice = get_invoice(pool, workspace_id, &invoice_id).await?;

    record_event(
        pool,
        workspace_id,
        "invoice_create_draft",
        "invoice",
        Some(&invoice_id),
        &serde_json::to_string(&invoice).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(invoice)
}

pub async fn update_draft(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &InvoiceUpdateDraftInput,
) -> Result<InvoiceSummary, AppError> {
    validate_lines(&input.lines)?;
    validate_vat_lines(pool, workspace_id, &input.lines).await?;

    let (total_ex, total_vat) = {
        let mut ex = 0i64;
        let mut vat = 0i64;
        for line in &input.lines {
            let bp = vat_rate_to_bp(line.vat_rate);
            let (line_ex, line_vat) = line_amounts(line.quantity, line.unit_price_minor, bp);
            ex += line_ex;
            vat += line_vat;
        }
        (ex, vat)
    };

    let mut tx = pool.begin().await?;

    let guard_result = sqlx::query(
        r#"
        UPDATE invoices
        SET updated_at = updated_at
        WHERE workspace_id = ?1 AND id = ?2 AND status = 'draft'
        "#,
    )
    .bind(workspace_id)
    .bind(&input.invoice_id)
    .execute(&mut *tx)
    .await?;

    if guard_result.rows_affected() == 0 {
        let status: Option<String> = sqlx::query_scalar(
            r#"
            SELECT status FROM invoices WHERE workspace_id = ?1 AND id = ?2 LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(&input.invoice_id)
        .fetch_optional(&mut *tx)
        .await?;

        tx.rollback().await?;
        match status {
            None => return Err(AppError::validation("Invoice not found", "invoiceId")),
            Some(_) => {
                return Err(AppError::validation(
                    "Only draft invoices can be updated",
                    "invoiceId",
                ))
            }
        }
    }

    sqlx::query(
        r#"
        DELETE FROM invoice_lines WHERE invoice_id = ?1
        "#,
    )
    .bind(&input.invoice_id)
    .execute(&mut *tx)
    .await?;

    insert_lines(&mut tx, &input.invoice_id, &input.lines).await?;

    sqlx::query(
        r#"
        UPDATE invoices
        SET due_date = ?1,
            total_ex_vat_minor = ?2,
            total_vat_minor = ?3,
            total_inc_vat_minor = ?4,
            updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?5 AND id = ?6 AND status = 'draft'
        "#,
    )
    .bind(input.due_date.as_deref())
    .bind(total_ex)
    .bind(total_vat)
    .bind(total_ex + total_vat)
    .bind(workspace_id)
    .bind(&input.invoice_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let invoice = get_invoice(pool, workspace_id, &input.invoice_id).await?;
    record_event(
        pool,
        workspace_id,
        "invoice_update_draft",
        "invoice",
        Some(&input.invoice_id),
        &serde_json::to_string(&invoice).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;
    Ok(invoice)
}

async fn check_idempotency_payload(
    pool: &SqlitePool,
    workspace_id: &str,
    job_type: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentInvoicePayload>, AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;

    let existing: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND idempotency_key = ?3
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(job_type)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let existing = if existing.is_some() {
        existing
    } else {
        sqlx::query_scalar(
            r#"
            SELECT payload_json FROM local_jobs
            WHERE workspace_id = ?1
              AND job_type = ?2
              AND json_extract(payload_json, '$.idempotencyKey') = ?3
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(job_type)
        .bind(key)
        .fetch_optional(pool)
        .await?
    };

    let Some(payload) = existing else {
        return Ok(None);
    };

    serde_json::from_str(&payload).map(Some).map_err(|error| AppError::internal(error.to_string()))
}

pub async fn check_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    job_type: &str,
    idempotency_key: &str,
) -> Result<Option<InvoiceSummary>, AppError> {
    Ok(check_idempotency_payload(pool, workspace_id, job_type, idempotency_key)
        .await?
        .map(|payload| payload.invoice))
}

pub async fn check_issue_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<InvoiceSummary>, AppError> {
    check_idempotency(pool, workspace_id, JOB_INVOICE_ISSUE, idempotency_key).await
}

pub async fn check_credit_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<InvoiceSummary>, AppError> {
    check_idempotency(pool, workspace_id, JOB_INVOICE_CREDIT, idempotency_key).await
}

async fn check_credit_idempotency_payload(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentInvoicePayload>, AppError> {
    check_idempotency_payload(pool, workspace_id, JOB_INVOICE_CREDIT, idempotency_key).await
}

async fn persist_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    job_type: &str,
    idempotency_key: &str,
    invoice: &InvoiceSummary,
    credit_request: Option<&CreditIdempotencyRequest>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    match record_idempotency_tx(
        &mut tx,
        workspace_id,
        job_type,
        idempotency_key,
        invoice,
        credit_request,
    )
    .await
    {
        Ok(()) => tx.commit().await?,
        Err(error) if error.is_unique_violation() => {
            tx.rollback().await?;
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error);
        }
    }
    Ok(())
}

async fn record_idempotency_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    job_type: &str,
    idempotency_key: &str,
    invoice: &InvoiceSummary,
    credit_request: Option<&CreditIdempotencyRequest>,
) -> Result<(), AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload = IdempotentInvoicePayload {
        idempotency_key: key.to_string(),
        invoice: invoice.clone(),
        credit_request: credit_request.cloned(),
    };
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| AppError::internal(error.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(job_type)
    .bind(payload_json)
    .bind(key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn enqueue_pdf_job_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
    invoice_number: &str,
) -> Result<String, AppError> {
    let job_id = Uuid::new_v4().to_string();
    let payload = serde_json::json!({
        "invoiceId": invoice_id,
        "invoiceNumber": invoice_number,
        "format": "pdf"
    });
    sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json)
        VALUES (?1, ?2, 'invoice_pdf_generate', 'queued', ?3)
        "#,
    )
    .bind(&job_id)
    .bind(workspace_id)
    .bind(payload.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(job_id)
}

pub async fn issue_invoice(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &InvoiceIssueInput,
) -> Result<InvoiceSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;
    if let Some(existing) =
        check_issue_idempotency(pool, workspace_id, idempotency_key).await?
    {
        validate_issue_idempotency_match(&input.invoice_id, &existing)?;
        return Ok(existing);
    }

    let invoice_status: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status FROM invoices
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&input.invoice_id)
    .fetch_optional(pool)
    .await?;

    let Some(status) = invoice_status else {
        return Err(AppError::validation("Invoice not found", "invoiceId"));
    };

    if status == "issued" {
        let invoice = get_invoice(pool, workspace_id, &input.invoice_id).await?;
        persist_idempotency(
            pool,
            workspace_id,
            JOB_INVOICE_ISSUE,
            idempotency_key,
            &invoice,
            None,
        )
        .await?;
        return Ok(invoice);
    }

    if status != "draft" {
        return Err(AppError::validation("Only draft invoices can be issued", "invoiceId"));
    }

    let draft_lines = lines_as_input(pool, &input.invoice_id).await?;
    if draft_lines.is_empty() {
        return Err(AppError::validation(
            "Draft invoice must have at least one line before issue",
            "lines",
        ));
    }
    validate_vat_lines(pool, workspace_id, &draft_lines).await?;

    let preflight = invoice_issue_preflight(
        pool,
        workspace_id,
        &InvoiceIssuePreflightInput {
            invoice_id: input.invoice_id.clone(),
            issue_date: input.issue_date.clone(),
        },
    )
    .await?;
    let issue_date = preflight.issue_date.clone();
    get_active_rule_version_for_year(pool, preflight.tax_year)
        .await?
        .ok_or_else(|| {
            AppError::validation(
                "No active rule version for issue date",
                "ruleVersion",
            )
        })?;
    let fiscal_year_id = fiscal_year_id_for_date(pool, workspace_id, &issue_date).await?;


    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let tax_status = match validate_issue_tax_status_tx(&mut tx, workspace_id).await {
        Ok(tax_status) => tax_status,
        Err(error) => {
            tx.rollback().await?;
            return Err(error);
        }
    };
    sqlx::query(
        r#"
        UPDATE invoices
        SET updated_at = updated_at
        WHERE workspace_id = ?1 AND id = ?2 AND status = 'draft'
        "#,
    )
    .bind(workspace_id)
    .bind(&input.invoice_id)
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query(
        r#"
        SELECT id, status, invoice_kind, total_ex_vat_minor, total_vat_minor
        FROM invoices
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&input.invoice_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "invoiceId"))?;

    let invoice_id: String = row.get("id");
    let status: String = row.get("status");
    let invoice_kind: String = row.get("invoice_kind");
    let total_ex_vat_minor: i64 = row.get("total_ex_vat_minor");
    let total_vat_minor: i64 = row.get("total_vat_minor");


    if status != "draft" {
        tx.rollback().await?;
        if status == "issued" {
            let invoice = get_invoice(pool, workspace_id, &invoice_id).await?;
            persist_idempotency(
                pool,
                workspace_id,
                JOB_INVOICE_ISSUE,
                idempotency_key,
                &invoice,
                None,
            )
            .await?;
            return Ok(invoice);
        }
        return Err(AppError::validation("Only draft invoices can be issued", "invoiceId"));
    }

    let locked_draft_lines = lines_as_input_tx(&mut tx, &invoice_id).await?;
    if locked_draft_lines.is_empty() {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Draft invoice must have at least one line before issue",
            "lines",
        ));
    }
    let vat_status: Option<String> = sqlx::query_scalar(
        "SELECT vat_status FROM vat_profiles WHERE workspace_id = ?1 LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(&mut *tx)
    .await?;
    let vat_status = vat_status
        .ok_or_else(|| AppError::validation("VAT profile is required before issuing", "vatProfile"))?;
    if vat_status == "exempt_low_turnover"
        && locked_draft_lines.iter().any(|line| line.vat_rate > 0.0)
    {
        tx.rollback().await?;
        return Err(AppError::validation(
            "VAT-exempt profile cannot issue VAT-charging invoices",
            "vatStatus",
        ));
    }
    let vat_buckets = vat_buckets_from_rate_lines(locked_draft_lines.iter().map(|line| {
        (
            line.quantity,
            line.unit_price_minor,
            vat_rate_to_bp(line.vat_rate),
        )
    }))?;

    if vat_status == "exempt_low_turnover" && total_vat_minor == 0 {
        let (rule_version, threshold_minor) = vat_threshold_rule_tx(&mut tx, &issue_date).await?;
        let current_turnover_minor =
            net_revenue_minor_for_fiscal_year_tx(&mut tx, workspace_id, &fiscal_year_id).await?;
        let enforced_preflight = build_invoice_issue_preflight(
            invoice_id.clone(),
            issue_date.clone(),
            current_turnover_minor,
            total_ex_vat_minor,
            &vat_status,
            total_vat_minor,
            Some(rule_version),
            Some(threshold_minor),
            preflight.tax_year,
        )?;
        if enforced_preflight.requires_vat_treatment_review {
            tx.rollback().await?;
            record_event(
                pool,
                workspace_id,
                "invoice_issue_blocked_vat_threshold",
                "invoice",
                Some(&input.invoice_id),
                &serde_json::to_string(&enforced_preflight)
                    .map_err(|error| AppError::internal(error.to_string()))?,
            )
            .await?;
            return Err(AppError::validation(
                "VAT treatment review is required before issuing this invoice",
                "vatStatus",
            ));
        }
    }

    let snapshot = capture_issued_invoice_snapshot_tx(
        &mut tx,
        workspace_id,
        &invoice_id,
        &issue_date,
        tax_status,
        vat_status,
    )
    .await?;
    ensure_fiscal_year_open_tx(&mut *tx, &fiscal_year_id).await?;

    let seq_row = sqlx::query(
        r#"
        SELECT id, prefix, next_number
        FROM invoice_sequences
        WHERE workspace_id = ?1 AND fiscal_year_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice sequence missing", "invoiceSequence"))?;

    let sequence_id: String = seq_row.get("id");
    let prefix: String = seq_row.get("prefix");
    let next_number: i64 = seq_row.get("next_number");
    let invoice_number = format!("{prefix}{next_number:04}");

    sqlx::query(
        r#"
        UPDATE invoice_sequences
        SET next_number = next_number + 1
        WHERE id = ?1
        "#,
    )
    .bind(&sequence_id)
    .execute(&mut *tx)
    .await?;

    let voucher_id = if invoice_kind == "standard" {
        Some(
            post_invoice_voucher_tx(
                &mut tx,
                workspace_id,
                &fiscal_year_id,
                &invoice_id,
                &issue_date,
                &vat_buckets,
            )
            .await?,
        )
    } else {
        None
    };

    let pdf_job_id = enqueue_pdf_job_tx(&mut tx, workspace_id, &invoice_id, &invoice_number).await?;

    let update_result = sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'issued',
            fiscal_year_id = ?1,
            invoice_number = ?2,
            issue_date = ?3,
            voucher_id = ?4,
            pdf_job_id = ?5,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?6 AND status = 'draft'
        "#,
    )
    .bind(&fiscal_year_id)
    .bind(&invoice_number)
    .bind(&issue_date)
    .bind(&voucher_id)
    .bind(&pdf_job_id)
    .bind(&invoice_id)
    .execute(&mut *tx)
    .await?;

    if update_result.rows_affected() == 0 {
        tx.rollback().await?;
        let current = get_invoice(pool, workspace_id, &invoice_id).await?;
        if current.status == "issued" {
            persist_idempotency(
                pool,
                workspace_id,
                JOB_INVOICE_ISSUE,
                idempotency_key,
                &current,
                None,
            )
            .await?;
            return Ok(current);
        }
        return Err(AppError::validation("Only draft invoices can be issued", "invoiceId"));
    }

    let invoice = fetch_invoice_summary_tx(&mut tx, workspace_id, &invoice_id).await?;

    match record_idempotency_tx(
        &mut tx,
        workspace_id,
        JOB_INVOICE_ISSUE,
        idempotency_key,
        &invoice,
        None,
    )
    .await
    {
        Ok(()) => {}
        Err(error) if error.is_unique_violation() =>
        {
            tx.rollback().await?;
            let cached = check_issue_idempotency(pool, workspace_id, idempotency_key)
                .await?
                .ok_or_else(|| AppError::internal("Idempotent issue replay failed"))?;
            validate_issue_idempotency_match(&input.invoice_id, &cached)?;
            return Ok(cached);
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error);
        }
    }

    record_event_tx(
        &mut *tx,
        workspace_id,
        "invoice_issue",
        "invoice",
        Some(&invoice_id),
        &serde_json::json!({
            "invoiceNumber": invoice_number,
            "idempotencyKey": idempotency_key,
            "voucherId": voucher_id,
            "pdfJobId": pdf_job_id,
            "businessName": snapshot.business_name,
            "ownerName": snapshot.owner_name,
            "taxStatus": snapshot.tax_status,
            "vatStatus": snapshot.vat_status,
            "ruleVersionId": snapshot.rule_version_id,
            "taxYear": snapshot.tax_year,
            "sourceUrl": snapshot.source_url
        })
        .to_string(),
    )
    .await?;

    if invoice_kind == "standard" {
        if let Some(ref voucher) = voucher_id {
            record_event_tx(
                &mut *tx,
                workspace_id,
                "voucher_post",
                "voucher",
                Some(voucher),
                &serde_json::json!({ "sourceType": "invoice", "sourceId": invoice_id }).to_string(),
            )
            .await?;
        }
    }

    tx.commit().await?;
    Ok(invoice)
}

pub async fn credit_invoice(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &InvoiceCreditInput,
) -> Result<InvoiceSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;
    let issue_date = resolve_accounting_date(input.issue_date.as_deref(), "issueDate")?;
    let credit_request = credit_idempotency_request(input, issue_date.clone());
    if let Some(cached) =
        check_credit_idempotency_payload(pool, workspace_id, idempotency_key).await?
    {
        validate_credit_idempotency_match(&credit_request, &cached)?;
        return Ok(cached.invoice);
    }

    if let Some(existing) =
        find_credit_invoice_by_source(pool, workspace_id, &credit_request.source_invoice_id).await?
    {
        validate_existing_credit_request(pool, workspace_id, &credit_request, &existing).await?;
        return Ok(existing);
    }

    let source = get_invoice(pool, workspace_id, &credit_request.source_invoice_id).await?;
    if source.invoice_kind != "standard" {
        return Err(AppError::validation("Cannot credit a credit note", "sourceInvoiceId"));
    }
    if !issued_invoice_snapshot_exists(pool, workspace_id, &source.id).await? {
        record_legacy_issued_invoice_snapshot_recovery_requirement(
            pool,
            workspace_id,
            &source.id,
        )
        .await?;
        return Err(AppError::validation(
            "Issued invoice snapshot is missing; explicit recovery is required before crediting",
            "sourceInvoiceId",
        ));
    }

    let mut date_validation_tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let source_issue_date: Option<String> = sqlx::query_scalar(
        r#"
        SELECT issue_date
        FROM invoices
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&source.id)
    .fetch_optional(&mut *date_validation_tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "sourceInvoiceId"))?;
    let source_issue_date = source_issue_date.ok_or_else(|| {
        AppError::validation("Issued invoice is missing issue date", "sourceInvoiceId")
    })?;
    let source_issue_date = NaiveDate::parse_from_str(&source_issue_date, "%Y-%m-%d").map_err(
        |_| AppError::validation("Issued invoice has an invalid issue date", "sourceInvoiceId"),
    )?;
    let credit_issue_date = NaiveDate::parse_from_str(&issue_date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", "issueDate"))?;
    if credit_issue_date < source_issue_date {
        date_validation_tx.rollback().await?;
        return Err(AppError::validation(
            "Credit issue date cannot be earlier than source invoice issue date",
            "issueDate",
        ));
    }
    date_validation_tx.commit().await?;

    let fiscal_year_id = fiscal_year_id_for_date(pool, workspace_id, &issue_date).await?;

    let source_voucher_id = source
        .voucher_id
        .clone()
        .ok_or_else(|| AppError::validation("Source invoice has no voucher", "sourceInvoiceId"))?;

    let credit_lines: Vec<InvoiceLineInput> = source
        .lines
        .iter()
        .map(|line| InvoiceLineInput {
            description: format!("Credit: {}", line.description),
            quantity: line.quantity,
            unit_price_minor: line.unit_price_minor,
            vat_rate: line.vat_rate_bp as f64 / 10_000.0,
            account_number: Some(line.account_number.clone()),
        })
        .collect();
    let vat_buckets = vat_buckets_from_source_invoice(&source)?;

    let credit_invoice_id = Uuid::new_v4().to_string();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    let source_row = sqlx::query(
        r#"
        SELECT status
        FROM invoices
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&source.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "sourceInvoiceId"))?;

    let source_status: String = source_row.get("status");

    if source_status == "credited" {
        tx.rollback().await?;
        let existing = find_credit_invoice_by_source(pool, workspace_id, &source.id)
            .await?
            .ok_or_else(|| AppError::validation("Credit note not found", "sourceInvoiceId"))?;
        validate_existing_credit_request(pool, workspace_id, &credit_request, &existing).await?;
        return Ok(existing);
    }

    if source_status != "issued" {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Only issued invoices can be credited",
            "sourceInvoiceId",
        ));
    }


    let existing_credit: Option<String> = sqlx::query_scalar(
        r#"
        SELECT credit_invoice_id FROM credit_notes
        WHERE workspace_id = ?1 AND source_invoice_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&source.id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(existing_id) = existing_credit {
        tx.rollback().await?;
        let existing = get_invoice(pool, workspace_id, &existing_id).await?;
        validate_existing_credit_request(pool, workspace_id, &credit_request, &existing).await?;
        return Ok(existing);
    }

    sqlx::query(
        r#"
        INSERT INTO invoices (
          id, workspace_id, counterparty_id, fiscal_year_id, status, invoice_kind,
          source_invoice_id, total_ex_vat_minor, total_vat_minor, total_inc_vat_minor
        ) VALUES (?1, ?2, ?3, ?4, 'draft', 'credit_note', ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&credit_invoice_id)
    .bind(workspace_id)
    .bind(&source.counterparty_id)
    .bind(&fiscal_year_id)
    .bind(&source.id)
    .bind(source.total_ex_vat_minor)
    .bind(source.total_vat_minor)
    .bind(source.total_inc_vat_minor)
    .execute(&mut *tx)
    .await?;

    insert_lines(&mut tx, &credit_invoice_id, &credit_lines).await?;

    let snapshot = copy_issued_invoice_snapshot_tx(
        &mut tx,
        workspace_id,
        &source.id,
        &credit_invoice_id,
    )
    .await?;
    ensure_fiscal_year_open_tx(&mut *tx, &fiscal_year_id).await?;

    let seq_row = sqlx::query(
        r#"
        SELECT id, prefix, next_number
        FROM invoice_sequences
        WHERE workspace_id = ?1 AND fiscal_year_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice sequence missing", "invoiceSequence"))?;

    let sequence_id: String = seq_row.get("id");
    let prefix: String = seq_row.get("prefix");
    let next_number: i64 = seq_row.get("next_number");
    let credit_invoice_number = format!("{prefix}{next_number:04}");

    sqlx::query(
        r#"
        UPDATE invoice_sequences
        SET next_number = next_number + 1
        WHERE id = ?1
        "#,
    )
    .bind(&sequence_id)
    .execute(&mut *tx)
    .await?;

    let reversal_voucher_id = post_reversal_voucher_tx(
        &mut tx,
        workspace_id,
        &fiscal_year_id,
        &credit_invoice_id,
        &issue_date,
        &vat_buckets,
    )
    .await?;

    let pdf_job_id =
        enqueue_pdf_job_tx(&mut tx, workspace_id, &credit_invoice_id, &credit_invoice_number)
            .await?;

    sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'issued',
            invoice_number = ?1,
            issue_date = ?2,
            voucher_id = ?3,
            pdf_job_id = ?4,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?5
        "#,
    )
    .bind(&credit_invoice_number)
    .bind(&issue_date)
    .bind(&reversal_voucher_id)
    .bind(&pdf_job_id)
    .bind(&credit_invoice_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'credited', updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1 AND workspace_id = ?2
        "#,
    )
    .bind(&source.id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;

    let credit_note_id = Uuid::new_v4().to_string();
    match sqlx::query(
        r#"
        INSERT INTO credit_notes (
          id, workspace_id, source_invoice_id, credit_invoice_id, reason, reversal_voucher_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&credit_note_id)
    .bind(workspace_id)
    .bind(&source.id)
    .bind(&credit_invoice_id)
    .bind(credit_request.reason.as_deref())
    .bind(&reversal_voucher_id)
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error)
                && error.to_string().contains("credit_notes.source_invoice_id") =>
        {
            tx.rollback().await?;
            let existing = find_credit_invoice_by_source(pool, workspace_id, &source.id)
                .await?
                .ok_or_else(|| AppError::internal("Credit note unique replay failed"))?;
            validate_existing_credit_request(pool, workspace_id, &credit_request, &existing).await?;
            persist_idempotency(
                pool,
                workspace_id,
                JOB_INVOICE_CREDIT,
                idempotency_key,
                &existing,
                Some(&credit_request),
            )
            .await?;
            let cached = check_credit_idempotency_payload(pool, workspace_id, idempotency_key)
                .await?
                .ok_or_else(|| AppError::internal("Idempotent credit replay failed"))?;
            validate_credit_idempotency_match(&credit_request, &cached)?;
            return Ok(cached.invoice);
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error.into());
        }
    }

    let credit_invoice = fetch_invoice_summary_tx(&mut tx, workspace_id, &credit_invoice_id).await?;

    match record_idempotency_tx(
        &mut tx,
        workspace_id,
        JOB_INVOICE_CREDIT,
        idempotency_key,
        &credit_invoice,
        Some(&credit_request),
    )
    .await
    {
        Ok(()) => {}
        Err(error) if error.is_unique_violation() => {
            tx.rollback().await?;
            let cached = check_credit_idempotency_payload(pool, workspace_id, idempotency_key)
                .await?
                .ok_or_else(|| AppError::internal("Idempotent credit replay failed"))?;
            validate_credit_idempotency_match(&credit_request, &cached)?;
            return Ok(cached.invoice);
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error);
        }
    }

    record_event_tx(
        &mut *tx,
        workspace_id,
        "invoice_credit",
        "credit_note",
        Some(&credit_note_id),
        &serde_json::json!({
            "sourceInvoiceId": source.id,
            "creditInvoiceId": credit_invoice_id,
            "reversalVoucherId": reversal_voucher_id,
            "idempotencyKey": idempotency_key,
            "businessName": snapshot.business_name,
            "ownerName": snapshot.owner_name,
            "taxStatus": snapshot.tax_status,
            "vatStatus": snapshot.vat_status,
            "ruleVersionId": snapshot.rule_version_id,
            "taxYear": snapshot.tax_year,
            "sourceUrl": snapshot.source_url
        })
        .to_string(),
    )
    .await?;

    record_event_tx(
        &mut *tx,
        workspace_id,
        "voucher_reverse",
        "voucher",
        Some(&reversal_voucher_id),
        &serde_json::json!({
            "sourceType": "credit_note",
            "sourceId": credit_invoice_id,
            "reversesVoucherId": source_voucher_id
        })
        .to_string(),
    )
    .await?;

    tx.commit().await?;
    Ok(credit_invoice)
}

pub async fn count_open_invoices(pool: &SqlitePool, workspace_id: &str) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM invoices i
        WHERE i.workspace_id = ?1
          AND i.status = 'issued'
          AND i.invoice_kind = 'standard'
          AND NOT EXISTS (
            SELECT 1 FROM reconciliation_matches rm
            WHERE rm.workspace_id = i.workspace_id AND rm.invoice_id = i.id
          )
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub async fn original_invoice_immutable(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_id: &str,
    expected_ex_vat: i64,
    expected_vat: i64,
    expected_number: &str,
) -> Result<bool, AppError> {
    let invoice = get_invoice(pool, workspace_id, invoice_id).await?;
    Ok(invoice.total_ex_vat_minor == expected_ex_vat
        && invoice.total_vat_minor == expected_vat
        && invoice.invoice_number.as_deref() == Some(expected_number)
        && invoice.status == "credited")
}
