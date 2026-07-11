use sqlx::{Executor, SqlitePool};
use uuid::Uuid;

use crate::error::AppError;

pub async fn record_event(
    pool: &SqlitePool,
    workspace_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    metadata_json: &str,
) -> Result<(), AppError> {
    record_event_tx(pool, workspace_id, action, resource_type, resource_id, metadata_json).await
}

pub async fn record_event_tx<'e, E>(
    executor: E,
    workspace_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    metadata_json: &str,
) -> Result<(), AppError>
where
    E: Executor<'e, Database = sqlx::Sqlite>,
{
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO audit_events (id, workspace_id, action, resource_type, resource_id, metadata_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(metadata_json)
    .execute(executor)
    .await?;

    Ok(())
}
