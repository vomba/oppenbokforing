use sqlx::SqlitePool;

use crate::error::AppError;

/// Trim and reject empty idempotency keys (shared across domain modules).
pub fn normalize_idempotency_key(key: &str) -> Result<&str, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(
            "Idempotency key is required",
            "idempotencyKey",
        ));
    }
    Ok(trimmed)
}

/// Fetch a `local_jobs` payload JSON for a workspace-scoped idempotency key.
pub async fn fetch_local_job_payload(
    pool: &SqlitePool,
    workspace_id: &str,
    job_type: &str,
    idempotency_key: &str,
) -> Result<Option<String>, AppError> {
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
    .bind(job_type)
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(existing)
}
