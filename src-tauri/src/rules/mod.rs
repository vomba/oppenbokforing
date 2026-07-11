use serde::Serialize;
use specta::Type;
use sqlx::{Row, SqlitePool};

use crate::error::AppError;

pub const ACTIVE_RULE_VERSION_ID: &str = "rv-2026-active";

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RuleVersionSummary {
    pub id: String,
    pub tax_year: i32,
    pub source_url: String,
    pub status: String,
}

pub async fn get_active_rule_version(pool: &SqlitePool) -> Result<Option<RuleVersionSummary>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, tax_year, source_url, status
        FROM rule_versions
        WHERE status = 'active'
        ORDER BY tax_year DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| RuleVersionSummary {
        id: row.get("id"),
        tax_year: row.get("tax_year"),
        source_url: row.get("source_url"),
        status: row.get("status"),
    }))
}

pub async fn require_rule_i64(pool: &SqlitePool, family: &str, key: &str) -> Result<i64, AppError> {
    get_rule_i64(pool, family, key)
        .await?
        .ok_or_else(|| {
            AppError::validation(
                "Active tax rule configuration is missing or invalid",
                "ruleVersion",
            )
        })
}

pub async fn get_rule_i64(pool: &SqlitePool, family: &str, key: &str) -> Result<Option<i64>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT tr.value_json AS value_json
        FROM tax_rules tr
        JOIN rule_versions rv ON rv.id = tr.rule_version_id
        WHERE rv.status = 'active' AND tr.family = ?1 AND tr.key = ?2
        LIMIT 1
        "#,
    )
    .bind(family)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|row| {
        let json: String = row.get("value_json");
        serde_json::from_str::<i64>(&json).ok()
    }))
}

pub async fn get_rule_version_by_id(
    pool: &SqlitePool,
    rule_version_id: &str,
) -> Result<Option<RuleVersionSummary>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, tax_year, source_url, status
        FROM rule_versions
        WHERE id = ?1
        LIMIT 1
        "#,
    )
    .bind(rule_version_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| RuleVersionSummary {
        id: row.get("id"),
        tax_year: row.get("tax_year"),
        source_url: row.get("source_url"),
        status: row.get("status"),
    }))
}

pub async fn get_rule_string(pool: &SqlitePool, family: &str, key: &str) -> Result<Option<String>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT tr.value_json AS value_json
        FROM tax_rules tr
        JOIN rule_versions rv ON rv.id = tr.rule_version_id
        WHERE rv.status = 'active' AND tr.family = ?1 AND tr.key = ?2
        LIMIT 1
        "#,
    )
    .bind(family)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|row| {
        let json: String = row.get("value_json");
        serde_json::from_str::<String>(&json).ok()
    }))
}

pub async fn get_rule_bool(pool: &SqlitePool, family: &str, key: &str) -> Result<Option<bool>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT tr.value_json AS value_json
        FROM tax_rules tr
        JOIN rule_versions rv ON rv.id = tr.rule_version_id
        WHERE rv.status = 'active' AND tr.family = ?1 AND tr.key = ?2
        LIMIT 1
        "#,
    )
    .bind(family)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|row| {
        let json: String = row.get("value_json");
        serde_json::from_str::<bool>(&json).ok()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_workspace;
    use tempfile::tempdir;

    #[tokio::test]
    async fn seed_migration_loads_vat_threshold() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("workspace.sqlite");
        let pool = connect_workspace(&db_path).await.unwrap();

        let threshold = get_rule_i64(&pool, "vat", "annual_turnover_threshold_minor")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(threshold, 12_000_000);
    }
}
