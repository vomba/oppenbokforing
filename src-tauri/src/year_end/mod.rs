use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

use crate::{
    audit::{record_event, record_event_tx},
    error::AppError,
    rules::{get_active_rule_version, get_rule_bool, get_rule_string, get_rule_version_by_id},
    workspace::{ensure_fiscal_year_open_tx, fiscal_year_id_for_year},
};

const JOB_YEAR_END_CREATE: &str = "year_end_package_create";
const JOB_YEAR_END_APPROVE: &str = "year_end_package_approve";
const JOB_YEAR_END_EXPORT: &str = "year_end_package_export";
const K1_REGIME: &str = "k1_simplified_annual_accounts";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentYearEndPayload {
    package_id: String,
    fiscal_year: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentYearEndExportPayload {
    package_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NeFieldSummary {
    pub field_code: String,
    pub amount_minor: i64,
    pub source_type: String,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndPackageSummary {
    pub id: String,
    pub fiscal_year_id: String,
    pub fiscal_year: i32,
    pub status: String,
    pub rule_version_id: String,
    pub k1_allowed: bool,
    pub ne_draft_present: bool,
    pub stored_locally: bool,
    pub export_path: Option<String>,
    pub fiscal_year_locked: bool,
    pub ne_fields: Vec<NeFieldSummary>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndPackageCreateInput {
    pub fiscal_year: i32,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndPackageGetInput {
    pub package_id: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndPackageFindInput {
    pub fiscal_year: i32,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndPackageApproveInput {
    pub package_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndPackageExportInput {
    pub package_id: String,
    pub idempotency_key: String,
    pub export_directory: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndReadinessInput {
    pub fiscal_year: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndReadinessItem {
    pub code: String,
    pub satisfied: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YearEndReadiness {
    pub items: Vec<YearEndReadinessItem>,
    pub ready_to_approve: bool,
}

struct LedgerSnapshot {
    revenue_minor: i64,
    expense_minor: i64,
    business_result_minor: i64,
    account_balances: Vec<(String, String, i64)>,
}

struct NeFieldDraft {
    field_code: String,
    amount_minor: i64,
    source_ref: String,
}

fn normalize_idempotency_key(key: &str) -> Result<&str, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Idempotency key is required", "idempotencyKey"));
    }
    Ok(trimmed)
}

fn resolve_workspace_subdir(
    subdir_path: &str,
    database_path: &str,
    label: &str,
) -> Result<PathBuf, AppError> {
    let subdir = PathBuf::from(subdir_path);
    if subdir.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(AppError::validation(format!("Invalid {label} path"), label));
    }

    let data_root = Path::new(database_path)
        .parent()
        .ok_or_else(|| AppError::validation("Invalid database path", "databasePath"))?;

    std::fs::create_dir_all(&subdir).map_err(AppError::from)?;
    let canonical_subdir = subdir
        .canonicalize()
        .map_err(|e| AppError::storage(format!("Cannot resolve {label} path: {e}")))?;
    let canonical_root = data_root
        .canonicalize()
        .map_err(|e| AppError::storage(format!("Cannot resolve workspace data path: {e}")))?;

    if !canonical_subdir.starts_with(&canonical_root) {
        return Err(AppError::validation(
            format!("{label} path must be inside workspace data directory"),
            label,
        ));
    }

    Ok(canonical_subdir)
}

async fn load_workspace_paths(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<(String, String, String), AppError> {
    sqlx::query_as(
        r#"
        SELECT documents_path, exports_path, database_path
        FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Workspace not found", "workspaceId"))
}

async fn k1_regime_allowed(pool: &SqlitePool) -> Result<bool, AppError> {
    let regime = get_rule_string(pool, "year_end", "accounting_regime").await?;
    Ok(regime.as_deref() == Some(K1_REGIME))
}

async fn ne_draft_required(pool: &SqlitePool) -> Result<bool, AppError> {
    Ok(get_rule_bool(pool, "year_end", "ne_draft_required")
        .await?
        .unwrap_or(false))
}

async fn expense_total_minor_for_fiscal_year<'e, E>(
    executor: E,
    workspace_id: &str,
    fiscal_year_id: &str,
) -> Result<i64, AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.debit_minor - jl.credit_minor), 0)
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        JOIN accounts a ON a.id = jl.account_id
        WHERE v.workspace_id = ?1
          AND v.fiscal_year_id = ?2
          AND v.status = 'posted'
          AND a.account_type = 'expense'
        "#,
    )
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .fetch_one(executor)
    .await?;

    Ok(total)
}

async fn account_balances_for_fiscal_year<'e, E>(
    executor: E,
    workspace_id: &str,
    fiscal_year_id: &str,
) -> Result<Vec<(String, String, i64)>, AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let rows = sqlx::query(
        r#"
        SELECT a.number, a.name,
               COALESCE(SUM(jl.debit_minor), 0) - COALESCE(SUM(jl.credit_minor), 0) AS balance_minor
        FROM journal_lines jl
        JOIN vouchers v ON v.id = jl.voucher_id
        JOIN accounts a ON a.id = jl.account_id
        WHERE v.workspace_id = ?1
          AND v.fiscal_year_id = ?2
          AND v.status = 'posted'
        GROUP BY a.id, a.number, a.name
        HAVING balance_minor != 0
        ORDER BY a.number
        "#,
    )
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get("number"),
                row.get("name"),
                row.get::<i64, _>("balance_minor"),
            )
        })
        .collect())
}

async fn net_revenue_minor_for_fiscal_year_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    fiscal_year_id: &str,
) -> Result<i64, AppError> {
    let revenue_account: String = sqlx::query_scalar(
        r#"
        SELECT id FROM accounts WHERE workspace_id = ?1 AND number = '3041' LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(&mut **tx)
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
    .fetch_one(&mut **tx)
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
    .fetch_one(&mut **tx)
    .await?;

    Ok(credits - debits)
}

async fn build_ledger_snapshot_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    fiscal_year_id: &str,
) -> Result<LedgerSnapshot, AppError> {
    let revenue_minor =
        net_revenue_minor_for_fiscal_year_tx(tx, workspace_id, fiscal_year_id).await?;
    let expense_minor =
        expense_total_minor_for_fiscal_year(&mut **tx, workspace_id, fiscal_year_id).await?;
    let business_result_minor = revenue_minor - expense_minor;
    let account_balances =
        account_balances_for_fiscal_year(&mut **tx, workspace_id, fiscal_year_id).await?;

    Ok(LedgerSnapshot {
        revenue_minor,
        expense_minor,
        business_result_minor,
        account_balances,
    })
}

fn map_ne_fields(snapshot: &LedgerSnapshot) -> Vec<NeFieldDraft> {
    vec![
        NeFieldDraft {
            field_code: "R1".to_string(),
            amount_minor: snapshot.revenue_minor,
            source_ref: "3041".to_string(),
        },
        NeFieldDraft {
            field_code: "B14".to_string(),
            amount_minor: snapshot.business_result_minor,
            source_ref: "ledger:result".to_string(),
        },
    ]
}

async fn check_create_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentYearEndPayload>, AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_YEAR_END_CREATE)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(json) = payload else {
        return Ok(None);
    };
    let parsed: IdempotentYearEndPayload =
        serde_json::from_str(&json).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Some(parsed))
}

fn validate_create_idempotency_match(
    fiscal_year: i32,
    cached: &IdempotentYearEndPayload,
) -> Result<(), AppError> {
    if cached.fiscal_year != fiscal_year {
        return Err(AppError::validation(
            "Idempotency key was already used for a different fiscal year",
            "idempotencyKey",
        ));
    }
    Ok(())
}

async fn check_export_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentYearEndExportPayload>, AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_YEAR_END_EXPORT)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(json) = payload else {
        return Ok(None);
    };
    let parsed: IdempotentYearEndExportPayload =
        serde_json::from_str(&json).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Some(parsed))
}

fn validate_export_idempotency_match(
    package_id: &str,
    cached: &IdempotentYearEndExportPayload,
) -> Result<(), AppError> {
    if cached.package_id != package_id {
        return Err(AppError::validation(
            "Idempotency key was already used for a different year-end package",
            "idempotencyKey",
        ));
    }
    Ok(())
}

async fn rule_source_url_for_package(
    pool: &SqlitePool,
    rule_version_id: &str,
) -> Result<String, AppError> {
    if let Some(version) = get_rule_version_by_id(pool, rule_version_id).await? {
        return Ok(version.source_url);
    }
    let active = get_active_rule_version(pool)
        .await?
        .ok_or_else(|| AppError::validation("No active rule version", "ruleVersion"))?;
    Ok(active.source_url)
}

async fn persist_ne_fields_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    package_id: &str,
    fields: &[NeFieldDraft],
) -> Result<(), AppError> {
    for field in fields {
        sqlx::query(
            r#"
            INSERT INTO ne_fields (
              id, year_end_package_id, field_code, amount_minor, source_type, source_ref
            ) VALUES (?1, ?2, ?3, ?4, 'ledger', ?5)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(package_id)
        .bind(&field.field_code)
        .bind(field.amount_minor)
        .bind(&field.source_ref)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn load_ne_fields(
    pool: &SqlitePool,
    package_id: &str,
) -> Result<Vec<NeFieldSummary>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT field_code, amount_minor, source_type, source_ref
        FROM ne_fields
        WHERE year_end_package_id = ?1
        ORDER BY field_code
        "#,
    )
    .bind(package_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| NeFieldSummary {
            field_code: row.get("field_code"),
            amount_minor: row.get("amount_minor"),
            source_type: row.get("source_type"),
            source_ref: row.get("source_ref"),
        })
        .collect())
}

async fn fiscal_year_is_closed(pool: &SqlitePool, fiscal_year_id: &str) -> Result<bool, AppError> {
    let status: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status FROM fiscal_years WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(fiscal_year_id)
    .fetch_optional(pool)
    .await?;

    Ok(matches!(status.as_deref(), Some("closed")))
}

async fn package_id_for_fiscal_year(
    pool: &SqlitePool,
    workspace_id: &str,
    fiscal_year_id: &str,
) -> Result<Option<String>, AppError> {
    sqlx::query_scalar(
        r#"
        SELECT id FROM year_end_packages
        WHERE workspace_id = ?1 AND fiscal_year_id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(fiscal_year_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

async fn load_package_summary(
    pool: &SqlitePool,
    workspace_id: &str,
    package_id: &str,
) -> Result<YearEndPackageSummary, AppError> {
    let row = sqlx::query(
        r#"
        SELECT yep.id, yep.fiscal_year_id, yep.status, yep.rule_version_id,
               yep.annual_accounts_path, yep.ne_draft_path, yep.export_path,
               fy.starts_on
        FROM year_end_packages yep
        JOIN fiscal_years fy ON fy.id = yep.fiscal_year_id
        WHERE yep.workspace_id = ?1 AND yep.id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(package_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Year-end package not found", "packageId"))?;

    let fiscal_year_id: String = row.get("fiscal_year_id");
    let starts_on: String = row.get("starts_on");
    let fiscal_year = starts_on
        .get(0..4)
        .and_then(|y| y.parse().ok())
        .unwrap_or(0);

    let k1_allowed = k1_regime_allowed(pool).await?;
    let ne_fields = load_ne_fields(pool, package_id).await?;
    let annual_accounts_path: Option<String> = row.get("annual_accounts_path");
    let ne_draft_path: Option<String> = row.get("ne_draft_path");
    let (documents_path, _exports_path, database_path) =
        load_workspace_paths(pool, workspace_id).await?;
    let documents_dir =
        resolve_workspace_subdir(&documents_path, &database_path, "documentsPath")?;
    let stored_locally = artifact_exists(&documents_dir, &annual_accounts_path)
        && artifact_exists(&documents_dir, &ne_draft_path);
    let fiscal_year_locked = fiscal_year_is_closed(pool, &fiscal_year_id).await?;

    Ok(YearEndPackageSummary {
        id: row.get("id"),
        fiscal_year_id,
        fiscal_year,
        status: row.get("status"),
        rule_version_id: row.get("rule_version_id"),
        k1_allowed,
        ne_draft_present: !ne_fields.is_empty(),
        stored_locally,
        export_path: row.get("export_path"),
        fiscal_year_locked,
        ne_fields,
    })
}

fn artifact_exists(base_dir: &Path, rel_path: &Option<String>) -> bool {
    rel_path
        .as_ref()
        .is_some_and(|path| base_dir.join(path).is_file())
}

fn package_artifacts_complete(
    documents_dir: &Path,
    exports_dir: &Path,
    annual_accounts_path: &Option<String>,
    ne_draft_path: &Option<String>,
    export_path: &Option<String>,
) -> bool {
    artifact_exists(documents_dir, annual_accounts_path)
        && artifact_exists(documents_dir, ne_draft_path)
        && artifact_exists(exports_dir, export_path)
}

fn ne_drafts_from_summaries(fields: &[NeFieldSummary]) -> Vec<NeFieldDraft> {
    fields
        .iter()
        .map(|field| NeFieldDraft {
            field_code: field.field_code.clone(),
            amount_minor: field.amount_minor,
            source_ref: field.source_ref.clone().unwrap_or_default(),
        })
        .collect()
}

fn read_export_ne_fields(exports_dir: &Path, rel_path: &str) -> Result<Vec<NeFieldDraft>, AppError> {
    let content = std::fs::read_to_string(exports_dir.join(rel_path)).map_err(AppError::from)?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| AppError::internal(e.to_string()))?;
    let fields = json
        .get("neFields")
        .and_then(|value| value.as_array())
        .ok_or_else(|| AppError::internal("Export file is missing neFields".to_string()))?;

    fields
        .iter()
        .map(|field| {
            Ok(NeFieldDraft {
                field_code: field
                    .get("fieldCode")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AppError::internal("Export NE field missing fieldCode".to_string()))?
                    .to_string(),
                amount_minor: field
                    .get("amountMinor")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| AppError::internal("Export NE field missing amountMinor".to_string()))?,
                source_ref: field
                    .get("sourceRef")
                    .and_then(|v| v.as_str())
                    .unwrap_or("export")
                    .to_string(),
            })
        })
        .collect()
}

async fn ensure_package_artifacts(
    pool: &SqlitePool,
    workspace_id: &str,
    package_id: &str,
) -> Result<YearEndPackageSummary, AppError> {
    let summary = load_package_summary(pool, workspace_id, package_id).await?;
    let (documents_path, exports_path, database_path) =
        load_workspace_paths(pool, workspace_id).await?;
    let documents_dir = resolve_workspace_subdir(&documents_path, &database_path, "documentsPath")?;
    let exports_dir = resolve_workspace_subdir(&exports_path, &database_path, "exportsPath")?;

    let row = sqlx::query(
        r#"
        SELECT annual_accounts_path, ne_draft_path, export_path
        FROM year_end_packages
        WHERE id = ?1
        LIMIT 1
        "#,
    )
    .bind(package_id)
    .fetch_one(pool)
    .await?;

    let annual_accounts_path: Option<String> = row.get("annual_accounts_path");
    let ne_draft_path: Option<String> = row.get("ne_draft_path");
    let export_path: Option<String> = row.get("export_path");

    if package_artifacts_complete(
        &documents_dir,
        &exports_dir,
        &annual_accounts_path,
        &ne_draft_path,
        &export_path,
    ) {
        return Ok(summary);
    }

    let source_url = rule_source_url_for_package(pool, &summary.rule_version_id).await?;
    let export_on_disk = artifact_exists(&exports_dir, &export_path);

    let mut tx = pool.begin().await?;
    let mut snapshot =
        build_ledger_snapshot_tx(&mut tx, workspace_id, &summary.fiscal_year_id).await?;
    let ne_field_drafts = if export_on_disk {
        let rel = export_path.as_ref().expect("export path checked above");
        let drafts = read_export_ne_fields(&exports_dir, rel)?;
        if let Some(b14) = drafts.iter().find(|field| field.field_code == "B14") {
            snapshot.business_result_minor = b14.amount_minor;
        }
        drafts
    } else if summary.status == "approved" {
        ne_drafts_from_summaries(&summary.ne_fields)
    } else {
        map_ne_fields(&snapshot)
    };
    tx.rollback().await?;

    let annual_ok = artifact_exists(&documents_dir, &annual_accounts_path);
    let ne_ok = artifact_exists(&documents_dir, &ne_draft_path);

    let (annual_rel, ne_rel) = if annual_ok && ne_ok {
        (
            annual_accounts_path.clone().expect("annual path checked above"),
            ne_draft_path.clone().expect("ne path checked above"),
        )
    } else {
        write_local_drafts(
            &documents_dir,
            summary.fiscal_year,
            &summary.rule_version_id,
            &snapshot,
            &ne_field_drafts,
        )
        .await?
    };

    let export_rel = if export_on_disk {
        export_path.clone().expect("export path checked above")
    } else {
        let ne_summaries: Vec<NeFieldSummary> = ne_field_drafts
            .iter()
            .map(|field| NeFieldSummary {
                field_code: field.field_code.clone(),
                amount_minor: field.amount_minor,
                source_type: "ledger".to_string(),
                source_ref: Some(field.source_ref.clone()),
            })
            .collect();
        let export_summary = YearEndPackageSummary {
            ne_fields: ne_summaries,
            ..summary.clone()
        };
        write_export_file(&exports_dir, &export_summary, &source_url).await?
    };

    sqlx::query(
        r#"
        UPDATE year_end_packages
        SET annual_accounts_path = ?1, ne_draft_path = ?2, export_path = ?3
        WHERE id = ?4
        "#,
    )
    .bind(&annual_rel)
    .bind(&ne_rel)
    .bind(&export_rel)
    .bind(package_id)
    .execute(pool)
    .await?;

    load_package_summary(pool, workspace_id, package_id).await
}

async fn write_local_drafts(
    documents_dir: &Path,
    fiscal_year: i32,
    rule_version_id: &str,
    snapshot: &LedgerSnapshot,
    ne_fields: &[NeFieldDraft],
) -> Result<(String, String), AppError> {
    let year_dir = documents_dir.join("year-end").join(fiscal_year.to_string());
    std::fs::create_dir_all(&year_dir).map_err(AppError::from)?;

    let annual_filename = format!("k1-annual-accounts-{fiscal_year}.json");
    let ne_filename = format!("ne-draft-{fiscal_year}.json");
    let annual_path = year_dir.join(&annual_filename);
    let ne_path = year_dir.join(&ne_filename);

    let annual_json = serde_json::json!({
        "fiscalYear": fiscal_year,
        "ruleVersionId": rule_version_id,
        "accountingRegime": K1_REGIME,
        "revenueMinor": snapshot.revenue_minor,
        "expenseMinor": snapshot.expense_minor,
        "accounts": snapshot.account_balances.iter().map(|(number, name, balance)| {
            serde_json::json!({
                "number": number,
                "name": name,
                "balanceMinor": balance,
            })
        }).collect::<Vec<_>>(),
        "businessResultMinor": snapshot.business_result_minor,
    });
    std::fs::write(
        &annual_path,
        serde_json::to_string_pretty(&annual_json).map_err(|e| AppError::internal(e.to_string()))?,
    )?;

    let ne_json = serde_json::json!({
        "fiscalYear": fiscal_year,
        "ruleVersionId": rule_version_id,
        "fields": ne_fields.iter().map(|f| {
            serde_json::json!({
                "fieldCode": f.field_code,
                "amountMinor": f.amount_minor,
                "sourceRef": f.source_ref,
            })
        }).collect::<Vec<_>>(),
    });
    std::fs::write(
        &ne_path,
        serde_json::to_string_pretty(&ne_json).map_err(|e| AppError::internal(e.to_string()))?,
    )?;

    Ok((
        format!("year-end/{fiscal_year}/{annual_filename}"),
        format!("year-end/{fiscal_year}/{ne_filename}"),
    ))
}

fn build_export_json(summary: &YearEndPackageSummary, source_url: &str) -> serde_json::Value {
    serde_json::json!({
        "fiscalYear": summary.fiscal_year,
        "ruleVersionId": summary.rule_version_id,
        "accountingRegime": K1_REGIME,
        "status": summary.status,
        "neFields": summary.ne_fields,
        "sourceUrl": source_url,
    })
}

async fn write_export_file(
    exports_dir: &Path,
    summary: &YearEndPackageSummary,
    source_url: &str,
) -> Result<String, AppError> {
    let export_dir = exports_dir.join("year-end");
    std::fs::create_dir_all(&export_dir).map_err(AppError::from)?;
    let filename = format!("year-end-package-{}-{}.json", summary.fiscal_year, summary.id);
    let export_path = export_dir.join(&filename);
    std::fs::write(
        &export_path,
        serde_json::to_string_pretty(&build_export_json(summary, source_url))
            .map_err(|e| AppError::internal(e.to_string()))?,
    )?;
    Ok(format!("year-end/{filename}"))
}

async fn vat_year_filing_items_pool(
    pool: &SqlitePool,
    workspace_id: &str,
    fiscal_year: i32,
) -> Result<Vec<YearEndReadinessItem>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT vat_status, reporting_period
        FROM vat_profiles
        WHERE workspace_id = ?1
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(vec![YearEndReadinessItem {
            code: "vat_periods_filed".to_string(),
            satisfied: true,
            detail: Some("No VAT profile configured".to_string()),
        }]);
    };

    let vat_status: String = row.get("vat_status");
    if vat_status != "registered" && vat_status != "voluntary_registered" {
        return Ok(vec![YearEndReadinessItem {
            code: "vat_periods_filed".to_string(),
            satisfied: true,
            detail: Some("VAT registration not required".to_string()),
        }]);
    }

    let reporting_period: String = row.get("reporting_period");
    let mut items = Vec::new();
    for period_key in crate::vat::period_keys_for_year(&reporting_period, fiscal_year) {
        let status: Option<String> = sqlx::query_scalar(
            r#"
            SELECT vr.status
            FROM vat_returns vr
            JOIN fiscal_periods fp ON fp.id = vr.fiscal_period_id
            WHERE vr.workspace_id = ?1 AND fp.period_key = ?2
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(&period_key)
        .fetch_optional(pool)
        .await?;

        let (satisfied, detail) = match status.as_deref() {
            Some("approved") => (true, None),
            Some("draft") => (
                false,
                Some(format!(
                    "Draft VAT return exists for {period_key}; approve or remove it first"
                )),
            ),
            _ => (
                false,
                Some(format!("Approved VAT return required for {period_key}")),
            ),
        };

        items.push(YearEndReadinessItem {
            code: format!("vat_period_{period_key}"),
            satisfied,
            detail,
        });
    }

    Ok(items)
}

async fn vat_year_filing_items_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    fiscal_year: i32,
) -> Result<Vec<YearEndReadinessItem>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT vat_status, reporting_period
        FROM vat_profiles
        WHERE workspace_id = ?1
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else {
        return Ok(vec![YearEndReadinessItem {
            code: "vat_periods_filed".to_string(),
            satisfied: true,
            detail: Some("No VAT profile configured".to_string()),
        }]);
    };

    let vat_status: String = row.get("vat_status");
    if vat_status != "registered" && vat_status != "voluntary_registered" {
        return Ok(vec![YearEndReadinessItem {
            code: "vat_periods_filed".to_string(),
            satisfied: true,
            detail: Some("VAT registration not required".to_string()),
        }]);
    }

    let reporting_period: String = row.get("reporting_period");
    let mut items = Vec::new();
    for period_key in crate::vat::period_keys_for_year(&reporting_period, fiscal_year) {
        let status: Option<String> = sqlx::query_scalar(
            r#"
            SELECT vr.status
            FROM vat_returns vr
            JOIN fiscal_periods fp ON fp.id = vr.fiscal_period_id
            WHERE vr.workspace_id = ?1 AND fp.period_key = ?2
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(&period_key)
        .fetch_optional(&mut **tx)
        .await?;

        let (satisfied, detail) = match status.as_deref() {
            Some("approved") => (true, None),
            Some("draft") => (
                false,
                Some(format!(
                    "Draft VAT return exists for {period_key}; approve or remove it first"
                )),
            ),
            _ => (
                false,
                Some(format!("Approved VAT return required for {period_key}")),
            ),
        };

        items.push(YearEndReadinessItem {
            code: format!("vat_period_{period_key}"),
            satisfied,
            detail,
        });
    }

    Ok(items)
}

async fn ensure_vat_year_filed_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    fiscal_year: i32,
) -> Result<(), AppError> {
    for item in vat_year_filing_items_tx(tx, workspace_id, fiscal_year).await? {
        if !item.satisfied {
            return Err(AppError::validation(
                item.detail
                    .as_deref()
                    .unwrap_or("VAT returns must be filed before year-end close"),
                "fiscalYear",
            ));
        }
    }

    Ok(())
}

async fn lock_fiscal_year_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    fiscal_year_id: &str,
) -> Result<(), AppError> {
    let updated = sqlx::query(
        r#"
        UPDATE fiscal_years
        SET status = 'closed', updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1 AND status = 'open'
        "#,
    )
    .bind(fiscal_year_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::locked_period("Fiscal year is already closed"));
    }

    sqlx::query(
        r#"
        UPDATE fiscal_periods
        SET status = 'locked'
        WHERE fiscal_year_id = ?1 AND status = 'open'
        "#,
    )
    .bind(fiscal_year_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn year_end_package_create(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &YearEndPackageCreateInput,
) -> Result<YearEndPackageSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;

    if let Some(cached) = check_create_idempotency(pool, workspace_id, idempotency_key).await? {
        validate_create_idempotency_match(input.fiscal_year, &cached)?;
        return ensure_package_artifacts(pool, workspace_id, &cached.package_id).await;
    }

    let k1_allowed = k1_regime_allowed(pool).await?;
    if !k1_allowed {
        return Err(AppError::validation(
            "Active rule version does not allow K1 simplified annual accounts",
            "fiscalYear",
        ));
    }

    if !ne_draft_required(pool).await? {
        return Err(AppError::validation("NE draft is not required for active rules", "fiscalYear"));
    }

    let rule_version = get_active_rule_version(pool)
        .await?
        .ok_or_else(|| AppError::validation("No active rule version", "ruleVersion"))?;

    let fiscal_year_id = fiscal_year_id_for_year(pool, workspace_id, input.fiscal_year).await?;

    if let Some(existing_id) =
        package_id_for_fiscal_year(pool, workspace_id, &fiscal_year_id).await?
    {
        return ensure_package_artifacts(pool, workspace_id, &existing_id).await;
    }

    let package_id = Uuid::new_v4().to_string();
    let payload_json = serde_json::to_string(&IdempotentYearEndPayload {
        package_id: package_id.clone(),
        fiscal_year: input.fiscal_year,
    })
    .map_err(|e| AppError::internal(e.to_string()))?;

    let (documents_path, exports_path, database_path) = load_workspace_paths(pool, workspace_id).await?;
    let documents_dir = resolve_workspace_subdir(&documents_path, &database_path, "documentsPath")?;
    let exports_dir = resolve_workspace_subdir(&exports_path, &database_path, "exportsPath")?;
    let source_url = rule_source_url_for_package(pool, &rule_version.id).await?;

    let mut tx = pool.begin().await?;
    ensure_fiscal_year_open_tx(&mut *tx, &fiscal_year_id).await?;

    let snapshot = build_ledger_snapshot_tx(&mut tx, workspace_id, &fiscal_year_id).await?;
    let ne_field_drafts = map_ne_fields(&snapshot);

    match sqlx::query(
        r#"
        INSERT INTO year_end_packages (
          id, workspace_id, fiscal_year_id, status, rule_version_id
        ) VALUES (?1, ?2, ?3, 'draft', ?4)
        "#,
    )
    .bind(&package_id)
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .bind(&rule_version.id)
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            tx.rollback().await?;
            if let Some(existing_id) =
                package_id_for_fiscal_year(pool, workspace_id, &fiscal_year_id).await?
            {
                return ensure_package_artifacts(pool, workspace_id, &existing_id).await;
            }
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    }

    persist_ne_fields_tx(&mut tx, &package_id, &ne_field_drafts).await?;

    ensure_vat_year_filed_tx(&mut tx, workspace_id, input.fiscal_year).await?;

    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_YEAR_END_CREATE)
    .bind(&payload_json)
    .bind(idempotency_key)
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            tx.rollback().await?;
            if let Some(cached) = check_create_idempotency(pool, workspace_id, idempotency_key).await? {
                validate_create_idempotency_match(input.fiscal_year, &cached)?;
                return ensure_package_artifacts(pool, workspace_id, &cached.package_id).await;
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
        "year_end_package_created",
        "year_end_package",
        Some(&package_id),
        &serde_json::json!({ "fiscalYear": input.fiscal_year }).to_string(),
    )
    .await?;

    tx.commit().await?;

    let ne_summaries: Vec<NeFieldSummary> = ne_field_drafts
        .iter()
        .map(|field| NeFieldSummary {
            field_code: field.field_code.clone(),
            amount_minor: field.amount_minor,
            source_type: "ledger".to_string(),
            source_ref: Some(field.source_ref.clone()),
        })
        .collect();

    let export_summary = YearEndPackageSummary {
        id: package_id.clone(),
        fiscal_year_id: fiscal_year_id.clone(),
        fiscal_year: input.fiscal_year,
        status: "draft".to_string(),
        rule_version_id: rule_version.id.clone(),
        k1_allowed: true,
        ne_draft_present: !ne_summaries.is_empty(),
        stored_locally: true,
        export_path: None,
        fiscal_year_locked: false,
        ne_fields: ne_summaries,
    };

    let (annual_rel, ne_rel) = write_local_drafts(
        &documents_dir,
        input.fiscal_year,
        &rule_version.id,
        &snapshot,
        &ne_field_drafts,
    )
    .await?;

    let export_rel = write_export_file(&exports_dir, &export_summary, &source_url).await?;

    sqlx::query(
        r#"
        UPDATE year_end_packages
        SET annual_accounts_path = ?1, ne_draft_path = ?2, export_path = ?3
        WHERE id = ?4
        "#,
    )
    .bind(&annual_rel)
    .bind(&ne_rel)
    .bind(&export_rel)
    .bind(&package_id)
    .execute(pool)
    .await?;

    record_event(
        pool,
        workspace_id,
        "year_end_package_export",
        "year_end_package",
        Some(&package_id),
        &serde_json::json!({ "exportPath": export_rel }).to_string(),
    )
    .await?;

    load_package_summary(pool, workspace_id, &package_id).await
}

pub async fn year_end_package_get(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &YearEndPackageGetInput,
) -> Result<YearEndPackageSummary, AppError> {
    ensure_package_artifacts(pool, workspace_id, &input.package_id).await
}

pub async fn year_end_package_find_by_fiscal_year(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &YearEndPackageFindInput,
) -> Result<Option<YearEndPackageSummary>, AppError> {
    let fiscal_year_id =
        fiscal_year_id_for_year(pool, workspace_id, input.fiscal_year).await?;
    let Some(package_id) =
        package_id_for_fiscal_year(pool, workspace_id, &fiscal_year_id).await?
    else {
        return Ok(None);
    };
    Ok(Some(
        ensure_package_artifacts(pool, workspace_id, &package_id).await?,
    ))
}

pub async fn year_end_readiness_get(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &YearEndReadinessInput,
) -> Result<YearEndReadiness, AppError> {
    let fiscal_year_id =
        fiscal_year_id_for_year(pool, workspace_id, input.fiscal_year).await?;

    let fiscal_year_status: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status FROM fiscal_years
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .fetch_optional(pool)
    .await?;

    let mut items = vec![YearEndReadinessItem {
        code: "fiscal_year_open".to_string(),
        satisfied: fiscal_year_status.as_deref() == Some("open"),
        detail: fiscal_year_status
            .as_ref()
            .map(|status| format!("Fiscal year status is {status}")),
    }];

    items.extend(
        vat_year_filing_items_pool(pool, workspace_id, input.fiscal_year).await?,
    );

    let k1_allowed = k1_regime_allowed(pool).await?;
    items.push(YearEndReadinessItem {
        code: "k1_allowed".to_string(),
        satisfied: k1_allowed,
        detail: if k1_allowed {
            None
        } else {
            Some("K1 simplified annual accounts are not allowed for this workspace".to_string())
        },
    });

    if let Some(package_id) =
        package_id_for_fiscal_year(pool, workspace_id, &fiscal_year_id).await?
    {
        let package = load_package_summary(pool, workspace_id, &package_id).await?;
        items.push(YearEndReadinessItem {
            code: "year_end_package_present".to_string(),
            satisfied: true,
            detail: None,
        });
        items.push(YearEndReadinessItem {
            code: "year_end_package_draft".to_string(),
            satisfied: package.status == "draft",
            detail: Some(format!("Package status is {}", package.status)),
        });
        items.push(YearEndReadinessItem {
            code: "ne_draft_present".to_string(),
            satisfied: package.ne_draft_present,
            detail: if package.ne_draft_present {
                None
            } else {
                Some("NE draft is missing from the year-end package".to_string())
            },
        });
    } else {
        items.push(YearEndReadinessItem {
            code: "year_end_package_present".to_string(),
            satisfied: false,
            detail: Some("Create a year-end package before approval".to_string()),
        });
    }

    let ready_to_approve = items.iter().all(|item| item.satisfied);

    Ok(YearEndReadiness {
        items,
        ready_to_approve,
    })
}

pub async fn year_end_package_approve(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &YearEndPackageApproveInput,
) -> Result<YearEndPackageSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;
    let summary = load_package_summary(pool, workspace_id, &input.package_id).await?;

    if summary.status == "approved" {
        return ensure_package_artifacts(pool, workspace_id, &input.package_id).await;
    }

    let mut tx = pool.begin().await?;
    ensure_fiscal_year_open_tx(&mut *tx, &summary.fiscal_year_id).await?;
    ensure_vat_year_filed_tx(&mut tx, workspace_id, summary.fiscal_year).await?;

    let updated = sqlx::query(
        r#"
        UPDATE year_end_packages
        SET status = 'approved', approved_at = CURRENT_TIMESTAMP
        WHERE id = ?1 AND workspace_id = ?2 AND status = 'draft'
        "#,
    )
    .bind(&input.package_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::validation("Year-end package is not in draft status", "packageId"));
    }

    lock_fiscal_year_tx(&mut tx, &summary.fiscal_year_id).await?;

    sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_YEAR_END_APPROVE)
    .bind(
        serde_json::to_string(&IdempotentYearEndPayload {
            package_id: input.package_id.clone(),
            fiscal_year: summary.fiscal_year,
        })
        .map_err(|e| AppError::internal(e.to_string()))?,
    )
    .bind(idempotency_key)
    .execute(&mut *tx)
    .await?;

    record_event_tx(
        &mut *tx,
        workspace_id,
        "year_end_package_approved",
        "year_end_package",
        Some(&input.package_id),
        "{}",
    )
    .await?;

    tx.commit().await?;
    ensure_package_artifacts(pool, workspace_id, &input.package_id).await
}

async fn publish_year_end_package_export(
    pool: &SqlitePool,
    workspace_id: &str,
    mut package: YearEndPackageSummary,
    export_directory: Option<&str>,
) -> Result<YearEndPackageSummary, AppError> {
    let Some(rel_path) = package.export_path.clone() else {
        return Ok(package);
    };
    let filename = std::path::Path::new(&rel_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("year-end-package.json");
    let (_, exports_path, database_path) = load_workspace_paths(pool, workspace_id).await?;
    let published = crate::paths::publish_export_artifact(
        pool,
        workspace_id,
        &exports_path,
        &database_path,
        &rel_path,
        export_directory,
        filename,
    )
    .await?;
    package.export_path = Some(published);
    Ok(package)
}

pub async fn year_end_package_export(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &YearEndPackageExportInput,
) -> Result<YearEndPackageSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;

    if let Some(cached) = check_export_idempotency(pool, workspace_id, idempotency_key).await? {
        validate_export_idempotency_match(&input.package_id, &cached)?;
        return publish_year_end_package_export(
            pool,
            workspace_id,
            ensure_package_artifacts(pool, workspace_id, &cached.package_id).await?,
            input.export_directory.as_deref(),
        )
        .await;
    }

    let summary = load_package_summary(pool, workspace_id, &input.package_id).await?;
    if summary.status != "approved" {
        return Err(AppError::validation(
            "Only approved year-end packages can be exported",
            "packageId",
        ));
    }

    let source_url = rule_source_url_for_package(pool, &summary.rule_version_id).await?;

    let (_, exports_path, database_path) = load_workspace_paths(pool, workspace_id).await?;
    let exports_dir = resolve_workspace_subdir(&exports_path, &database_path, "exportsPath")?;
    let export_rel = write_export_file(&exports_dir, &summary, &source_url).await?;

    sqlx::query(
        r#"
        UPDATE year_end_packages SET export_path = ?1 WHERE id = ?2
        "#,
    )
    .bind(&export_rel)
    .bind(&input.package_id)
    .execute(pool)
    .await?;

    let export_payload = serde_json::to_string(&IdempotentYearEndExportPayload {
        package_id: input.package_id.clone(),
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
    .bind(JOB_YEAR_END_EXPORT)
    .bind(&export_payload)
    .bind(idempotency_key)
    .execute(pool)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            if let Some(cached) = check_export_idempotency(pool, workspace_id, idempotency_key).await? {
                validate_export_idempotency_match(&input.package_id, &cached)?;
                return publish_year_end_package_export(
                    pool,
                    workspace_id,
                    ensure_package_artifacts(pool, workspace_id, &cached.package_id).await?,
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
        "year_end_package_export",
        "year_end_package",
        Some(&input.package_id),
        &serde_json::json!({ "exportPath": export_rel }).to_string(),
    )
    .await?;

    let package = ensure_package_artifacts(pool, workspace_id, &input.package_id).await?;
    publish_year_end_package_export(
        pool,
        workspace_id,
        package,
        input.export_directory.as_deref(),
    )
    .await
}
