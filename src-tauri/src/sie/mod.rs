use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

use crate::{
    audit::record_event,
    error::AppError,
    workspace::{fiscal_year_id_for_year, resolve_workspace_exports_dir},
};

const JOB_SIE_EXPORT: &str = "sie_export_create";
const PROGRAM_NAME: &str = "ÖppenBokföring";
const PROGRAM_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SieExportSummary {
    pub export_path: String,
    pub fiscal_year: i32,
    pub voucher_count: usize,
    pub account_count: usize,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SieExportCreateInput {
    pub fiscal_year: i32,
    pub idempotency_key: String,
    pub export_directory: Option<String>,
}

fn normalize_idempotency_key(key: &str) -> Result<String, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(
            "Idempotency key is required",
            "idempotencyKey",
        ));
    }
    Ok(trimmed.to_string())
}

fn sanitize_sie_text(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('"', "'")
        .trim()
        .to_string()
}

pub fn format_sie_amount(signed_minor: i64) -> String {
    let negative = signed_minor < 0;
    let abs_minor = signed_minor.unsigned_abs();
    let major = abs_minor / 100;
    let cents = abs_minor % 100;
    if negative {
        format!("-{major}.{cents:02}")
    } else {
        format!("{major}.{cents:02}")
    }
}

struct SieAccount {
    number: String,
    name: String,
    opening_minor: i64,
    closing_minor: i64,
}

struct SieVoucher {
    index: usize,
    accounting_date: String,
    source_type: String,
    source_id: Option<String>,
    lines: Vec<SieLine>,
}

struct SieLine {
    account_number: String,
    debit_minor: i64,
    credit_minor: i64,
    text: String,
}

pub(crate) fn render_sie_type4(
    company_name: &str,
    fiscal_year: i32,
    accounts: &[SieAccount],
    vouchers: &[SieVoucher],
) -> String {
    let today = Utc::now().format("%Y%m%d").to_string();
    let mut lines = vec![
        "#FLAGGA 0".to_string(),
        format!("#PROGRAM \"{PROGRAM_NAME}\" \"{PROGRAM_VERSION}\""),
        "#FORMAT PC8".to_string(),
        format!("#GEN {today}"),
        "#SIETYP 4".to_string(),
        format!("#FNAMN \"{}\"", sanitize_sie_text(company_name)),
        "#VALUTA SEK".to_string(),
        format!("#RAR 0 {fiscal_year}0101 {fiscal_year}1231"),
    ];

    for account in accounts {
        lines.push(format!(
            "#KONTO {} \"{}\"",
            account.number,
            sanitize_sie_text(&account.name)
        ));
        if account.opening_minor != 0 {
            lines.push(format!(
                "#IB 0 {} {}",
                account.number,
                format_sie_amount(account.opening_minor)
            ));
        }
        if account.closing_minor != 0 {
            lines.push(format!(
                "#UB 0 {} {}",
                account.number,
                format_sie_amount(account.closing_minor)
            ));
        }
    }

    for voucher in vouchers {
        let ver_text = sanitize_sie_text(&format!(
            "{} {}",
            voucher.source_type,
            voucher.source_id.as_deref().unwrap_or("")
        ));
        lines.push(format!(
            "#VER \"A\" \"{}\" {} \"{}\"",
            voucher.index, voucher.accounting_date, ver_text
        ));
        for line in &voucher.lines {
            let signed_minor = line.debit_minor - line.credit_minor;
            lines.push(format!(
                "#TRANS {} {{}} {} {} \"{}\"",
                line.account_number,
                format_sie_amount(signed_minor),
                voucher.accounting_date,
                sanitize_sie_text(&line.text)
            ));
        }
        lines.push("#VEREND".to_string());
    }

    lines.join("\r\n") + "\r\n"
}

async fn load_sie_data(
    pool: &SqlitePool,
    workspace_id: &str,
    fiscal_year: i32,
) -> Result<(String, Vec<SieAccount>, Vec<SieVoucher>), AppError> {
    let fiscal_year_id = fiscal_year_id_for_year(pool, workspace_id, fiscal_year).await?;

    sqlx::query(
        r#"
        SELECT starts_on, ends_on FROM fiscal_years WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(&fiscal_year_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Fiscal year not found", "fiscalYear"))?;

    let company_name: Option<String> = sqlx::query_scalar(
        r#"
        SELECT business_name FROM sole_trader_profiles
        WHERE workspace_id = ?1
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    let company_name = company_name.unwrap_or_else(|| "Enskild firma".to_string());

    let account_rows = sqlx::query(
        r#"
        SELECT a.number, a.name,
          COALESCE(SUM(jl.debit_minor - jl.credit_minor), 0) AS balance_minor
        FROM accounts a
        LEFT JOIN journal_lines jl ON jl.account_id = a.id
        LEFT JOIN vouchers v ON v.id = jl.voucher_id
          AND v.workspace_id = ?1
          AND v.fiscal_year_id = ?2
          AND v.status = 'posted'
        WHERE a.workspace_id = ?1
        GROUP BY a.id, a.number, a.name
        ORDER BY a.number
        "#,
    )
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .fetch_all(pool)
    .await?;

    let voucher_rows = sqlx::query(
        r#"
        SELECT id, accounting_date, source_type, source_id
        FROM vouchers
        WHERE workspace_id = ?1 AND fiscal_year_id = ?2 AND status = 'posted'
        ORDER BY accounting_date, created_at, id
        "#,
    )
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .fetch_all(pool)
    .await?;

    let line_rows = sqlx::query(
        r#"
        SELECT jl.voucher_id, a.number AS account_number, jl.debit_minor, jl.credit_minor, v.source_type
        FROM journal_lines jl
        JOIN accounts a ON a.id = jl.account_id
        JOIN vouchers v ON v.id = jl.voucher_id
        WHERE v.workspace_id = ?1 AND v.fiscal_year_id = ?2 AND v.status = 'posted'
        ORDER BY jl.voucher_id, a.number
        "#,
    )
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .fetch_all(pool)
    .await?;

    let mut lines_by_voucher: BTreeMap<String, Vec<SieLine>> = BTreeMap::new();
    let mut referenced_accounts = BTreeSet::new();
    for row in line_rows {
        let voucher_id: String = row.get("voucher_id");
        let account_number: String = row.get("account_number");
        referenced_accounts.insert(account_number.clone());
        lines_by_voucher
            .entry(voucher_id)
            .or_default()
            .push(SieLine {
                account_number,
                debit_minor: row.get("debit_minor"),
                credit_minor: row.get("credit_minor"),
                text: row.get("source_type"),
            });
    }

    let all_accounts: Vec<SieAccount> = account_rows
        .iter()
        .map(|row| {
            let balance: i64 = row.get("balance_minor");
            SieAccount {
                number: row.get("number"),
                name: row.get("name"),
                opening_minor: 0,
                closing_minor: balance,
            }
        })
        .collect();

    let accounts: Vec<SieAccount> = all_accounts
        .into_iter()
        .filter(|account| {
            referenced_accounts.contains(&account.number) || account.closing_minor != 0
        })
        .collect();

    let mut vouchers = Vec::new();
    for (index, row) in voucher_rows.iter().enumerate() {
        let voucher_id: String = row.get("id");
        let accounting_date: String = row
            .get::<Option<String>, _>("accounting_date")
            .unwrap_or_else(|| format!("{fiscal_year}0101"));
        let accounting_date = accounting_date.replace('-', "");

        let lines = lines_by_voucher.remove(&voucher_id).unwrap_or_default();

        vouchers.push(SieVoucher {
            index: index + 1,
            accounting_date,
            source_type: row.get("source_type"),
            source_id: row.get("source_id"),
            lines,
        });
    }

    Ok((company_name, accounts, vouchers))
}

fn idempotency_fiscal_year_mismatch() -> AppError {
    AppError::validation(
        "Idempotency key was already used for a different fiscal year",
        "idempotencyKey",
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentSieExportPayload {
    fiscal_year: i32,
    summary: SieExportSummary,
}

async fn check_sie_export_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentSieExportPayload>, AppError> {
    let payload: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_SIE_EXPORT)
    .bind(idempotency_key)
    .fetch_optional(pool)
    .await?;

    let Some(json) = payload else {
        return Ok(None);
    };

    let parsed: IdempotentSieExportPayload =
        serde_json::from_str(&json).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Some(parsed))
}

async fn publish_sie_summary(
    pool: &SqlitePool,
    workspace_id: &str,
    exports_path: &str,
    database_path: &str,
    summary: SieExportSummary,
    export_directory: Option<&str>,
    rel_path: &str,
) -> Result<SieExportSummary, AppError> {
    let filename = std::path::Path::new(rel_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export.se");
    let published_path = crate::paths::publish_export_artifact(
        pool,
        workspace_id,
        exports_path,
        database_path,
        rel_path,
        export_directory,
        filename,
    )
    .await?;
    Ok(SieExportSummary {
        export_path: published_path,
        fiscal_year: summary.fiscal_year,
        voucher_count: summary.voucher_count,
        account_count: summary.account_count,
    })
}

async fn ensure_sie_export_file(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    cached: &IdempotentSieExportPayload,
    export_directory: Option<&str>,
) -> Result<SieExportSummary, AppError> {
    let (exports_path, database_path): (String, String) = sqlx::query_as(
        r#"
        SELECT exports_path, database_path FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let export_root = resolve_workspace_exports_dir(&exports_path, &database_path)?;
    let export_path = export_root.join(&cached.summary.export_path);
    let (company_name, accounts, vouchers) =
        load_sie_data(pool, workspace_id, cached.fiscal_year).await?;
    let summary = SieExportSummary {
        export_path: cached.summary.export_path.clone(),
        fiscal_year: cached.fiscal_year,
        voucher_count: vouchers.len(),
        account_count: accounts.len(),
    };

    if export_path.is_file()
        && summary.voucher_count == cached.summary.voucher_count
        && summary.account_count == cached.summary.account_count
    {
        return publish_sie_summary(
            pool,
            workspace_id,
            &exports_path,
            &database_path,
            summary,
            export_directory,
            &cached.summary.export_path,
        )
        .await;
    }

    let content = render_sie_type4(&company_name, cached.fiscal_year, &accounts, &vouchers);

    let content_for_write = content;
    let export_path_for_write = export_path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        if let Some(parent) = export_path_for_write.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::from)?;
        }
        std::fs::write(&export_path_for_write, content_for_write).map_err(AppError::from)
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))??;

    update_sie_export_payload(
        pool,
        workspace_id,
        idempotency_key,
        &IdempotentSieExportPayload {
            fiscal_year: cached.fiscal_year,
            summary: summary.clone(),
        },
    )
    .await?;

    publish_sie_summary(
        pool,
        workspace_id,
        &exports_path,
        &database_path,
        summary,
        export_directory,
        &cached.summary.export_path,
    )
    .await
}

async fn update_sie_export_payload(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    payload: &IdempotentSieExportPayload,
) -> Result<(), AppError> {
    let payload_json = serde_json::to_string(payload).map_err(|e| AppError::internal(e.to_string()))?;
    sqlx::query(
        r#"
        UPDATE local_jobs
        SET payload_json = ?1
        WHERE workspace_id = ?2 AND job_type = ?3 AND idempotency_key = ?4
        "#,
    )
    .bind(&payload_json)
    .bind(workspace_id)
    .bind(JOB_SIE_EXPORT)
    .bind(idempotency_key)
    .execute(pool)
    .await?;
    Ok(())
}

async fn write_sie_export_file(
    exports_path: &str,
    database_path: &str,
    rel_path: &str,
    content: String,
) -> Result<(), AppError> {
    let export_dir = resolve_workspace_exports_dir(exports_path, database_path)?;
    let export_path = export_dir.join(rel_path);
    let parent = export_path
        .parent()
        .ok_or_else(|| AppError::internal("Invalid SIE export path".to_string()))?
        .to_path_buf();

    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        std::fs::create_dir_all(&parent).map_err(AppError::from)?;
        std::fs::write(&export_path, content).map_err(AppError::from)
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))??;

    Ok(())
}

pub async fn sie_export_create(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &SieExportCreateInput,
) -> Result<SieExportSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;

    if let Some(cached) =
        check_sie_export_idempotency(pool, workspace_id, &idempotency_key).await?
    {
        if cached.fiscal_year != input.fiscal_year {
            return Err(idempotency_fiscal_year_mismatch());
        }
        return ensure_sie_export_file(
            pool,
            workspace_id,
            &idempotency_key,
            &cached,
            input.export_directory.as_deref(),
        )
        .await;
    }

    let (company_name, accounts, vouchers) =
        load_sie_data(pool, workspace_id, input.fiscal_year).await?;

    let content = render_sie_type4(&company_name, input.fiscal_year, &accounts, &vouchers);

    let (exports_path, database_path): (String, String) = sqlx::query_as(
        r#"
        SELECT exports_path, database_path FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let filename = format!("sie-{}-{}.se", workspace_id, input.fiscal_year);
    let rel_path = format!("sie/{filename}");

    write_sie_export_file(&exports_path, &database_path, &rel_path, content).await?;

    let summary = SieExportSummary {
        export_path: rel_path.clone(),
        fiscal_year: input.fiscal_year,
        voucher_count: vouchers.len(),
        account_count: accounts.len(),
    };

    let payload = serde_json::to_string(&IdempotentSieExportPayload {
        fiscal_year: input.fiscal_year,
        summary: summary.clone(),
    })
    .map_err(|e| AppError::internal(e.to_string()))?;

    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_SIE_EXPORT)
    .bind(&payload)
    .bind(&idempotency_key)
    .execute(pool)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            if let Some(cached) =
                check_sie_export_idempotency(pool, workspace_id, &idempotency_key).await?
            {
                if cached.fiscal_year != input.fiscal_year {
                    return Err(idempotency_fiscal_year_mismatch());
                }
                return ensure_sie_export_file(
            pool,
            workspace_id,
            &idempotency_key,
            &cached,
            input.export_directory.as_deref(),
        )
        .await;
            }
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    }

    record_event(
        pool,
        workspace_id,
        "sie_export_create",
        "sie_export",
        Some(&idempotency_key),
        &serde_json::json!({
            "exportPath": rel_path,
            "fiscalYear": input.fiscal_year,
            "voucherCount": vouchers.len(),
        })
        .to_string(),
    )
    .await?;

    let published_path = crate::paths::publish_export_artifact(
        pool,
        workspace_id,
        &exports_path,
        &database_path,
        &rel_path,
        input.export_directory.as_deref(),
        &filename,
    )
    .await?;

    Ok(SieExportSummary {
        export_path: published_path,
        fiscal_year: summary.fiscal_year,
        voucher_count: summary.voucher_count,
        account_count: summary.account_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sie_render_includes_required_headers() {
        let content = render_sie_type4("Test AB", 2026, &[], &[]);
        assert!(content.contains("#SIETYP 4"));
        assert!(content.contains("#FORMAT PC8"));
        assert!(content.contains("#FNAMN \"Test AB\""));
        assert!(content.contains("#RAR 0 20260101 20261231"));
    }

    #[test]
    fn sie_sanitizes_control_characters() {
        assert_eq!(sanitize_sie_text("line\nbreak"), "linebreak");
    }

    #[test]
    fn sie_amount_uses_integer_precision() {
        assert_eq!(format_sie_amount(1_234_567_890_12), "1234567890.12");
        assert_eq!(format_sie_amount(-1_234_567_890_12), "-1234567890.12");
    }

    #[test]
    fn sie_includes_zero_balance_referenced_accounts() {
        let accounts = vec![
            SieAccount {
                number: "1510".to_string(),
                name: "Receivable".to_string(),
                opening_minor: 0,
                closing_minor: 0,
            },
            SieAccount {
                number: "3041".to_string(),
                name: "Revenue".to_string(),
                opening_minor: 0,
                closing_minor: 1_000_000,
            },
        ];
        let vouchers = vec![SieVoucher {
            index: 1,
            accounting_date: "20260315".to_string(),
            source_type: "credit_note".to_string(),
            source_id: Some("cn-1".to_string()),
            lines: vec![
                SieLine {
                    account_number: "1510".to_string(),
                    debit_minor: 0,
                    credit_minor: 1_000_000,
                    text: "credit_note".to_string(),
                },
                SieLine {
                    account_number: "3041".to_string(),
                    debit_minor: 1_000_000,
                    credit_minor: 0,
                    text: "credit_note".to_string(),
                },
            ],
        }];
        let content = render_sie_type4("Test AB", 2026, &accounts, &vouchers);
        assert!(content.contains("#KONTO 1510"));
        assert!(content.contains("#TRANS 1510 {}"));
    }
}
