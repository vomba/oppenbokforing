use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{audit::record_event, error::AppError};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Counterparty {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub email: Option<String>,
    pub org_number: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CounterpartyCreateInput {
    pub kind: String,
    pub name: String,
    pub email: Option<String>,
    pub org_number: Option<String>,
}

pub async fn list_counterparties(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Vec<Counterparty>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, kind, name, email, org_number
        FROM counterparties
        WHERE workspace_id = ?1
        ORDER BY name ASC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| Counterparty {
            id: row.get("id"),
            kind: row.get("kind"),
            name: row.get("name"),
            email: row.get("email"),
            org_number: row.get("org_number"),
        })
        .collect())
}

pub async fn create_counterparty(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &CounterpartyCreateInput,
) -> Result<Counterparty, AppError> {
    let kind = input.kind.trim();
    if !matches!(kind, "customer" | "supplier") {
        return Err(AppError::validation("Invalid counterparty kind", "kind"));
    }
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::validation("Name is required", "name"));
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO counterparties (id, workspace_id, kind, name, email, org_number)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(kind)
    .bind(name)
    .bind(input.email.as_deref())
    .bind(input.org_number.as_deref())
    .execute(pool)
    .await?;

    let counterparty = Counterparty {
        id,
        kind: kind.to_string(),
        name: name.to_string(),
        email: input.email.clone(),
        org_number: input.org_number.clone(),
    };

    record_event(
        pool,
        workspace_id,
        "counterparty_create",
        "counterparty",
        Some(&counterparty.id),
        &serde_json::to_string(&counterparty).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(counterparty)
}
