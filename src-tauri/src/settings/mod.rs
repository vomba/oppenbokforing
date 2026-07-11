use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};

use crate::{audit::record_event, error::AppError};

const VALID_LOCALES: &[&str] = &["en", "sv"];

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettings {
    pub id: String,
    pub locale: String,
    pub updater_enabled: bool,
    pub default_export_directory: Option<String>,
    pub default_backup_directory: Option<String>,
    pub dashboard_tour_completed: bool,
    pub simple_mode: bool,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettingsSaveInput {
    pub locale: String,
    pub updater_enabled: Option<bool>,
    pub default_export_directory: Option<String>,
    pub default_backup_directory: Option<String>,
    pub simple_mode: Option<bool>,
}

pub fn validate_locale(locale: &str) -> Result<(), AppError> {
    if VALID_LOCALES.contains(&locale) {
        Ok(())
    } else {
        Err(AppError::validation(
            "Locale must be 'en' or 'sv'",
            "locale",
        ))
    }
}

pub async fn ensure_default_settings(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<(), AppError> {
    let settings_id = format!("ws-settings-{workspace_id}");
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO workspace_settings (id, workspace_id, locale, updater_enabled)
        VALUES (?1, ?2, 'sv', 0)
        "#,
    )
    .bind(&settings_id)
    .bind(workspace_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn workspace_settings_get(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<WorkspaceSettings, AppError> {
    ensure_default_settings(pool, workspace_id).await?;
    load_settings(pool, workspace_id).await
}

pub async fn workspace_settings_save(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &WorkspaceSettingsSaveInput,
) -> Result<WorkspaceSettings, AppError> {
    validate_locale(&input.locale)?;
    ensure_default_settings(pool, workspace_id).await?;

    let current = load_settings(pool, workspace_id).await?;
    let updater_enabled = input
        .updater_enabled
        .unwrap_or(current.updater_enabled);
    let default_export_directory = match input.default_export_directory.as_deref() {
        Some(_) => normalize_optional_directory(input.default_export_directory.as_deref())?,
        None => current.default_export_directory,
    };
    let default_backup_directory = match input.default_backup_directory.as_deref() {
        Some(_) => normalize_optional_directory(input.default_backup_directory.as_deref())?,
        None => current.default_backup_directory,
    };
    let simple_mode = input.simple_mode.unwrap_or(current.simple_mode);
    let settings_id = format!("ws-settings-{workspace_id}");

    sqlx::query(
        r#"
        UPDATE workspace_settings
        SET locale = ?1,
            updater_enabled = ?2,
            default_export_directory = ?3,
            default_backup_directory = ?4,
            simple_mode = ?5,
            updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?6
        "#,
    )
    .bind(&input.locale)
    .bind(if updater_enabled { 1 } else { 0 })
    .bind(&default_export_directory)
    .bind(&default_backup_directory)
    .bind(if simple_mode { 1 } else { 0 })
    .bind(workspace_id)
    .execute(pool)
    .await?;

    record_event(
        pool,
        workspace_id,
        "workspace_settings_save",
        "workspace_settings",
        Some(&settings_id),
        &serde_json::json!({
            "locale": input.locale,
            "updaterEnabled": updater_enabled,
            "defaultExportDirectory": default_export_directory,
            "defaultBackupDirectory": default_backup_directory,
            "simpleMode": simple_mode,
        })
        .to_string(),
    )
    .await?;

    load_settings(pool, workspace_id).await
}

async fn load_settings(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<WorkspaceSettings, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, locale, updater_enabled, default_export_directory, default_backup_directory, dashboard_tour_completed, simple_mode
        FROM workspace_settings
        WHERE workspace_id = ?1
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Workspace settings not found", "workspaceId"))?;

    Ok(WorkspaceSettings {
        id: row.get("id"),
        locale: row.get("locale"),
        updater_enabled: row.get::<i64, _>("updater_enabled") != 0,
        default_export_directory: row.get("default_export_directory"),
        default_backup_directory: row.get("default_backup_directory"),
        dashboard_tour_completed: row.get::<i64, _>("dashboard_tour_completed") != 0,
        simple_mode: row.get::<i64, _>("simple_mode") != 0,
    })
}

pub async fn dashboard_tour_mark_complete(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<WorkspaceSettings, AppError> {
    ensure_default_settings(pool, workspace_id).await?;
    sqlx::query(
        r#"
        UPDATE workspace_settings
        SET dashboard_tour_completed = 1,
            updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?1
        "#,
    )
    .bind(workspace_id)
    .execute(pool)
    .await?;

    record_event(
        pool,
        workspace_id,
        "dashboard_tour_complete",
        "workspace_settings",
        None,
        "{}",
    )
    .await?;

    load_settings(pool, workspace_id).await
}

fn normalize_optional_directory(path: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(path) = path else {
        return Ok(None);
    };
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let validated = crate::paths::validate_user_directory(trimmed, "directory")?;
    Ok(Some(validated.to_string_lossy().to_string()))
}
