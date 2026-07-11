use sqlx::{Row, SqlitePool};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::{audit::record_event_tx, error::AppError};

pub struct VatBucket {
    pub vat_code: String,
    pub ex_vat_minor: i64,
    pub vat_minor: i64,
}

pub fn vat_code_from_rate_bp(vat_rate_bp: i64) -> Option<&'static str> {
    match vat_rate_bp {
        0 => Some("VAT0"),
        2500 => Some("VAT25"),
        1200 => Some("VAT12"),
        600 => Some("VAT6"),
        _ => None,
    }
}

pub fn vat_buckets_from_rate_lines(
    lines: impl IntoIterator<Item = (i64, i64, i64)>,
) -> Result<Vec<VatBucket>, AppError> {
    let mut by_rate: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for (quantity, unit_price_minor, vat_rate_bp) in lines {
        let ex = quantity.saturating_mul(unit_price_minor);
        let vat = (ex.saturating_mul(vat_rate_bp) + 5_000) / 10_000;
        let entry = by_rate.entry(vat_rate_bp).or_insert((0, 0));
        entry.0 += ex;
        entry.1 += vat;
    }

    let mut buckets = Vec::new();
    for (rate_bp, (ex_vat_minor, vat_minor)) in by_rate {
        if ex_vat_minor == 0 && vat_minor == 0 {
            continue;
        }
        let vat_code = vat_code_from_rate_bp(rate_bp).ok_or_else(|| {
            AppError::validation(
                format!("Unsupported VAT rate: {rate_bp} basis points"),
                "vatRate",
            )
        })?;
        buckets.push(VatBucket {
            vat_code: vat_code.to_string(),
            ex_vat_minor,
            vat_minor,
        });
    }
    Ok(buckets)
}

pub struct BasAccountIds {
    pub receivable: String,
    pub revenue: String,
    pub output_vat: String,
}

async fn lookup_account(
    executor: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    .fetch_optional(&mut **executor)
    .await?;
    id.ok_or_else(|| AppError::validation(format!("Account {number} is not configured"), "accounts"))
}

pub async fn bas_account_ids(pool: &SqlitePool, workspace_id: &str) -> Result<BasAccountIds, AppError> {
    Ok(BasAccountIds {
        receivable: {
            let id: Option<String> = sqlx::query_scalar(
                r#"
                SELECT id FROM accounts WHERE workspace_id = ?1 AND number = '1510' LIMIT 1
                "#,
            )
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
            id.ok_or_else(|| AppError::validation("Account 1510 is not configured", "accounts"))?
        },
        revenue: {
            let id: Option<String> = sqlx::query_scalar(
                r#"
                SELECT id FROM accounts WHERE workspace_id = ?1 AND number = '3041' LIMIT 1
                "#,
            )
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
            id.ok_or_else(|| AppError::validation("Account 3041 is not configured", "accounts"))?
        },
        output_vat: {
            let id: Option<String> = sqlx::query_scalar(
                r#"
                SELECT id FROM accounts WHERE workspace_id = ?1 AND number = '2611' LIMIT 1
                "#,
            )
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
            id.ok_or_else(|| AppError::validation("Account 2611 is not configured", "accounts"))?
        },
    })
}

async fn bas_account_ids_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
) -> Result<BasAccountIds, AppError> {
    Ok(BasAccountIds {
        receivable: lookup_account(tx, workspace_id, "1510").await?,
        revenue: lookup_account(tx, workspace_id, "3041").await?,
        output_vat: lookup_account(tx, workspace_id, "2611").await?,
    })
}

pub async fn post_invoice_voucher_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    fiscal_year_id: &str,
    invoice_id: &str,
    accounting_date: &str,
    buckets: &[VatBucket],
) -> Result<String, AppError> {
    let accounts = bas_account_ids_tx(tx, workspace_id).await?;
    let total_ex_vat_minor: i64 = buckets.iter().map(|b| b.ex_vat_minor).sum();
    let total_vat_minor: i64 = buckets.iter().map(|b| b.vat_minor).sum();
    let total_inc = total_ex_vat_minor + total_vat_minor;
    if total_inc <= 0 {
        return Err(AppError::validation("Invoice total must be positive", "lines"));
    }

    crate::vat::ensure_fiscal_period_open_tx(tx, workspace_id, accounting_date).await?;

    let voucher_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO vouchers (
          id, workspace_id, fiscal_year_id, status, source_type, source_id,
          posted_at, accounting_date
        )
        VALUES (?1, ?2, ?3, 'posted', 'invoice', ?4, CURRENT_TIMESTAMP, ?5)
        "#,
    )
    .bind(&voucher_id)
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .bind(invoice_id)
    .bind(accounting_date)
    .execute(&mut **tx)
    .await?;

    insert_line_tx(
        tx,
        &voucher_id,
        &accounts.receivable,
        total_inc,
        0,
        None,
    )
    .await?;
    insert_line_tx(
        tx,
        &voucher_id,
        &accounts.revenue,
        0,
        total_ex_vat_minor,
        None,
    )
    .await?;
    for bucket in buckets {
        if bucket.vat_minor > 0 {
            insert_line_tx(
                tx,
                &voucher_id,
                &accounts.output_vat,
                0,
                bucket.vat_minor,
                Some(&bucket.vat_code),
            )
            .await?;
        }
    }

    verify_balanced_tx(tx, &voucher_id).await?;
    Ok(voucher_id)
}

pub async fn post_invoice_voucher(
    pool: &SqlitePool,
    workspace_id: &str,
    fiscal_year_id: &str,
    invoice_id: &str,
    accounting_date: &str,
    buckets: &[VatBucket],
) -> Result<String, AppError> {
    let mut tx = pool.begin().await?;
    let voucher_id = post_invoice_voucher_tx(
        &mut tx,
        workspace_id,
        fiscal_year_id,
        invoice_id,
        accounting_date,
        buckets,
    )
    .await?;
    record_event_tx(
        &mut *tx,
        workspace_id,
        "voucher_post",
        "voucher",
        Some(&voucher_id),
        &serde_json::json!({ "sourceType": "invoice", "sourceId": invoice_id }).to_string(),
    )
    .await?;
    tx.commit().await?;
    Ok(voucher_id)
}

pub async fn post_reversal_voucher_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    fiscal_year_id: &str,
    credit_invoice_id: &str,
    accounting_date: &str,
    buckets: &[VatBucket],
) -> Result<String, AppError> {
    let accounts = bas_account_ids_tx(tx, workspace_id).await?;
    let total_ex_vat_minor: i64 = buckets.iter().map(|b| b.ex_vat_minor).sum();
    let total_vat_minor: i64 = buckets.iter().map(|b| b.vat_minor).sum();
    let total_inc = total_ex_vat_minor + total_vat_minor;

    crate::vat::ensure_fiscal_period_open_tx(tx, workspace_id, accounting_date).await?;

    let voucher_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO vouchers (
          id, workspace_id, fiscal_year_id, status, source_type, source_id,
          posted_at, accounting_date
        )
        VALUES (?1, ?2, ?3, 'posted', 'credit_note', ?4, CURRENT_TIMESTAMP, ?5)
        "#,
    )
    .bind(&voucher_id)
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .bind(credit_invoice_id)
    .bind(accounting_date)
    .execute(&mut **tx)
    .await?;

    insert_line_tx(tx, &voucher_id, &accounts.receivable, 0, total_inc, None).await?;
    insert_line_tx(
        tx,
        &voucher_id,
        &accounts.revenue,
        total_ex_vat_minor,
        0,
        None,
    )
    .await?;
    for bucket in buckets {
        if bucket.vat_minor > 0 {
            insert_line_tx(
                tx,
                &voucher_id,
                &accounts.output_vat,
                bucket.vat_minor,
                0,
                Some(&bucket.vat_code),
            )
            .await?;
        }
    }

    verify_balanced_tx(tx, &voucher_id).await?;
    Ok(voucher_id)
}

pub async fn post_reversal_voucher(
    pool: &SqlitePool,
    workspace_id: &str,
    fiscal_year_id: &str,
    credit_invoice_id: &str,
    source_voucher_id: &str,
    accounting_date: &str,
    buckets: &[VatBucket],
) -> Result<String, AppError> {
    let mut tx = pool.begin().await?;
    let voucher_id = post_reversal_voucher_tx(
        &mut tx,
        workspace_id,
        fiscal_year_id,
        credit_invoice_id,
        accounting_date,
        buckets,
    )
    .await?;
    record_event_tx(
        &mut *tx,
        workspace_id,
        "voucher_reverse",
        "voucher",
        Some(&voucher_id),
        &serde_json::json!({
            "sourceType": "credit_note",
            "sourceId": credit_invoice_id,
            "reversesVoucherId": source_voucher_id
        })
        .to_string(),
    )
    .await?;
    tx.commit().await?;
    Ok(voucher_id)
}

async fn insert_line_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    voucher_id: &str,
    account_id: &str,
    debit_minor: i64,
    credit_minor: i64,
    vat_code: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO journal_lines (id, voucher_id, account_id, debit_minor, credit_minor, vat_code)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(voucher_id)
    .bind(account_id)
    .bind(debit_minor)
    .bind(credit_minor)
    .bind(vat_code)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn verify_balanced_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    voucher_id: &str,
) -> Result<(), AppError> {
    let row = sqlx::query(
        r#"
        SELECT COALESCE(SUM(debit_minor), 0) AS debit_total,
               COALESCE(SUM(credit_minor), 0) AS credit_total
        FROM journal_lines
        WHERE voucher_id = ?1
        "#,
    )
    .bind(voucher_id)
    .fetch_one(&mut **tx)
    .await?;

    let debit_total: i64 = row.get("debit_total");
    let credit_total: i64 = row.get("credit_total");
    if debit_total != credit_total {
        return Err(AppError::validation(
            "Voucher is not balanced",
            "journalLines",
        ));
    }
    Ok(())
}

pub async fn net_revenue_minor_for_fiscal_year(
    pool: &SqlitePool,
    workspace_id: &str,
    fiscal_year_id: &str,
) -> Result<i64, AppError> {
    let revenue_account: String = sqlx::query_scalar(
        r#"
        SELECT id FROM accounts WHERE workspace_id = ?1 AND number = '3041' LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Revenue account missing", "accounts"))?;

    let credits: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.credit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND v.fiscal_year_id = ?2
          AND jl.account_id = ?3 AND v.status = 'posted'
        "#,
    )
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .bind(&revenue_account)
    .fetch_one(pool)
    .await?;

    let debits: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.debit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND v.fiscal_year_id = ?2
          AND jl.account_id = ?3 AND v.status = 'posted'
        "#,
    )
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .bind(&revenue_account)
    .fetch_one(pool)
    .await?;

    Ok(credits - debits)
}

pub async fn account_balance_minor(
    pool: &SqlitePool,
    workspace_id: &str,
    account_number: &str,
) -> Result<i64, AppError> {
    let account_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM accounts WHERE workspace_id = ?1 AND number = ?2 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(account_number)
    .fetch_optional(pool)
    .await?;

    let Some(account_id) = account_id else {
        return Ok(0);
    };

    let debits: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.debit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND jl.account_id = ?2 AND v.status = 'posted'
        "#,
    )
    .bind(workspace_id)
    .bind(&account_id)
    .fetch_one(pool)
    .await?;

    let credits: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.credit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND jl.account_id = ?2 AND v.status = 'posted'
        "#,
    )
    .bind(workspace_id)
    .bind(&account_id)
    .fetch_one(pool)
    .await?;

    Ok(debits - credits)
}

pub async fn net_revenue_minor(pool: &SqlitePool, workspace_id: &str) -> Result<i64, AppError> {
    let revenue_account: String = sqlx::query_scalar(
        r#"
        SELECT id FROM accounts WHERE workspace_id = ?1 AND number = '3041' LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Revenue account missing", "accounts"))?;

    let credits: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.credit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND jl.account_id = ?2 AND v.status = 'posted'
        "#,
    )
    .bind(workspace_id)
    .bind(&revenue_account)
    .fetch_one(pool)
    .await?;

    let debits: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.debit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND jl.account_id = ?2 AND v.status = 'posted'
        "#,
    )
    .bind(workspace_id)
    .bind(&revenue_account)
    .fetch_one(pool)
    .await?;

    Ok(credits - debits)
}

pub async fn net_output_vat_minor(pool: &SqlitePool, workspace_id: &str) -> Result<i64, AppError> {
    let vat_account: String = sqlx::query_scalar(
        r#"
        SELECT id FROM accounts WHERE workspace_id = ?1 AND number = '2611' LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Output VAT account missing", "accounts"))?;

    let credits: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.credit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND jl.account_id = ?2 AND v.status = 'posted'
        "#,
    )
    .bind(workspace_id)
    .bind(&vat_account)
    .fetch_one(pool)
    .await?;

    let debits: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.debit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND jl.account_id = ?2 AND v.status = 'posted'
        "#,
    )
    .bind(workspace_id)
    .bind(&vat_account)
    .fetch_one(pool)
    .await?;

    Ok(credits - debits)
}

pub async fn has_reversal_for_invoice(
    pool: &SqlitePool,
    workspace_id: &str,
    source_invoice_id: &str,
) -> Result<bool, AppError> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM credit_notes
        WHERE workspace_id = ?1 AND source_invoice_id = ?2
        "#,
    )
    .bind(workspace_id)
    .bind(source_invoice_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoucherCountInput {
    pub status: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoucherListInput {
    pub status: Option<String>,
    pub source_type: Option<String>,
    pub limit: Option<i64>,
    pub before_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoucherSummary {
    pub id: String,
    pub status: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub accounting_date: Option<String>,
    pub posted_at: Option<String>,
    pub debit_total_minor: i64,
    pub credit_total_minor: i64,
    pub line_count: i64,
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoucherGetInput {
    pub voucher_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JournalLineRow {
    pub account_number: String,
    pub account_name: String,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub vat_code: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoucherDetail {
    pub id: String,
    pub status: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub fiscal_year_id: Option<String>,
    pub accounting_date: Option<String>,
    pub posted_at: Option<String>,
    pub document_id: Option<String>,
    pub no_document_reason: Option<String>,
    pub lines: Vec<JournalLineRow>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub id: String,
    pub number: String,
    pub name: String,
    pub account_type: String,
    pub normal_balance: String,
    pub balance_minor: i64,
}

fn voucher_list_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(100).clamp(1, 500)
}

pub async fn voucher_list(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &VoucherListInput,
) -> Result<Vec<VoucherSummary>, AppError> {
    let limit = voucher_list_limit(input.limit);
    let before_id = input
        .before_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let rows = sqlx::query(
        r#"
        SELECT v.id, v.status, v.source_type, v.source_id, v.accounting_date, v.posted_at,
               v.document_id,
               COALESCE(SUM(jl.debit_minor), 0) AS debit_total_minor,
               COALESCE(SUM(jl.credit_minor), 0) AS credit_total_minor,
               COUNT(jl.id) AS line_count
        FROM vouchers v
        LEFT JOIN journal_lines jl ON jl.voucher_id = v.id
        WHERE v.workspace_id = ?1
          AND (?2 IS NULL OR v.status = ?2)
          AND (?3 IS NULL OR v.source_type = ?3)
          AND (
            ?4 IS NULL
            OR (
              COALESCE(v.accounting_date, v.posted_at, v.created_at) <
              (SELECT COALESCE(accounting_date, posted_at, created_at)
               FROM vouchers WHERE id = ?4 AND workspace_id = ?1)
              OR (
                COALESCE(v.accounting_date, v.posted_at, v.created_at) =
                (SELECT COALESCE(accounting_date, posted_at, created_at)
                 FROM vouchers WHERE id = ?4 AND workspace_id = ?1)
                AND v.id < ?4
              )
            )
          )
        GROUP BY v.id
        ORDER BY COALESCE(v.accounting_date, v.posted_at, v.created_at) DESC, v.id DESC
        LIMIT ?5
        "#,
    )
    .bind(workspace_id)
    .bind(input.status.as_deref())
    .bind(input.source_type.as_deref())
    .bind(before_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| VoucherSummary {
            id: row.get("id"),
            status: row.get("status"),
            source_type: row.get("source_type"),
            source_id: row.get("source_id"),
            accounting_date: row.get("accounting_date"),
            posted_at: row.get("posted_at"),
            debit_total_minor: row.get("debit_total_minor"),
            credit_total_minor: row.get("credit_total_minor"),
            line_count: row.get("line_count"),
            document_id: row.get("document_id"),
        })
        .collect())
}

pub async fn voucher_count(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &VoucherCountInput,
) -> Result<i64, AppError> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM vouchers
        WHERE workspace_id = ?1
          AND (?2 IS NULL OR status = ?2)
        "#,
    )
    .bind(workspace_id)
    .bind(input.status.as_deref())
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

pub async fn voucher_get(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &VoucherGetInput,
) -> Result<VoucherDetail, AppError> {
    let voucher_id = input.voucher_id.trim();
    if voucher_id.is_empty() {
        return Err(AppError::validation("Voucher id is required", "voucherId"));
    }

    let header = sqlx::query(
        r#"
        SELECT id, status, source_type, source_id, fiscal_year_id,
               accounting_date, posted_at, document_id, no_document_reason
        FROM vouchers
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(voucher_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Voucher not found", "voucherId"))?;

    let line_rows = sqlx::query(
        r#"
        SELECT a.number AS account_number, a.name AS account_name,
               jl.debit_minor, jl.credit_minor, jl.vat_code
        FROM journal_lines jl
        JOIN accounts a ON a.id = jl.account_id
        WHERE jl.voucher_id = ?1
        ORDER BY a.number
        "#,
    )
    .bind(voucher_id)
    .fetch_all(pool)
    .await?;

    let lines = line_rows
        .into_iter()
        .map(|row| JournalLineRow {
            account_number: row.get("account_number"),
            account_name: row.get("account_name"),
            debit_minor: row.get("debit_minor"),
            credit_minor: row.get("credit_minor"),
            vat_code: row.get("vat_code"),
        })
        .collect();

    Ok(VoucherDetail {
        id: header.get("id"),
        status: header.get("status"),
        source_type: header.get("source_type"),
        source_id: header.get("source_id"),
        fiscal_year_id: header.get("fiscal_year_id"),
        accounting_date: header.get("accounting_date"),
        posted_at: header.get("posted_at"),
        document_id: header.get("document_id"),
        no_document_reason: header.get("no_document_reason"),
        lines,
    })
}

pub async fn account_list(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<AccountSummary>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.number, a.name, a.account_type, a.normal_balance,
               COALESCE(SUM(CASE WHEN v.status = 'posted' THEN jl.debit_minor ELSE 0 END), 0)
                 AS debit_total,
               COALESCE(SUM(CASE WHEN v.status = 'posted' THEN jl.credit_minor ELSE 0 END), 0)
                 AS credit_total
        FROM accounts a
        LEFT JOIN journal_lines jl ON jl.account_id = a.id
        LEFT JOIN vouchers v ON v.id = jl.voucher_id
        WHERE a.workspace_id = ?1
        GROUP BY a.id
        ORDER BY a.number
        "#,
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let normal_balance: String = row.get("normal_balance");
            let debit_total: i64 = row.get("debit_total");
            let credit_total: i64 = row.get("credit_total");
            let balance_minor = if normal_balance == "credit" {
                credit_total - debit_total
            } else {
                debit_total - credit_total
            };
            AccountSummary {
                id: row.get("id"),
                number: row.get("number"),
                name: row.get("name"),
                account_type: row.get("account_type"),
                normal_balance,
                balance_minor,
            }
        })
        .collect())
}
