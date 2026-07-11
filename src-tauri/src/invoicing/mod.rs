use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    audit::{record_event, record_event_tx},
    error::AppError,
    ledger::{post_invoice_voucher_tx, post_reversal_voucher_tx, vat_buckets_from_rate_lines},
    profiles::{get_tax_profile, get_vat_profile},
    workspace::{ensure_fiscal_year_open, fiscal_year_id_for_date},
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentInvoicePayload {
    idempotency_key: String,
    invoice: InvoiceSummary,
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
    let vat_profile = get_vat_profile(pool, workspace_id).await?;
    let status = vat_profile
        .map(|profile| profile.vat_status)
        .unwrap_or_else(|| "exempt_low_turnover".to_string());

    let charges_vat = lines.iter().any(|line| line.vat_rate > 0.0);
    if status == "exempt_low_turnover" && charges_vat {
        return Err(AppError::validation(
            "VAT-exempt profile cannot issue VAT-charging invoices",
            "vatStatus",
        ));
    }
    Ok(())
}

fn normalize_idempotency_key(key: &str) -> Result<&str, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Idempotency key is required", "idempotencyKey"));
    }
    Ok(trimmed)
}

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
    source_invoice_id: &str,
    cached: &InvoiceSummary,
) -> Result<(), AppError> {
    if cached.source_invoice_id.as_deref() != Some(source_invoice_id) {
        return Err(AppError::validation(
            "Idempotency key was already used for a different source invoice",
            "idempotencyKey",
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

pub async fn check_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    job_type: &str,
    idempotency_key: &str,
) -> Result<Option<InvoiceSummary>, AppError> {
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

    let parsed: IdempotentInvoicePayload = serde_json::from_str(&payload)
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Some(parsed.invoice))
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

async fn persist_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    job_type: &str,
    idempotency_key: &str,
    invoice: &InvoiceSummary,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    match record_idempotency_tx(&mut tx, workspace_id, job_type, idempotency_key, invoice).await {
        Ok(()) => tx.commit().await?,
        Err(error) if error.is_unique_violation() =>
        {
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
) -> Result<(), AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload = IdempotentInvoicePayload {
        idempotency_key: key.to_string(),
        invoice: invoice.clone(),
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

    let tax_profile = get_tax_profile(pool, workspace_id).await?;
    if tax_profile.is_none() {
        return Err(AppError::validation("Tax profile is required before issuing", "taxProfile"));
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

    let issue_date = input
        .issue_date
        .clone()
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
    let fiscal_year_id = fiscal_year_id_for_date(pool, workspace_id, &issue_date).await?;
    ensure_fiscal_year_open(pool, &fiscal_year_id).await?;

    let vat_buckets = vat_buckets_from_rate_lines(draft_lines.iter().map(|line| {
        (
            line.quantity,
            line.unit_price_minor,
            vat_rate_to_bp(line.vat_rate),
        )
    }))?;

    let mut tx = pool.begin().await?;

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
    let _ = (total_ex_vat_minor, total_vat_minor);

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
            )
            .await?;
            return Ok(invoice);
        }
        return Err(AppError::validation("Only draft invoices can be issued", "invoiceId"));
    }

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
            "pdfJobId": pdf_job_id
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
    if let Some(existing) =
        check_credit_idempotency(pool, workspace_id, idempotency_key).await?
    {
        validate_credit_idempotency_match(&input.source_invoice_id, &existing)?;
        return Ok(existing);
    }

    if let Some(existing) =
        find_credit_invoice_by_source(pool, workspace_id, &input.source_invoice_id).await?
    {
        return Ok(existing);
    }

    let source = get_invoice(pool, workspace_id, &input.source_invoice_id).await?;
    if source.invoice_kind != "standard" {
        return Err(AppError::validation("Cannot credit a credit note", "sourceInvoiceId"));
    }

    let source_fiscal_year_id: String = sqlx::query_scalar(
        r#"
        SELECT fiscal_year_id FROM invoices WHERE workspace_id = ?1 AND id = ?2 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&source.id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "sourceInvoiceId"))?;

    ensure_fiscal_year_open(pool, &source_fiscal_year_id).await?;

    let issue_date = source
        .issue_date
        .clone()
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

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
    validate_vat_lines(pool, workspace_id, &credit_lines).await?;

    let vat_buckets = vat_buckets_from_rate_lines(credit_lines.iter().map(|line| {
        (
            line.quantity,
            line.unit_price_minor,
            vat_rate_to_bp(line.vat_rate),
        )
    }))?;

    let credit_invoice_id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await?;

    let source_status: String = sqlx::query_scalar(
        r#"
        SELECT status FROM invoices WHERE workspace_id = ?1 AND id = ?2 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&source.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "sourceInvoiceId"))?;

    if source_status == "credited" {
        tx.rollback().await?;
        return find_credit_invoice_by_source(pool, workspace_id, &source.id)
            .await?
            .ok_or_else(|| AppError::validation("Credit note not found", "sourceInvoiceId"));
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
        return get_invoice(pool, workspace_id, &existing_id).await;
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
    .bind(&source_fiscal_year_id)
    .bind(&source.id)
    .bind(source.total_ex_vat_minor)
    .bind(source.total_vat_minor)
    .bind(source.total_inc_vat_minor)
    .execute(&mut *tx)
    .await?;

    insert_lines(&mut tx, &credit_invoice_id, &credit_lines).await?;

    let seq_row = sqlx::query(
        r#"
        SELECT id, prefix, next_number
        FROM invoice_sequences
        WHERE workspace_id = ?1 AND fiscal_year_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&source_fiscal_year_id)
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
        &source_fiscal_year_id,
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
    .bind(input.reason.as_deref())
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
            persist_idempotency(
                pool,
                workspace_id,
                JOB_INVOICE_CREDIT,
                idempotency_key,
                &existing,
            )
            .await?;
            return Ok(existing);
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
    )
    .await
    {
        Ok(()) => {}
        Err(error) if error.is_unique_violation() =>
        {
            tx.rollback().await?;
            let cached = check_credit_idempotency(pool, workspace_id, idempotency_key)
                .await?
                .ok_or_else(|| AppError::internal("Idempotent credit replay failed"))?;
            validate_credit_idempotency_match(&input.source_invoice_id, &cached)?;
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
        "invoice_credit",
        "credit_note",
        Some(&credit_note_id),
        &serde_json::json!({
            "sourceInvoiceId": source.id,
            "creditInvoiceId": credit_invoice_id,
            "reversalVoucherId": reversal_voucher_id,
            "idempotencyKey": idempotency_key
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
