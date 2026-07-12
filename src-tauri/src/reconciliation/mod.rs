use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    audit::record_event_tx,
    error::AppError,
    workspace::{ensure_fiscal_year_open_tx, year_from_date},
};

const JOB_RECONCILIATION_MATCH: &str = "reconciliation_match_create";
const JOB_INVOICE_PAYMENT_RECORD: &str = "invoice_payment_record";

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationMatchCreateInput {
    pub staged_transaction_id: String,
    pub match_kind: String,
    pub invoice_id: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoicePaymentRecordInput {
    pub invoice_id: String,
    pub document_id: String,
    pub payment_date: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationMatchResult {
    pub match_id: String,
    pub voucher_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentMatchPayload {
    idempotency_key: String,
    #[serde(default)]
    staged_transaction_id: Option<String>,
    #[serde(default)]
    invoice_id: Option<String>,
    #[serde(default)]
    match_kind: Option<String>,
    result: ReconciliationMatchResult,
}

fn normalize_idempotency_key(key: &str) -> Result<&str, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Idempotency key is required", "idempotencyKey"));
    }
    Ok(trimmed)
}

async fn check_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentMatchPayload>, AppError> {
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
    .bind(JOB_RECONCILIATION_MATCH)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(payload) = existing else {
        return Ok(None);
    };
    let parsed: IdempotentMatchPayload =
        serde_json::from_str(&payload).map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Some(parsed))
}

async fn record_idempotency_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    idempotency_key: &str,
    input: &ReconciliationMatchCreateInput,
    result: &ReconciliationMatchResult,
) -> Result<(), AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload = IdempotentMatchPayload {
        idempotency_key: key.to_string(),
        staged_transaction_id: Some(input.staged_transaction_id.clone()),
        invoice_id: input.invoice_id.clone(),
        match_kind: Some(input.match_kind.clone()),
        result: result.clone(),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_RECONCILIATION_MATCH)
    .bind(payload_json)
    .bind(key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_match_idempotency_inputs(
    input: &ReconciliationMatchCreateInput,
    cached: &IdempotentMatchPayload,
) -> Result<(), AppError> {
    if let Some(ref staged_id) = cached.staged_transaction_id {
        if staged_id != input.staged_transaction_id.trim() {
            return Err(AppError::validation(
                "Idempotency key was already used for a different staged transaction",
                "idempotencyKey",
            ));
        }
    }

    if let Some(ref kind) = cached.match_kind {
        if kind != input.match_kind.trim() {
            return Err(AppError::validation(
                "Idempotency key was already used for a different match kind",
                "idempotencyKey",
            ));
        }
    }

    if let Some(ref invoice_id) = cached.invoice_id {
        if invoice_id != input.invoice_id.as_deref().unwrap_or_default() {
            return Err(AppError::validation(
                "Idempotency key was already used for a different invoice",
                "idempotencyKey",
            ));
        }
    }

    Ok(())
}

async fn lookup_account_id_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    number: &str,
) -> Result<String, AppError> {
    let id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM accounts WHERE workspace_id = ?1 AND number = ?2 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(number)
    .fetch_optional(&mut **tx)
    .await?;
    id.ok_or_else(|| AppError::validation(format!("Account {number} is not configured"), "accounts"))
}

async fn assert_invoice_payable_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<(), AppError> {
    let row = sqlx::query(
        r#"
        SELECT status, invoice_kind
        FROM invoices
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::validation("Invoice not found", "invoiceId"))?;

    let status: String = row.get("status");
    let invoice_kind: String = row.get("invoice_kind");

    if invoice_kind != "standard" {
        return Err(AppError::validation(
            "Only standard invoices can be matched for payment",
            "invoiceId",
        ));
    }

    if status != "issued" {
        return Err(AppError::validation(
            "Only issued invoices can be matched for payment",
            "invoiceId",
        ));
    }

    Ok(())
}

async fn assert_invoice_not_already_paid_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<(), AppError> {
    let existing: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM reconciliation_matches
        WHERE workspace_id = ?1
          AND invoice_id = ?2
          AND match_kind = 'invoice_payment'
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_optional(&mut **tx)
    .await?;

    if existing.is_some() {
        return Err(AppError::validation(
            "Invoice already has a payment match",
            "invoiceId",
        ));
    }

    Ok(())
}

async fn verify_balanced_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    voucher_id: &str,
) -> Result<(), AppError> {
    let row = sqlx::query(
        r#"
        SELECT COALESCE(SUM(debit_minor), 0) AS debits,
               COALESCE(SUM(credit_minor), 0) AS credits
        FROM journal_lines
        WHERE voucher_id = ?1
        "#,
    )
    .bind(voucher_id)
    .fetch_one(&mut **tx)
    .await?;

    let debits: i64 = row.get("debits");
    let credits: i64 = row.get("credits");
    if debits != credits {
        return Err(AppError::validation("Voucher is not balanced", "voucher"));
    }
    Ok(())
}

async fn assert_document_in_workspace_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    document_id: &str,
) -> Result<String, AppError> {
    let trimmed = document_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Document id is required", "documentId"));
    }

    let row = sqlx::query(
        r#"
        SELECT id, original_filename
        FROM documents
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(trimmed)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::validation("Document not found in workspace", "documentId"))?;

    Ok(row.get("original_filename"))
}

async fn post_invoice_payment_voucher_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    fiscal_year_id: &str,
    invoice_id: &str,
    transaction_date: &str,
    amount_minor: i64,
    document_id: Option<&str>,
) -> Result<String, AppError> {
    let bank_account_id = lookup_account_id_tx(tx, workspace_id, "1930").await?;
    let receivable_account_id = lookup_account_id_tx(tx, workspace_id, "1510").await?;

    assert_invoice_payable_tx(tx, workspace_id, invoice_id).await?;

    let voucher_id = Uuid::new_v4().to_string();
    match sqlx::query(
        r#"
        INSERT INTO vouchers (
          id, workspace_id, fiscal_year_id, status, source_type, source_id,
          posted_at, accounting_date, document_id
        )
        VALUES (?1, ?2, ?3, 'posted', 'reconciliation', ?4, CURRENT_TIMESTAMP, ?5, ?6)
        "#,
    )
    .bind(&voucher_id)
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .bind(invoice_id)
    .bind(transaction_date)
    .bind(document_id)
    .execute(&mut **tx)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error)
                && error.to_string().contains("idx_vouchers_one_reconciliation_per_invoice") =>
        {
            return Err(AppError::validation(
                "Invoice already has a payment voucher",
                "invoiceId",
            ));
        }
        Err(error) => return Err(error.into()),
    }

    sqlx::query(
        r#"
        INSERT INTO journal_lines (id, voucher_id, account_id, debit_minor, credit_minor, vat_code)
        VALUES (?1, ?2, ?3, ?4, 0, NULL)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&voucher_id)
    .bind(&bank_account_id)
    .bind(amount_minor)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO journal_lines (id, voucher_id, account_id, debit_minor, credit_minor, vat_code)
        VALUES (?1, ?2, ?3, 0, ?4, NULL)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&voucher_id)
    .bind(&receivable_account_id)
    .bind(amount_minor)
    .execute(&mut **tx)
    .await?;

    verify_balanced_tx(tx, &voucher_id).await?;
    Ok(voucher_id)
}

pub async fn reconciliation_match_create(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &ReconciliationMatchCreateInput,
) -> Result<ReconciliationMatchResult, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;
    if let Some(existing) = check_idempotency(pool, workspace_id, idempotency_key).await? {
        validate_match_idempotency_inputs(input, &existing)?;
        return Ok(existing.result);
    }

    if input.staged_transaction_id.trim().is_empty() {
        return Err(AppError::validation(
            "Staged transaction id is required",
            "stagedTransactionId",
        ));
    }

    if input.match_kind.trim().is_empty() {
        return Err(AppError::validation("Match kind is required", "matchKind"));
    }

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    let staged_row = sqlx::query(
        r#"
        SELECT id, status, transaction_date, amount_minor
        FROM staged_transactions
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&input.staged_transaction_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::validation("Staged transaction not found", "stagedTransactionId"))?;

    let staged_status: String = staged_row.get("status");
    if staged_status != "staged" {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Staged transaction is not available for matching",
            "stagedTransactionId",
        ));
    }

    let transaction_date: String = staged_row.get("transaction_date");
    let amount_minor: i64 = staged_row.get("amount_minor");

    crate::vat::ensure_fiscal_period_open_tx(&mut tx, workspace_id, &transaction_date).await?;

    let fiscal_year_id = format!(
        "fy-{workspace_id}-{}",
        year_from_date(&transaction_date)?
    );
    ensure_fiscal_year_open_tx(&mut *tx, &fiscal_year_id).await?;

    let voucher_id = if input.match_kind == "invoice_payment" {
        let invoice_id = input
            .invoice_id
            .as_deref()
            .ok_or_else(|| AppError::validation("Invoice id is required", "invoiceId"))?;

        assert_invoice_payable_tx(&mut tx, workspace_id, invoice_id).await?;
        assert_invoice_not_already_paid_tx(&mut tx, workspace_id, invoice_id).await?;

        let invoice_total_inc_vat: i64 = sqlx::query_scalar(
            r#"
            SELECT total_inc_vat_minor FROM invoices
            WHERE workspace_id = ?1 AND id = ?2
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(invoice_id)
        .fetch_one(&mut *tx)
        .await?;

        if amount_minor != invoice_total_inc_vat {
            tx.rollback().await?;
            return Err(AppError::validation(
                "Payment amount must match invoice total",
                "stagedTransactionId",
            ));
        }

        let voucher_id = post_invoice_payment_voucher_tx(
            &mut tx,
            workspace_id,
            &fiscal_year_id,
            invoice_id,
            &transaction_date,
            amount_minor,
            None,
        )
        .await?;
        Some(voucher_id)
    } else {
        tx.rollback().await?;
        return Err(AppError::validation("Unsupported match kind", "matchKind"));
    };

    let staged_update = sqlx::query(
        r#"
        UPDATE staged_transactions
        SET status = 'matched'
        WHERE id = ?1 AND workspace_id = ?2 AND status = 'staged'
        "#,
    )
    .bind(&input.staged_transaction_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;

    if staged_update.rows_affected() == 0 {
        tx.rollback().await?;
        return Err(AppError::validation(
            "Staged transaction is not available for matching",
            "stagedTransactionId",
        ));
    }

    let match_id = Uuid::new_v4().to_string();
    match sqlx::query(
        r#"
        INSERT INTO reconciliation_matches (id, workspace_id, staged_transaction_id, match_kind, invoice_id, voucher_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&match_id)
    .bind(workspace_id)
    .bind(&input.staged_transaction_id)
    .bind(&input.match_kind)
    .bind(input.invoice_id.as_deref())
    .bind(voucher_id.as_deref())
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error)
                && error.to_string().contains("idx_reconciliation_one_payment_per_invoice") =>
        {
            tx.rollback().await?;
            return Err(AppError::validation(
                "Invoice already has a payment match",
                "invoiceId",
            ));
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error.into());
        }
    }

    let result = ReconciliationMatchResult {
        match_id: match_id.clone(),
        voucher_id: voucher_id.clone(),
    };

    match record_idempotency_tx(&mut tx, workspace_id, idempotency_key, input, &result).await {
        Ok(()) => {}
        Err(error) if error.is_unique_violation() =>
        {
            tx.rollback().await?;
            let cached = check_idempotency(pool, workspace_id, idempotency_key)
                .await?
                .ok_or_else(|| AppError::internal("Idempotent match replay failed"))?;
            validate_match_idempotency_inputs(input, &cached)?;
            return Ok(cached.result);
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error);
        }
    }

    record_event_tx(
        &mut *tx,
        workspace_id,
        "reconciliation_match_create",
        "reconciliation_match",
        Some(&match_id),
        &serde_json::json!({
            "stagedTransactionId": input.staged_transaction_id,
            "matchKind": input.match_kind,
            "invoiceId": input.invoice_id,
            "voucherId": voucher_id,
            "idempotencyKey": idempotency_key,
        })
        .to_string(),
    )
    .await?;

    tx.commit().await?;
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentInvoicePaymentPayload {
    idempotency_key: String,
    invoice_id: String,
    document_id: String,
    result: ReconciliationMatchResult,
}

async fn check_invoice_payment_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentInvoicePaymentPayload>, AppError> {
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
    .bind(JOB_INVOICE_PAYMENT_RECORD)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(payload) = existing else {
        return Ok(None);
    };
    let parsed: IdempotentInvoicePaymentPayload =
        serde_json::from_str(&payload).map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Some(parsed))
}

fn validate_invoice_payment_idempotency_inputs(
    input: &InvoicePaymentRecordInput,
    cached: &IdempotentInvoicePaymentPayload,
) -> Result<(), AppError> {
    if cached.invoice_id != input.invoice_id.trim() {
        return Err(AppError::validation(
            "Idempotency key was already used for a different invoice",
            "idempotencyKey",
        ));
    }
    if cached.document_id != input.document_id.trim() {
        return Err(AppError::validation(
            "Idempotency key was already used for a different document",
            "idempotencyKey",
        ));
    }
    Ok(())
}

async fn record_invoice_payment_idempotency_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    input: &InvoicePaymentRecordInput,
    result: &ReconciliationMatchResult,
) -> Result<(), AppError> {
    let key = normalize_idempotency_key(&input.idempotency_key)?;
    let payload = IdempotentInvoicePaymentPayload {
        idempotency_key: key.to_string(),
        invoice_id: input.invoice_id.trim().to_string(),
        document_id: input.document_id.trim().to_string(),
        result: result.clone(),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_INVOICE_PAYMENT_RECORD)
    .bind(payload_json)
    .bind(key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn invoice_payment_record(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &InvoicePaymentRecordInput,
) -> Result<ReconciliationMatchResult, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;
    if let Some(existing) =
        check_invoice_payment_idempotency(pool, workspace_id, idempotency_key).await?
    {
        validate_invoice_payment_idempotency_inputs(input, &existing)?;
        return Ok(existing.result);
    }

    if input.invoice_id.trim().is_empty() {
        return Err(AppError::validation("Invoice id is required", "invoiceId"));
    }

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    let document_filename =
        assert_document_in_workspace_tx(&mut tx, workspace_id, &input.document_id).await?;
    let invoice_id = input.invoice_id.trim();

    assert_invoice_payable_tx(&mut tx, workspace_id, invoice_id).await?;
    assert_invoice_not_already_paid_tx(&mut tx, workspace_id, invoice_id).await?;

    let invoice_row = sqlx::query(
        r#"
        SELECT total_inc_vat_minor, issue_date
        FROM invoices
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_one(&mut *tx)
    .await?;

    let amount_minor: i64 = invoice_row.get("total_inc_vat_minor");
    let issue_date: Option<String> = invoice_row.get("issue_date");
    let payment_date = input
        .payment_date
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or(issue_date)
        .ok_or_else(|| {
            AppError::validation("Payment date is required when invoice has no issue date", "paymentDate")
        })?;

    crate::vat::ensure_fiscal_period_open_tx(&mut tx, workspace_id, &payment_date).await?;
    let fiscal_year_id = format!(
        "fy-{workspace_id}-{}",
        year_from_date(&payment_date)?
    );
    ensure_fiscal_year_open_tx(&mut *tx, &fiscal_year_id).await?;

    let staged_id = Uuid::new_v4().to_string();
    let staged_description = format!("Bank statement: {document_filename}");
    sqlx::query(
        r#"
        INSERT INTO staged_transactions (
          id, workspace_id, csv_import_id, transaction_date, description, amount_minor, status
        ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, 'staged')
        "#,
    )
    .bind(&staged_id)
    .bind(workspace_id)
    .bind(&payment_date)
    .bind(&staged_description)
    .bind(amount_minor)
    .execute(&mut *tx)
    .await?;

    let voucher_id = post_invoice_payment_voucher_tx(
        &mut tx,
        workspace_id,
        &fiscal_year_id,
        invoice_id,
        &payment_date,
        amount_minor,
        Some(input.document_id.trim()),
    )
    .await?;

    sqlx::query(
        r#"
        UPDATE staged_transactions
        SET status = 'matched'
        WHERE id = ?1 AND workspace_id = ?2 AND status = 'staged'
        "#,
    )
    .bind(&staged_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;

    let match_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO reconciliation_matches (id, workspace_id, staged_transaction_id, match_kind, invoice_id, voucher_id)
        VALUES (?1, ?2, ?3, 'invoice_payment', ?4, ?5)
        "#,
    )
    .bind(&match_id)
    .bind(workspace_id)
    .bind(&staged_id)
    .bind(invoice_id)
    .bind(&voucher_id)
    .execute(&mut *tx)
    .await?;

    let result = ReconciliationMatchResult {
        match_id: match_id.clone(),
        voucher_id: Some(voucher_id.clone()),
    };

    match record_invoice_payment_idempotency_tx(&mut tx, workspace_id, input, &result).await {
        Ok(()) => {}
        Err(error) if error.is_unique_violation() => {
            tx.rollback().await?;
            let cached = check_invoice_payment_idempotency(pool, workspace_id, idempotency_key)
                .await?
                .ok_or_else(|| AppError::internal("Idempotent invoice payment replay failed"))?;
            validate_invoice_payment_idempotency_inputs(input, &cached)?;
            return Ok(cached.result);
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error);
        }
    }

    record_event_tx(
        &mut *tx,
        workspace_id,
        "invoice_payment_record",
        "reconciliation_match",
        Some(&match_id),
        &serde_json::json!({
            "invoiceId": invoice_id,
            "documentId": input.document_id.trim(),
            "paymentDate": payment_date,
            "stagedTransactionId": staged_id,
            "voucherId": voucher_id,
            "idempotencyKey": idempotency_key,
        })
        .to_string(),
    )
    .await?;

    tx.commit().await?;
    Ok(result)
}

