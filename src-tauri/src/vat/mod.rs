use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    audit::{record_event, record_event_tx},
    error::AppError,
    ledger::net_revenue_minor_for_fiscal_year,
    profiles::get_vat_profile,
    rules::{get_active_rule_version, get_rule_i64, require_rule_i64},
    workspace::{ensure_fiscal_year_open_tx, fiscal_year_id_for_year},
};

const JOB_VAT_RETURN_DRAFT: &str = "vat_return_draft_create";
const JOB_VAT_RETURN_APPROVE: &str = "vat_return_approve";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentVatReturnPayload {
    vat_return_id: String,
    period_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentVatApprovePayload {
    vat_return_id: String,
}

struct LedgerLineRow {
    voucher_id: String,
    account_number: String,
    debit_minor: i64,
    credit_minor: i64,
    vat_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnBox {
    pub box_code: String,
    pub amount_minor: i64,
    pub source_query_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnSummary {
    pub id: String,
    pub fiscal_period_id: String,
    pub period_key: String,
    pub status: String,
    pub rule_version_id: String,
    pub boxes: Vec<VatReturnBox>,
    pub box49_amount_minor: i64,
    pub zero_return: bool,
    pub export_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnDraftCreateInput {
    pub period_key: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnGetInput {
    pub vat_return_id: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnApproveInput {
    pub vat_return_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnExportInput {
    pub vat_return_id: String,
    pub export_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatThresholdStatus {
    pub annual_turnover_minor: i64,
    pub threshold_minor: i64,
    pub warning: String,
    pub must_register_for_vat: bool,
    pub must_charge_vat: bool,
}

struct PeriodBounds {
    fiscal_year: i32,
    starts_on: String,
    ends_on: String,
}

fn normalize_idempotency_key(key: &str) -> Result<&str, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Idempotency key is required", "idempotencyKey"));
    }
    Ok(trimmed)
}

fn parse_period_key(period_key: &str) -> Result<PeriodBounds, AppError> {
    let key = period_key.trim();
    if let Some(dash) = key.find("-M") {
        let year: i32 = key[..dash]
            .parse()
            .map_err(|_| AppError::validation("Invalid period key", "periodKey"))?;
        let month: u32 = key[dash + 2..]
            .parse()
            .map_err(|_| AppError::validation("Invalid period key", "periodKey"))?;
        if !(1..=12).contains(&month) {
            return Err(AppError::validation("Invalid monthly period key", "periodKey"));
        }
        let starts_on = NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| AppError::validation("Invalid monthly period key", "periodKey"))?;
        let ends_on = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
        }
        .and_then(|d| d.pred_opt())
        .ok_or_else(|| AppError::validation("Invalid monthly period key", "periodKey"))?;
        return Ok(PeriodBounds {
            fiscal_year: year,
            starts_on: starts_on.format("%Y-%m-%d").to_string(),
            ends_on: ends_on.format("%Y-%m-%d").to_string(),
        });
    }
    if let Some(rest) = key.strip_suffix("-Q1") {
        let year: i32 = rest.parse().map_err(|_| AppError::validation("Invalid period key", "periodKey"))?;
        return Ok(PeriodBounds {
            fiscal_year: year,
            starts_on: format!("{year}-01-01"),
            ends_on: format!("{year}-03-31"),
        });
    }
    if let Some(rest) = key.strip_suffix("-Q2") {
        let year: i32 = rest.parse().map_err(|_| AppError::validation("Invalid period key", "periodKey"))?;
        return Ok(PeriodBounds {
            fiscal_year: year,
            starts_on: format!("{year}-04-01"),
            ends_on: format!("{year}-06-30"),
        });
    }
    if let Some(rest) = key.strip_suffix("-Q3") {
        let year: i32 = rest.parse().map_err(|_| AppError::validation("Invalid period key", "periodKey"))?;
        return Ok(PeriodBounds {
            fiscal_year: year,
            starts_on: format!("{year}-07-01"),
            ends_on: format!("{year}-09-30"),
        });
    }
    if let Some(rest) = key.strip_suffix("-Q4") {
        let year: i32 = rest.parse().map_err(|_| AppError::validation("Invalid period key", "periodKey"))?;
        return Ok(PeriodBounds {
            fiscal_year: year,
            starts_on: format!("{year}-10-01"),
            ends_on: format!("{year}-12-31"),
        });
    }
    if key.len() == 4 && key.chars().all(|c| c.is_ascii_digit()) {
        let year: i32 = key.parse().map_err(|_| AppError::validation("Invalid period key", "periodKey"))?;
        return Ok(PeriodBounds {
            fiscal_year: year,
            starts_on: format!("{year}-01-01"),
            ends_on: format!("{year}-12-31"),
        });
    }
    Err(AppError::validation(
        "Period key must be YYYY, YYYY-M01..M12, or YYYY-Q1..Q4",
        "periodKey",
    ))
}

pub fn period_key_matches_reporting_period(reporting_period: &str, period_key: &str) -> bool {
    let key = period_key.trim();
    match reporting_period {
        "yearly" => key.len() == 4 && key.chars().all(|c| c.is_ascii_digit()),
        "monthly" => key.contains("-M"),
        _ => key.contains("-Q"),
    }
}

pub fn period_keys_for_year(reporting_period: &str, year: i32) -> Vec<String> {
    match reporting_period {
        "yearly" => vec![year.to_string()],
        "monthly" => (1..=12)
            .map(|month| format!("{year}-M{month:02}"))
            .collect(),
        _ => vec![
            format!("{year}-Q1"),
            format!("{year}-Q2"),
            format!("{year}-Q3"),
            format!("{year}-Q4"),
        ],
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FiscalPeriodSummary {
    pub id: String,
    pub fiscal_year_id: String,
    pub fiscal_year: i32,
    pub period_key: String,
    pub starts_on: String,
    pub ends_on: String,
    pub status: String,
}

pub async fn fiscal_period_list(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<FiscalPeriodSummary>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT fp.id, fp.fiscal_year_id, fp.period_key, fp.starts_on, fp.ends_on, fp.status,
               CAST(substr(fy.starts_on, 1, 4) AS INTEGER) AS fiscal_year
        FROM fiscal_periods fp
        JOIN fiscal_years fy ON fy.id = fp.fiscal_year_id
        WHERE fp.workspace_id = ?1
        ORDER BY fp.starts_on
        "#,
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| FiscalPeriodSummary {
            id: row.get("id"),
            fiscal_year_id: row.get("fiscal_year_id"),
            fiscal_year: row.get("fiscal_year"),
            period_key: row.get("period_key"),
            starts_on: row.get("starts_on"),
            ends_on: row.get("ends_on"),
            status: row.get("status"),
        })
        .collect())
}

fn validate_period_key_for_profile(reporting_period: &str, period_key: &str) -> Result<(), AppError> {
    if !period_key_matches_reporting_period(reporting_period, period_key) {
        return Err(AppError::validation(
            "Period key does not match VAT reporting period",
            "periodKey",
        ));
    }
    let _ = parse_period_key(period_key)?;
    Ok(())
}

pub async fn seed_vat_codes(pool: &SqlitePool, workspace_id: &str) -> Result<(), AppError> {
    let codes = [
        ("VAT25", 2500_i64, Some("05"), Some("10")),
        ("VAT12", 1200_i64, Some("06"), Some("11")),
        ("VAT6", 600_i64, Some("07"), Some("12")),
    ];
    for (code, rate_bp, output_box, input_box) in codes {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO vat_codes (
              id, workspace_id, code, rate_bp, output_box, input_box, deductible
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
            "#,
        )
        .bind(format!("vc-{workspace_id}-{code}"))
        .bind(workspace_id)
        .bind(code)
        .bind(rate_bp)
        .bind(output_box)
        .bind(input_box)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn ensure_fiscal_period(
    pool: &SqlitePool,
    workspace_id: &str,
    period_key: &str,
) -> Result<String, AppError> {
    let bounds = parse_period_key(period_key)?;
    let fiscal_year_id = fiscal_year_id_for_year(pool, workspace_id, bounds.fiscal_year).await?;
    let period_id = format!("fp-{workspace_id}-{period_key}");

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO fiscal_periods (
          id, workspace_id, fiscal_year_id, period_key, starts_on, ends_on, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open')
        "#,
    )
    .bind(&period_id)
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .bind(period_key)
    .bind(&bounds.starts_on)
    .bind(&bounds.ends_on)
    .execute(pool)
    .await?;

    Ok(period_id)
}

pub async fn ensure_fiscal_period_open(
    pool: &SqlitePool,
    workspace_id: &str,
    date: &str,
) -> Result<(), AppError> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", "date"))?;
    let date_str = parsed.format("%Y-%m-%d").to_string();

    let locked: Option<String> = sqlx::query_scalar(
        r#"
        SELECT period_key FROM fiscal_periods
        WHERE workspace_id = ?1
          AND status = 'locked'
          AND starts_on <= ?2
          AND ends_on >= ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&date_str)
    .fetch_optional(pool)
    .await?;

    if locked.is_some() {
        return Err(AppError::locked_period("Fiscal period is locked after VAT approval"));
    }
    Ok(())
}

pub async fn ensure_fiscal_period_open_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    date: &str,
) -> Result<(), AppError> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", "date"))?;
    let date_str = parsed.format("%Y-%m-%d").to_string();

    let locked: Option<String> = sqlx::query_scalar(
        r#"
        SELECT period_key FROM fiscal_periods
        WHERE workspace_id = ?1
          AND status = 'locked'
          AND starts_on <= ?2
          AND ends_on >= ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&date_str)
    .fetch_optional(&mut **tx)
    .await?;

    if locked.is_some() {
        return Err(AppError::locked_period("Fiscal period is locked after VAT approval"));
    }
    Ok(())
}

fn sales_box_for_code(vat_code: &str) -> &'static str {
    match vat_code {
        "VAT12" => "06",
        "VAT6" => "07",
        _ => "05",
    }
}

fn ex_vat_from_output_vat(vat_code: &str, vat_minor: i64) -> i64 {
    let rate_bp = match vat_code {
        "VAT12" => 1200,
        "VAT6" => 600,
        _ => 2500,
    };
    (vat_minor * 10_000 + rate_bp / 2) / rate_bp
}

fn output_vat_box_for_code(vat_code: Option<&str>) -> &'static str {
    match vat_code {
        Some("VAT12") => "11",
        Some("VAT6") => "12",
        _ => "10",
    }
}

async fn aggregate_vat_boxes(
    pool: &SqlitePool,
    workspace_id: &str,
    starts_on: &str,
    ends_on: &str,
) -> Result<Vec<VatReturnBox>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT v.id AS voucher_id,
               a.number AS account_number,
               jl.debit_minor,
               jl.credit_minor,
               jl.vat_code
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        JOIN accounts a ON a.id = jl.account_id
        WHERE v.workspace_id = ?1
          AND v.status = 'posted'
          AND date(COALESCE(v.accounting_date, v.posted_at)) >= date(?2)
          AND date(COALESCE(v.accounting_date, v.posted_at)) <= date(?3)
        ORDER BY v.id ASC, a.number ASC, jl.id ASC
        "#,
    )
    .bind(workspace_id)
    .bind(starts_on)
    .bind(ends_on)
    .fetch_all(pool)
    .await?;

    let ledger_lines: Vec<LedgerLineRow> = rows
        .into_iter()
        .map(|row| LedgerLineRow {
            voucher_id: row.get("voucher_id"),
            account_number: row.get("account_number"),
            debit_minor: row.get("debit_minor"),
            credit_minor: row.get("credit_minor"),
            vat_code: row.get("vat_code"),
        })
        .collect();

    Ok(compute_vat_boxes_from_ledger_lines(
        workspace_id,
        starts_on,
        ends_on,
        &ledger_lines,
    ))
}

async fn aggregate_vat_boxes_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    starts_on: &str,
    ends_on: &str,
) -> Result<Vec<VatReturnBox>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT v.id AS voucher_id,
               a.number AS account_number,
               jl.debit_minor,
               jl.credit_minor,
               jl.vat_code
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        JOIN accounts a ON a.id = jl.account_id
        WHERE v.workspace_id = ?1
          AND v.status = 'posted'
          AND date(COALESCE(v.accounting_date, v.posted_at)) >= date(?2)
          AND date(COALESCE(v.accounting_date, v.posted_at)) <= date(?3)
        ORDER BY v.id ASC, a.number ASC, jl.id ASC
        "#,
    )
    .bind(workspace_id)
    .bind(starts_on)
    .bind(ends_on)
    .fetch_all(&mut **tx)
    .await?;

    let ledger_lines: Vec<LedgerLineRow> = rows
        .into_iter()
        .map(|row| LedgerLineRow {
            voucher_id: row.get("voucher_id"),
            account_number: row.get("account_number"),
            debit_minor: row.get("debit_minor"),
            credit_minor: row.get("credit_minor"),
            vat_code: row.get("vat_code"),
        })
        .collect();

    Ok(compute_vat_boxes_from_ledger_lines(
        workspace_id,
        starts_on,
        ends_on,
        &ledger_lines,
    ))
}

fn compute_vat_boxes_from_ledger_lines(
    workspace_id: &str,
    starts_on: &str,
    ends_on: &str,
    rows: &[LedgerLineRow],
) -> Vec<VatReturnBox> {
    let mut box_amounts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut current_voucher: Option<String> = None;
    let mut revenue_net = 0_i64;
    let mut output_vat_by_code: std::collections::BTreeMap<String, i64> =
        std::collections::BTreeMap::new();
    let mut input_vat_net = 0_i64;

    let mut hasher = Sha256::new();
    hasher.update(workspace_id.as_bytes());
    hasher.update(starts_on.as_bytes());
    hasher.update(ends_on.as_bytes());

    let flush_voucher = |revenue_net: &mut i64,
                         output_vat_by_code: &mut std::collections::BTreeMap<String, i64>,
                         input_vat_net: &mut i64,
                         box_amounts: &mut std::collections::BTreeMap<String, i64>| {
        if output_vat_by_code.is_empty() {
            if *revenue_net != 0 {
                *box_amounts.entry("05".to_string()).or_insert(0) += *revenue_net;
            }
        } else {
            let mut allocated_ex = 0_i64;
            for (code, vat_amount) in output_vat_by_code.iter() {
                if *vat_amount == 0 {
                    continue;
                }
                let ex_vat = ex_vat_from_output_vat(code, *vat_amount);
                let sales_box = sales_box_for_code(code);
                *box_amounts.entry(sales_box.to_string()).or_insert(0) += ex_vat;
                allocated_ex += ex_vat;

                let vat_box = output_vat_box_for_code(Some(code.as_str()));
                *box_amounts.entry(vat_box.to_string()).or_insert(0) += *vat_amount;
            }
            if *revenue_net != 0 && allocated_ex != *revenue_net {
                let diff = *revenue_net - allocated_ex;
                *box_amounts.entry("05".to_string()).or_insert(0) += diff;
            }
        }
        if *input_vat_net != 0 {
            *box_amounts.entry("48".to_string()).or_insert(0) += *input_vat_net;
        }
        *revenue_net = 0;
        output_vat_by_code.clear();
        *input_vat_net = 0;
    };

    for row in rows {
        hasher.update(row.voucher_id.as_bytes());
        hasher.update(row.account_number.as_bytes());
        hasher.update(row.debit_minor.to_le_bytes());
        hasher.update(row.credit_minor.to_le_bytes());
        if let Some(code) = &row.vat_code {
            hasher.update(code.as_bytes());
        }

        if current_voucher.as_deref() != Some(row.voucher_id.as_str()) {
            if current_voucher.is_some() {
                flush_voucher(
                    &mut revenue_net,
                    &mut output_vat_by_code,
                    &mut input_vat_net,
                    &mut box_amounts,
                );
            }
            current_voucher = Some(row.voucher_id.clone());
        }

        match row.account_number.as_str() {
            "3041" => revenue_net += row.credit_minor - row.debit_minor,
            "2611" => {
                let code = row
                    .vat_code
                    .clone()
                    .unwrap_or_else(|| "VAT25".to_string());
                *output_vat_by_code.entry(code).or_insert(0) +=
                    row.credit_minor - row.debit_minor;
            }
            "2641" => input_vat_net += row.debit_minor - row.credit_minor,
            _ => {}
        }
    }

    if current_voucher.is_some() {
        flush_voucher(
            &mut revenue_net,
            &mut output_vat_by_code,
            &mut input_vat_net,
            &mut box_amounts,
        );
    }

    let output_vat_total: i64 = ["10", "11", "12"]
        .iter()
        .filter_map(|code| box_amounts.get(*code))
        .sum();
    let input_vat_total = box_amounts.get("48").copied().unwrap_or(0);
    box_amounts.insert("49".to_string(), output_vat_total - input_vat_total);

    let query_hash = format!("{:x}", hasher.finalize());

    box_amounts
        .into_iter()
        .map(|(box_code, amount_minor)| VatReturnBox {
            box_code,
            amount_minor,
            source_query_hash: Some(query_hash.clone()),
        })
        .collect()
}

pub fn current_reporting_period_key(reporting_period: &str, as_of: NaiveDate) -> String {
    match reporting_period {
        "yearly" => as_of.year().to_string(),
        "monthly" => format!("{}-M{:02}", as_of.year(), as_of.month()),
        _ => {
            let quarter = (as_of.month() - 1) / 3 + 1;
            format!("{}-Q{quarter}", as_of.year())
        }
    }
}

async fn box49_from_existing_return(
    pool: &SqlitePool,
    workspace_id: &str,
    period_key: &str,
) -> Result<Option<i64>, AppError> {
    let amount: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT vrb.amount_minor
        FROM vat_return_boxes vrb
        JOIN vat_returns vr ON vr.id = vrb.vat_return_id
        JOIN fiscal_periods fp ON fp.id = vr.fiscal_period_id
        WHERE vr.workspace_id = ?1
          AND fp.period_key = ?2
          AND vrb.box_code = '49'
        ORDER BY vr.created_at DESC
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(period_key)
    .fetch_optional(pool)
    .await?;
    Ok(amount)
}

pub async fn estimated_vat_reserve_minor(
    pool: &SqlitePool,
    workspace_id: &str,
    period_key: &str,
) -> Result<i64, AppError> {
    if let Some(box49) = box49_from_existing_return(pool, workspace_id, period_key).await? {
        return Ok(box49.max(0));
    }

    let bounds = parse_period_key(period_key)?;
    let boxes = aggregate_vat_boxes(pool, workspace_id, &bounds.starts_on, &bounds.ends_on).await?;
    let box49 = boxes
        .iter()
        .find(|b| b.box_code == "49")
        .map(|b| b.amount_minor)
        .unwrap_or(0);
    Ok(box49.max(0))
}


async fn load_vat_return_summary(
    pool: &SqlitePool,
    workspace_id: &str,
    vat_return_id: &str,
) -> Result<VatReturnSummary, AppError> {
    let row = sqlx::query(
        r#"
        SELECT vr.id, vr.fiscal_period_id, fp.period_key, vr.status, vr.rule_version_id, vr.export_path
        FROM vat_returns vr
        JOIN fiscal_periods fp ON fp.id = vr.fiscal_period_id
        WHERE vr.workspace_id = ?1 AND vr.id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(vat_return_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("VAT return not found", "vatReturnId"))?;

    let boxes_rows = sqlx::query(
        r#"
        SELECT box_code, amount_minor, source_query_hash
        FROM vat_return_boxes
        WHERE vat_return_id = ?1
        ORDER BY box_code ASC
        "#,
    )
    .bind(vat_return_id)
    .fetch_all(pool)
    .await?;

    let boxes: Vec<VatReturnBox> = boxes_rows
        .into_iter()
        .map(|b| VatReturnBox {
            box_code: b.get("box_code"),
            amount_minor: b.get("amount_minor"),
            source_query_hash: b.get("source_query_hash"),
        })
        .collect();

    let box49 = boxes
        .iter()
        .find(|b| b.box_code == "49")
        .map(|b| b.amount_minor)
        .unwrap_or(0);

    Ok(VatReturnSummary {
        id: row.get("id"),
        fiscal_period_id: row.get("fiscal_period_id"),
        period_key: row.get("period_key"),
        status: row.get("status"),
        rule_version_id: row.get("rule_version_id"),
        boxes,
        box49_amount_minor: box49,
        zero_return: box49 == 0,
        export_path: row.get("export_path"),
    })
}

async fn check_draft_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentVatReturnPayload>, AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_VAT_RETURN_DRAFT)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(json) = payload else {
        return Ok(None);
    };
    let parsed: IdempotentVatReturnPayload =
        serde_json::from_str(&json).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Some(parsed))
}

fn validate_draft_idempotency_match(
    period_key: &str,
    cached: &IdempotentVatReturnPayload,
) -> Result<(), AppError> {
    if cached.period_key != period_key {
        return Err(AppError::validation(
            "Idempotency key was already used for a different VAT period",
            "idempotencyKey",
        ));
    }
    Ok(())
}

pub async fn vat_return_draft_create(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &VatReturnDraftCreateInput,
) -> Result<VatReturnSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;
    let period_key = input.period_key.trim();

    if let Some(cached) = check_draft_idempotency(pool, workspace_id, idempotency_key).await? {
        validate_draft_idempotency_match(period_key, &cached)?;
        return load_vat_return_summary(pool, workspace_id, &cached.vat_return_id).await;
    }

    let vat_profile = get_vat_profile(pool, workspace_id).await?;
    let reporting_period = vat_profile
        .as_ref()
        .map(|p| p.reporting_period.as_str())
        .unwrap_or("quarterly");
    validate_period_key_for_profile(reporting_period, period_key)?;

    let status = vat_profile
        .as_ref()
        .map(|p| p.vat_status.as_str())
        .unwrap_or("exempt_low_turnover");
    if status != "registered" && status != "voluntary_registered" {
        return Err(AppError::validation(
            "VAT return requires a registered VAT profile",
            "vatStatus",
        ));
    }

    seed_vat_codes(pool, workspace_id).await?;
    let fiscal_period_id = ensure_fiscal_period(pool, workspace_id, period_key).await?;

    let period_locked: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status FROM fiscal_periods WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(&fiscal_period_id)
    .fetch_optional(pool)
    .await?;
    if period_locked.as_deref() == Some("locked") {
        return Err(AppError::locked_period("Fiscal period is already locked"));
    }

    let bounds = parse_period_key(period_key)?;
    let boxes = aggregate_vat_boxes(pool, workspace_id, &bounds.starts_on, &bounds.ends_on).await?;

    let rule_version = get_active_rule_version(pool)
        .await?
        .ok_or_else(|| AppError::internal("No active rule version"))?;
    let rule_version_id = rule_version.id;

    let mut tx = pool.begin().await?;

    let existing_return: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM vat_returns
        WHERE workspace_id = ?1 AND fiscal_period_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&fiscal_period_id)
    .fetch_optional(&mut *tx)
    .await?;

    let vat_return_id = if let Some(id) = existing_return {
        if sqlx::query_scalar::<_, String>(
            r#"
            SELECT status FROM vat_returns WHERE id = ?1 LIMIT 1
            "#,
        )
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .as_deref()
            == Some("approved")
        {
            tx.rollback().await?;
            return Err(AppError::validation(
                "VAT return for this period is already approved",
                "periodKey",
            ));
        }
        sqlx::query("DELETE FROM vat_return_boxes WHERE vat_return_id = ?1")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        id
    } else {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO vat_returns (id, workspace_id, fiscal_period_id, status, rule_version_id)
            VALUES (?1, ?2, ?3, 'draft', ?4)
            "#,
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(&fiscal_period_id)
        .bind(&rule_version_id)
        .execute(&mut *tx)
        .await?;
        id
    };

    for b in &boxes {
        sqlx::query(
            r#"
            INSERT INTO vat_return_boxes (id, vat_return_id, box_code, amount_minor, source_query_hash)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&vat_return_id)
        .bind(&b.box_code)
        .bind(b.amount_minor)
        .bind(b.source_query_hash.as_deref())
        .execute(&mut *tx)
        .await?;
    }

    let payload = IdempotentVatReturnPayload {
        vat_return_id: vat_return_id.clone(),
        period_key: period_key.to_string(),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|e| AppError::internal(e.to_string()))?;

    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_VAT_RETURN_DRAFT)
    .bind(payload_json)
    .bind(idempotency_key)
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            tx.rollback().await?;
            if let Some(cached) = check_draft_idempotency(pool, workspace_id, idempotency_key).await?
            {
                validate_draft_idempotency_match(period_key, &cached)?;
                return load_vat_return_summary(pool, workspace_id, &cached.vat_return_id).await;
            }
            return Err(error.into());
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error.into());
        }
    }

    record_event_tx(
        &mut *tx,
        workspace_id,
        "vat_return_draft_create",
        "vat_return",
        Some(&vat_return_id),
        &serde_json::json!({ "periodKey": period_key, "ruleVersionId": rule_version_id }).to_string(),
    )
    .await?;

    tx.commit().await?;
    load_vat_return_summary(pool, workspace_id, &vat_return_id).await
}

pub async fn vat_return_get(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &VatReturnGetInput,
) -> Result<VatReturnSummary, AppError> {
    load_vat_return_summary(pool, workspace_id, &input.vat_return_id).await
}

fn validate_approve_idempotency_match(
    vat_return_id: &str,
    cached: &IdempotentVatApprovePayload,
) -> Result<(), AppError> {
    if cached.vat_return_id != vat_return_id {
        return Err(AppError::validation(
            "Idempotency key was already used for a different VAT return",
            "idempotencyKey",
        ));
    }
    Ok(())
}

async fn check_approve_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentVatApprovePayload>, AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_VAT_RETURN_APPROVE)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(json) = payload else {
        return Ok(None);
    };
    let parsed: IdempotentVatApprovePayload =
        serde_json::from_str(&json).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Some(parsed))
}

async fn persist_vat_return_boxes_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    vat_return_id: &str,
    boxes: &[VatReturnBox],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM vat_return_boxes WHERE vat_return_id = ?1")
        .bind(vat_return_id)
        .execute(&mut **tx)
        .await?;

    for b in boxes {
        sqlx::query(
            r#"
            INSERT INTO vat_return_boxes (id, vat_return_id, box_code, amount_minor, source_query_hash)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(vat_return_id)
        .bind(&b.box_code)
        .bind(b.amount_minor)
        .bind(b.source_query_hash.as_deref())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn vat_return_approve(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &VatReturnApproveInput,
) -> Result<VatReturnSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;

    if let Some(cached) = check_approve_idempotency(pool, workspace_id, idempotency_key).await? {
        validate_approve_idempotency_match(&input.vat_return_id, &cached)?;
        return load_vat_return_summary(pool, workspace_id, &cached.vat_return_id).await;
    }

    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        SELECT vr.id, vr.fiscal_period_id, vr.status, fp.fiscal_year_id,
               fp.period_key, fp.starts_on, fp.ends_on
        FROM vat_returns vr
        JOIN fiscal_periods fp ON fp.id = vr.fiscal_period_id
        WHERE vr.workspace_id = ?1 AND vr.id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&input.vat_return_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::validation("VAT return not found", "vatReturnId"))?;

    let status: String = row.get("status");
    if status == "approved" {
        tx.rollback().await?;
        return load_vat_return_summary(pool, workspace_id, &input.vat_return_id).await;
    }

    let fiscal_period_id: String = row.get("fiscal_period_id");
    let fiscal_year_id: String = row.get("fiscal_year_id");
    let starts_on: String = row.get("starts_on");
    let ends_on: String = row.get("ends_on");

    ensure_fiscal_year_open_tx(&mut *tx, &fiscal_year_id).await?;

    let fresh_boxes =
        aggregate_vat_boxes_tx(&mut tx, workspace_id, &starts_on, &ends_on).await?;
    persist_vat_return_boxes_tx(&mut tx, &input.vat_return_id, &fresh_boxes).await?;

    let updated = sqlx::query(
        r#"
        UPDATE vat_returns
        SET status = 'approved', approved_at = CURRENT_TIMESTAMP
        WHERE id = ?1 AND status = 'draft'
        "#,
    )
    .bind(&input.vat_return_id)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        return Err(AppError::validation(
            "VAT return is not in draft status",
            "vatReturnId",
        ));
    }

    sqlx::query(
        r#"
        UPDATE fiscal_periods
        SET status = 'locked'
        WHERE id = ?1 AND status = 'open'
        "#,
    )
    .bind(&fiscal_period_id)
    .execute(&mut *tx)
    .await?;

    let payload = IdempotentVatApprovePayload {
        vat_return_id: input.vat_return_id.clone(),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|e| AppError::internal(e.to_string()))?;

    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_VAT_RETURN_APPROVE)
    .bind(payload_json)
    .bind(idempotency_key)
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            tx.rollback().await?;
            if let Some(cached) =
                check_approve_idempotency(pool, workspace_id, idempotency_key).await?
            {
                validate_approve_idempotency_match(&input.vat_return_id, &cached)?;
                return load_vat_return_summary(pool, workspace_id, &cached.vat_return_id).await;
            }
            return Err(error.into());
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error.into());
        }
    }

    record_event_tx(
        &mut *tx,
        workspace_id,
        "vat_return_approved",
        "vat_return",
        Some(&input.vat_return_id),
        &serde_json::json!({ "fiscalPeriodId": fiscal_period_id }).to_string(),
    )
    .await?;

    tx.commit().await?;
    load_vat_return_summary(pool, workspace_id, &input.vat_return_id).await
}

pub async fn vat_return_export(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &VatReturnExportInput,
) -> Result<VatReturnSummary, AppError> {
    let summary = load_vat_return_summary(pool, workspace_id, &input.vat_return_id).await?;
    if summary.status != "approved" {
        return Err(AppError::validation(
            "Only approved VAT returns can be exported",
            "vatReturnId",
        ));
    }

    let (exports_path, database_path): (String, String) = sqlx::query_as(
        r#"
        SELECT exports_path, database_path FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Workspace not found", "workspaceId"))?;

    let export_dir =
        crate::workspace::resolve_workspace_exports_dir(&exports_path, &database_path)?
            .join("vat-returns");
    std::fs::create_dir_all(&export_dir).map_err(AppError::from)?;

    let filename = format!(
        "vat-return-{}-{}.json",
        summary.period_key,
        summary.id
    );
    let export_path = export_dir.join(&filename);
    let export_json = serde_json::json!({
        "periodKey": summary.period_key,
        "ruleVersionId": summary.rule_version_id,
        "boxes": summary.boxes,
        "box49AmountMinor": summary.box49_amount_minor,
        "zeroReturn": summary.zero_return,
        "sourceUrl": "https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/declaringtaxesbusinesses/vatdeclarations.4.12815e4f14a62bc048f52be.html"
    });
    std::fs::write(
        &export_path,
        serde_json::to_string_pretty(&export_json).map_err(|e| AppError::internal(e.to_string()))?,
    )?;

    let rel_path = format!("vat-returns/{filename}");
    sqlx::query(
        r#"
        UPDATE vat_returns SET export_path = ?1 WHERE id = ?2
        "#,
    )
    .bind(&rel_path)
    .bind(&input.vat_return_id)
    .execute(pool)
    .await?;

    record_event(
        pool,
        workspace_id,
        "vat_return_export",
        "vat_return",
        Some(&input.vat_return_id),
        &serde_json::json!({ "exportPath": rel_path }).to_string(),
    )
    .await?;

    let published = crate::paths::publish_export_artifact(
        pool,
        workspace_id,
        &exports_path,
        &database_path,
        &rel_path,
        input.export_directory.as_deref(),
        &filename,
    )
    .await?;

    let mut summary =
        load_vat_return_summary(pool, workspace_id, &input.vat_return_id).await?;
    summary.export_path = Some(published);
    Ok(summary)
}

pub async fn vat_threshold_status(
    pool: &SqlitePool,
    workspace_id: &str,
    rule_year: i32,
) -> Result<VatThresholdStatus, AppError> {
    let threshold = require_rule_i64(pool, "vat", "annual_turnover_threshold_minor").await?;
    let warning_ratio = get_rule_i64(pool, "vat", "threshold_warning_ratio")
        .await?
        .unwrap_or(75);

    let fiscal_year_id = format!("fy-{workspace_id}-{rule_year}");
    let turnover = net_revenue_minor_for_fiscal_year(pool, workspace_id, &fiscal_year_id).await?;

    let vat_profile = get_vat_profile(pool, workspace_id).await?;
    let vat_status = vat_profile
        .map(|p| p.vat_status)
        .unwrap_or_else(|| "exempt_low_turnover".to_string());

    let warning = if turnover > threshold {
        "breached"
    } else if turnover as f64 >= threshold as f64 * (warning_ratio as f64 / 100.0) {
        "approaching"
    } else {
        "none"
    };

    let must_charge_vat = matches!(vat_status.as_str(), "registered" | "voluntary_registered")
        || (vat_status == "exempt_low_turnover" && turnover > threshold);

    Ok(VatThresholdStatus {
        annual_turnover_minor: turnover,
        threshold_minor: threshold,
        warning: warning.to_string(),
        must_register_for_vat: turnover > threshold && vat_status == "exempt_low_turnover",
        must_charge_vat,
    })
}

pub async fn period_is_locked(
    pool: &SqlitePool,
    workspace_id: &str,
    period_key: &str,
) -> Result<bool, AppError> {
    let status: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status FROM fiscal_periods
        WHERE workspace_id = ?1 AND period_key = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(period_key)
    .fetch_optional(pool)
    .await?;
    Ok(status.as_deref() == Some("locked"))
}

pub async fn has_business_activity(
    pool: &SqlitePool,
    workspace_id: &str,
    fiscal_year_id: &str,
) -> Result<bool, AppError> {
    let invoice_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM invoices
        WHERE workspace_id = ?1 AND fiscal_year_id = ?2 AND status IN ('issued', 'credited')
        "#,
    )
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .fetch_one(pool)
    .await?;

    let voucher_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM vouchers
        WHERE workspace_id = ?1 AND fiscal_year_id = ?2 AND status = 'posted'
          AND source_type IN ('expense', 'reconciliation')
        "#,
    )
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .fetch_one(pool)
    .await?;

    Ok(invoice_count > 0 || voucher_count > 0)
}
