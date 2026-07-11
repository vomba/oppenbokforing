use chrono::{Datelike, NaiveDate, Utc};
use sqlx::SqlitePool;
use std::path::{Component, Path, PathBuf};

use crate::error::AppError;

pub fn resolve_workspace_exports_dir(
    exports_path: &str,
    database_path: &str,
) -> Result<PathBuf, AppError> {
    let exports = PathBuf::from(exports_path);
    if exports.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(AppError::validation("Invalid exports path", "exportsPath"));
    }

    let data_root = Path::new(database_path)
        .parent()
        .ok_or_else(|| AppError::validation("Invalid database path", "databasePath"))?;

    let canonical_root = data_root.canonicalize().map_err(AppError::from)?;

    if exports.exists() {
        let canonical_exports = exports.canonicalize().map_err(AppError::from)?;
        if !canonical_exports.starts_with(&canonical_root) {
            return Err(AppError::validation(
                "Exports path must be inside workspace data directory",
                "exportsPath",
            ));
        }
        return Ok(canonical_exports);
    }

    let mut probe = exports.as_path();
    while !probe.exists() {
        probe = probe
            .parent()
            .ok_or_else(|| AppError::validation("Invalid exports path", "exportsPath"))?;
    }
    let canonical_probe = probe.canonicalize().map_err(AppError::from)?;
    if !canonical_probe.starts_with(&canonical_root) {
        return Err(AppError::validation(
            "Exports path must be inside workspace data directory",
            "exportsPath",
        ));
    }

    std::fs::create_dir_all(&exports).map_err(AppError::from)?;
    let canonical_exports = exports.canonicalize().map_err(AppError::from)?;
    if !canonical_exports.starts_with(&canonical_root) {
        return Err(AppError::validation(
            "Exports path must be inside workspace data directory",
            "exportsPath",
        ));
    }

    Ok(canonical_exports)
}

pub fn reject_path_traversal(relative: &str, field: &str) -> Result<(), AppError> {
    if relative.contains("..") {
        return Err(AppError::validation(
            "Path must not contain parent directory segments",
            field,
        ));
    }
    Ok(())
}

pub fn safe_join_under(root: &Path, relative: &str, field: &str) -> Result<PathBuf, AppError> {
    reject_path_traversal(relative, field)?;
    Ok(root.join(relative))
}

pub fn ensure_path_within_root(resolved: &Path, root: &Path, field: &str) -> Result<(), AppError> {
    let canonical_root = root.canonicalize().map_err(AppError::from)?;
    let canonical_resolved = resolved.canonicalize().map_err(AppError::from)?;
    if !canonical_resolved.starts_with(&canonical_root) {
        return Err(AppError::validation(
            "Path must stay inside the allowed directory",
            field,
        ));
    }
    Ok(())
}

const BAS_ACCOUNTS: &[(&str, &str, &str, &str)] = &[
    ("1510", "Kundfordringar", "asset", "debit"),
    ("1930", "Företagskonto / bank", "asset", "debit"),
    ("2641", "Ingående moms", "asset", "debit"),
    ("2611", "Utgående moms 25%", "liability", "credit"),
    ("3041", "Försäljning tjänster", "revenue", "credit"),
    ("5610", "Personbilskostnader", "expense", "debit"),
];

pub fn year_from_date(date: &str) -> Result<i32, AppError> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid date format", "issueDate"))?;
    Ok(parsed.year())
}

pub async fn bootstrap_accounting_defaults(
    pool: &SqlitePool,
    workspace_id: &str,
    rule_year: i32,
) -> Result<(), AppError> {
    let fiscal_year_id = format!("fy-{workspace_id}-{rule_year}");
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO fiscal_years (id, workspace_id, starts_on, ends_on, status)
        VALUES (?1, ?2, ?3, ?4, 'open')
        "#,
    )
    .bind(&fiscal_year_id)
    .bind(workspace_id)
    .bind(format!("{rule_year}-01-01"))
    .bind(format!("{rule_year}-12-31"))
    .execute(pool)
    .await?;

    for (number, name, account_type, normal_balance) in BAS_ACCOUNTS {
        let account_id = format!("acc-{workspace_id}-{number}");
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO accounts (
              id, workspace_id, number, name, account_type, normal_balance
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(account_id)
        .bind(workspace_id)
        .bind(number)
        .bind(name)
        .bind(account_type)
        .bind(normal_balance)
        .execute(pool)
        .await?;
    }

    let sequence_id = format!("seq-{workspace_id}-{rule_year}");
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO invoice_sequences (id, workspace_id, fiscal_year_id, prefix, next_number)
        VALUES (?1, ?2, ?3, ?4, 1)
        "#,
    )
    .bind(sequence_id)
    .bind(workspace_id)
    .bind(&fiscal_year_id)
    .bind(format!("{rule_year}-"))
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn fiscal_year_id_for_year(
    pool: &SqlitePool,
    workspace_id: &str,
    rule_year: i32,
) -> Result<String, AppError> {
    let fiscal_year_id = format!("fy-{workspace_id}-{rule_year}");
    let exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1 FROM fiscal_years WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(&fiscal_year_id)
    .fetch_optional(pool)
    .await?;

    if exists.is_none() {
        bootstrap_accounting_defaults(pool, workspace_id, rule_year).await?;
    }

    Ok(fiscal_year_id)
}

pub async fn current_fiscal_year_id(
    pool: &SqlitePool,
    workspace_id: &str,
    rule_year: i32,
) -> Result<String, AppError> {
    fiscal_year_id_for_year(pool, workspace_id, rule_year).await
}

pub async fn fiscal_year_id_for_date(
    pool: &SqlitePool,
    workspace_id: &str,
    date: &str,
) -> Result<String, AppError> {
    fiscal_year_id_for_year(pool, workspace_id, year_from_date(date)?).await
}

pub async fn ensure_fiscal_year_open(
    pool: &SqlitePool,
    fiscal_year_id: &str,
) -> Result<(), AppError> {
    ensure_fiscal_year_open_tx(pool, fiscal_year_id).await
}

pub async fn ensure_fiscal_year_open_tx<'e, E>(
    executor: E,
    fiscal_year_id: &str,
) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let status: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status FROM fiscal_years WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(fiscal_year_id)
    .fetch_optional(executor)
    .await?;

    match status.as_deref() {
        Some("open") => Ok(()),
        Some(_) => Err(AppError::locked_period("Fiscal year is locked")),
        None => Err(AppError::validation("Fiscal year not found", "fiscalYear")),
    }
}

pub async fn ensure_workspace_ready(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<(), AppError> {
    let year = year_from_date(&Utc::now().format("%Y-%m-%d").to_string())?;
    bootstrap_accounting_defaults(pool, workspace_id, year).await?;
    crate::vat::seed_vat_codes(pool, workspace_id).await?;
    crate::settings::ensure_default_settings(pool, workspace_id).await
}

#[cfg(test)]
mod tests {
    use super::resolve_workspace_exports_dir;

    #[test]
    fn resolve_exports_dir_io_error_is_redacted() {
        let error = resolve_workspace_exports_dir(
            "/tmp/exports",
            "/this/path/definitely/does/not/exist/workspace.sqlite",
        )
        .expect_err("missing workspace parent should fail");

        assert_eq!(error.code, "storage_error");
        assert!(!error.message.contains("does/not/exist"));
        assert!(!error.message.contains("workspace.sqlite"));
    }
}
