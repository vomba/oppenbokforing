use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    audit::record_event_tx,
    error::AppError,
    workspace::{ensure_fiscal_year_open, fiscal_year_id_for_date},
};

const JOB_EXPENSE_POST: &str = "expense_post";

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpensePostInput {
    pub amount_minor_ex_vat: i64,
    pub vat_rate: f64,
    pub expense_account_number: String,
    pub payment_account_number: String,
    pub document_id: Option<String>,
    pub no_document_reason: Option<String>,
    pub staged_transaction_id: Option<String>,
    pub idempotency_key: String,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpensePostResult {
    pub voucher_id: Option<String>,
    pub debit_expense_minor: i64,
    pub debit_input_vat_minor: i64,
    pub credit_payment_minor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentExpensePayload {
    idempotency_key: String,
    amount_minor_ex_vat: i64,
    vat_rate: f64,
    expense_account_number: String,
    payment_account_number: String,
    document_id: Option<String>,
    no_document_reason: Option<String>,
    staged_transaction_id: Option<String>,
    date: Option<String>,
    result: ExpensePostResult,
}

fn normalize_idempotency_key(key: &str) -> Result<&str, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Idempotency key is required", "idempotencyKey"));
    }
    Ok(trimmed)
}

fn vat_rate_to_bp(rate: f64) -> Result<i64, AppError> {
    if !(0.0..=1.0).contains(&rate) {
        return Err(AppError::validation("VAT rate must be between 0 and 1", "vatRate"));
    }
    Ok((rate * 10_000.0).round() as i64)
}

fn compute_vat(amount_ex: i64, vat_rate_bp: i64) -> i64 {
    (amount_ex.saturating_mul(vat_rate_bp) + 5_000) / 10_000
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

async fn check_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentExpensePayload>, AppError> {
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
    .bind(JOB_EXPENSE_POST)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(payload) = existing else {
        return Ok(None);
    };

    let parsed: IdempotentExpensePayload =
        serde_json::from_str(&payload).map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Some(parsed))
}

async fn record_idempotency(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    idempotency_key: &str,
    input: &ExpensePostInput,
    result: &ExpensePostResult,
) -> Result<(), AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload = IdempotentExpensePayload {
        idempotency_key: key.to_string(),
        amount_minor_ex_vat: input.amount_minor_ex_vat,
        vat_rate: input.vat_rate,
        expense_account_number: input.expense_account_number.clone(),
        payment_account_number: input.payment_account_number.clone(),
        document_id: input.document_id.clone(),
        no_document_reason: input.no_document_reason.clone(),
        staged_transaction_id: input.staged_transaction_id.clone(),
        date: input.date.clone(),
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
    .bind(JOB_EXPENSE_POST)
    .bind(payload_json)
    .bind(key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_expense_idempotency_inputs(
    input: &ExpensePostInput,
    cached: &IdempotentExpensePayload,
) -> Result<(), AppError> {
    if cached.amount_minor_ex_vat != input.amount_minor_ex_vat
        || (cached.vat_rate - input.vat_rate).abs() > f64::EPSILON
        || cached.expense_account_number != input.expense_account_number
        || cached.payment_account_number != input.payment_account_number
        || cached.document_id.as_deref() != input.document_id.as_deref()
        || cached.no_document_reason.as_deref() != input.no_document_reason.as_deref()
        || cached.staged_transaction_id.as_deref() != input.staged_transaction_id.as_deref()
        || cached.date.as_deref() != input.date.as_deref()
    {
        return Err(AppError::validation(
            "Idempotency key was already used for a different expense",
            "idempotencyKey",
        ));
    }

    Ok(())
}

async fn mark_staged_expense_matched_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    staged_transaction_id: &str,
    voucher_id: &str,
    payment_credit_minor: i64,
) -> Result<(), AppError> {
    let staged_row = sqlx::query(
        r#"
        SELECT status, amount_minor
        FROM staged_transactions
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(staged_transaction_id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(staged_row) = staged_row else {
        return Err(AppError::validation(
            "Staged transaction not found",
            "stagedTransactionId",
        ));
    };

    let status: String = staged_row.get("status");
    if status != "staged" {
        return Err(AppError::validation(
            "Staged transaction is not available for matching",
            "stagedTransactionId",
        ));
    }

    let amount_minor: i64 = staged_row.get("amount_minor");
    if amount_minor != -payment_credit_minor {
        return Err(AppError::validation(
            "Staged transaction amount must match expense payment total",
            "stagedTransactionId",
        ));
    }

    let updated = sqlx::query(
        r#"
        UPDATE staged_transactions
        SET status = 'matched'
        WHERE id = ?1 AND workspace_id = ?2 AND status = 'staged'
        "#,
    )
    .bind(staged_transaction_id)
    .bind(workspace_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::validation(
            "Staged transaction is not available for matching",
            "stagedTransactionId",
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO reconciliation_matches (
          id, workspace_id, staged_transaction_id, match_kind, invoice_id, voucher_id
        ) VALUES (?1, ?2, ?3, 'expense', NULL, ?4)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(staged_transaction_id)
    .bind(voucher_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn expense_post(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &ExpensePostInput,
) -> Result<ExpensePostResult, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;
    if let Some(existing) = check_idempotency(pool, workspace_id, idempotency_key).await? {
        validate_expense_idempotency_inputs(input, &existing)?;
        return Ok(existing.result);
    }

    if input.amount_minor_ex_vat <= 0 {
        return Err(AppError::validation(
            "Expense amount must be positive",
            "amountMinorExVat",
        ));
    }

    let document_id_trimmed: Option<String> = input
        .document_id
        .as_ref()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(|s| s.to_string());
    let has_document = document_id_trimmed.is_some();
    let has_reason = input
        .no_document_reason
        .as_deref()
        .map(|reason| !reason.trim().is_empty())
        .unwrap_or(false);

    if !has_document && !has_reason {
        return Err(AppError::validation(
            "Expense requires a document or no-document reason",
            "documentId",
        ));
    }

    // When a document id is supplied it must refer to an existing document row
    // in the workspace — empty strings or unknown ids are not accepted as
    // evidence.
    if let Some(doc_id) = document_id_trimmed.as_deref() {
        let exists: Option<String> = sqlx::query_scalar(
            r#"
            SELECT id FROM documents
            WHERE workspace_id = ?1 AND id = ?2
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(doc_id)
        .fetch_optional(pool)
        .await?;

        if exists.is_none() {
            return Err(AppError::validation(
                "Document not found in workspace",
                "documentId",
            ));
        }
    }

    let vat_rate_bp = vat_rate_to_bp(input.vat_rate)?;
    let input_vat_minor = compute_vat(input.amount_minor_ex_vat, vat_rate_bp);
    let total_inc = input.amount_minor_ex_vat + input_vat_minor;

    let date = input
        .date
        .clone()
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
    let fiscal_year_id = fiscal_year_id_for_date(pool, workspace_id, &date).await?;
    ensure_fiscal_year_open(pool, &fiscal_year_id).await?;

    let mut tx = pool.begin().await?;

    let expense_account_id =
        lookup_account_id_tx(&mut tx, workspace_id, input.expense_account_number.trim()).await?;
    let payment_account_id =
        lookup_account_id_tx(&mut tx, workspace_id, input.payment_account_number.trim()).await?;

    let input_vat_account_id = if input_vat_minor > 0 {
        Some(lookup_account_id_tx(&mut tx, workspace_id, "2641").await?)
    } else {
        None
    };

    // Map VAT rate to BAS VAT code; 25%, 12% and 6% are supported.
    let vat_code = if input_vat_minor > 0 {
        match vat_rate_bp {
            2500 => Some("VAT25"),
            1200 => Some("VAT12"),
            600 => Some("VAT6"),
            _ => {
                return Err(AppError::validation(
                    "Unsupported VAT rate for input VAT",
                    "vatRate",
                ))
            }
        }
    } else {
        None
    };

    let voucher_id = Uuid::new_v4().to_string();
    crate::vat::ensure_fiscal_period_open_tx(&mut tx, workspace_id, &date).await?;
    sqlx::query(
        r#"
        INSERT INTO vouchers (
          id, workspace_id, fiscal_year_id, status, source_type, source_id, posted_at,
          document_id, no_document_reason, accounting_date
        )
        VALUES (?1, ?2, ?3, 'posted', 'expense', NULL, CURRENT_TIMESTAMP, ?4, ?5, ?6)
        "#,
    )
    .bind(&voucher_id)
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .bind(document_id_trimmed.as_deref())
    .bind(input.no_document_reason.as_deref())
    .bind(&date)
    .execute(&mut *tx)
    .await?;

    // Debit expense
    sqlx::query(
        r#"
        INSERT INTO journal_lines (id, voucher_id, account_id, debit_minor, credit_minor, vat_code)
        VALUES (?1, ?2, ?3, ?4, 0, NULL)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&voucher_id)
    .bind(&expense_account_id)
    .bind(input.amount_minor_ex_vat)
    .execute(&mut *tx)
    .await?;

    // Debit input VAT
    if let Some(vat_account_id) = input_vat_account_id {
        sqlx::query(
            r#"
            INSERT INTO journal_lines (id, voucher_id, account_id, debit_minor, credit_minor, vat_code)
            VALUES (?1, ?2, ?3, ?4, 0, ?5)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&voucher_id)
        .bind(&vat_account_id)
        .bind(input_vat_minor)
        .bind(vat_code)
        .execute(&mut *tx)
        .await?;
    }

    // Credit payment
    sqlx::query(
        r#"
        INSERT INTO journal_lines (id, voucher_id, account_id, debit_minor, credit_minor, vat_code)
        VALUES (?1, ?2, ?3, 0, ?4, NULL)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&voucher_id)
    .bind(&payment_account_id)
    .bind(total_inc)
    .execute(&mut *tx)
    .await?;

    verify_balanced_tx(&mut tx, &voucher_id).await?;

    if let Some(staged_id) = input
        .staged_transaction_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        mark_staged_expense_matched_tx(&mut tx, workspace_id, staged_id, &voucher_id, total_inc).await?;
    }

    let result = ExpensePostResult {
        voucher_id: Some(voucher_id.clone()),
        debit_expense_minor: input.amount_minor_ex_vat,
        debit_input_vat_minor: input_vat_minor,
        credit_payment_minor: total_inc,
    };

    match record_idempotency(&mut tx, workspace_id, idempotency_key, input, &result).await {
        Ok(()) => {}
        Err(error)
            if error.is_unique_violation() =>
        {
            tx.rollback().await?;
            let cached = check_idempotency(pool, workspace_id, idempotency_key)
                .await?
                .ok_or_else(|| AppError::internal("Idempotent expense replay failed"))?;
            validate_expense_idempotency_inputs(input, &cached)?;
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
        "expense_post",
        "voucher",
        Some(&voucher_id),
        &serde_json::json!({
            "amountMinorExVat": input.amount_minor_ex_vat,
            "vatMinor": input_vat_minor,
            "paymentMinor": total_inc,
            "documentId": input.document_id,
            "noDocumentReason": input.no_document_reason,
            "stagedTransactionId": input.staged_transaction_id,
            "idempotencyKey": idempotency_key,
        })
        .to_string(),
    )
    .await?;

    // Record the voucher_post audit event inside the same transaction so that
    // any failure here rolls back the voucher and idempotency record together.
    record_event_tx(
        &mut *tx,
        workspace_id,
        "voucher_post",
        "voucher",
        Some(&voucher_id),
        &serde_json::json!({ "sourceType": "expense", "voucherId": voucher_id }).to_string(),
    )
    .await?;

    tx.commit().await?;

    Ok(result)
}

