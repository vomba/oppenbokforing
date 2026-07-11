use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{audit::record_event, documents, error::AppError};

const JOB_CSV_IMPORT: &str = "csv_import_create";

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CsvImportCreateInput {
    pub source_path: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CsvImportSummary {
    pub id: String,
    pub staged_count: i64,
    pub first_staged_transaction_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentCsvPayload {
    source_content_sha256: String,
    summary: CsvImportSummary,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        write!(&mut out, "{:02x}", b).expect("hex encode");
    }
    out
}

fn validate_csv_idempotency_inputs(
    source_content_sha256: &str,
    cached: &IdempotentCsvPayload,
) -> Result<(), AppError> {
    if cached.source_content_sha256 != source_content_sha256 {
        return Err(AppError::validation(
            "Idempotency key was already used for a different CSV file",
            "idempotencyKey",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParsedRow {
    date: String,
    description: String,
    amount_minor: i64,
}

fn normalize_idempotency_key(key: &str) -> Result<&str, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Idempotency key is required", "idempotencyKey"));
    }
    Ok(trimmed)
}

async fn check_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentCsvPayload>, AppError> {
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
    .bind(JOB_CSV_IMPORT)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(payload) = existing else {
        return Ok(None);
    };
    Ok(Some(
        serde_json::from_str(&payload).map_err(|error| AppError::internal(error.to_string()))?,
    ))
}

async fn record_idempotency(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    idempotency_key: &str,
    source_content_sha256: &str,
    summary: &CsvImportSummary,
) -> Result<(), AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload = IdempotentCsvPayload {
        source_content_sha256: source_content_sha256.to_string(),
        summary: summary.clone(),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| AppError::internal(error.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_CSV_IMPORT)
    .bind(payload_json)
    .bind(key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn parse_csv(raw: &str) -> Result<Vec<ParsedRow>, AppError> {
    let mut lines = raw.lines();
    let header = lines.next().ok_or_else(|| AppError::validation("CSV is empty", "sourcePath"))?;
    let columns: Vec<&str> = header.split(',').map(str::trim).collect();
    let date_idx = columns
        .iter()
        .position(|c| *c == "date")
        .ok_or_else(|| AppError::validation("CSV must include date column", "sourcePath"))?;
    let desc_idx = columns
        .iter()
        .position(|c| *c == "description")
        .ok_or_else(|| AppError::validation("CSV must include description column", "sourcePath"))?;
    let amount_idx = columns
        .iter()
        .position(|c| *c == "amount_minor")
        .ok_or_else(|| AppError::validation("CSV must include amount_minor column", "sourcePath"))?;

    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let (date, description, amount_minor) = if columns.len() == 3
            && date_idx == 0
            && desc_idx == 1
            && amount_idx == 2
        {
            let last_comma = line
                .rfind(',')
                .ok_or_else(|| AppError::validation("Invalid CSV row", "sourcePath"))?;
            let amount_minor = line[last_comma + 1..]
                .trim()
                .trim_matches('"')
                .parse::<i64>()
                .map_err(|_| AppError::validation("Invalid amount_minor", "sourcePath"))?;
            let rest = &line[..last_comma];
            let first_comma = rest
                .find(',')
                .ok_or_else(|| AppError::validation("Invalid CSV row", "sourcePath"))?;
            let date = rest[..first_comma].trim().trim_matches('"').to_string();
            let description = rest[first_comma + 1..]
                .trim()
                .trim_matches('"')
                .to_string();
            (date, description, amount_minor)
        } else {
            let parts: Vec<&str> = line.split(',').collect();
            let date = parts.get(date_idx).unwrap_or(&"").trim().to_string();
            let description = parts.get(desc_idx).unwrap_or(&"").trim().to_string();
            let amount_minor = parts
                .get(amount_idx)
                .unwrap_or(&"0")
                .trim()
                .parse::<i64>()
                .map_err(|_| AppError::validation("Invalid amount_minor", "sourcePath"))?;
            (date, description, amount_minor)
        };

        if date.is_empty() || description.is_empty() {
            return Err(AppError::validation("Invalid CSV row", "sourcePath"));
        }
        rows.push(ParsedRow {
            date,
            description,
            amount_minor,
        });
    }
    if rows.is_empty() {
        return Err(AppError::validation("CSV contains no rows", "sourcePath"));
    }
    Ok(rows)
}

pub async fn csv_import_create(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &CsvImportCreateInput,
) -> Result<CsvImportSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;

    let source_path = input.source_path.trim();
    if source_path.is_empty() {
        return Err(AppError::validation("Source path is required", "sourcePath"));
    }

    let raw_bytes = std::fs::read(source_path)?;
    let source_content_sha256 = sha256_hex(&raw_bytes);
    let raw = String::from_utf8(raw_bytes).map_err(|error| AppError::validation(
        format!("CSV must be valid UTF-8: {error}"),
        "sourcePath",
    ))?;
    let parsed = parse_csv(&raw)?;

    if let Some(cached) = check_idempotency(pool, workspace_id, idempotency_key).await? {
        validate_csv_idempotency_inputs(&source_content_sha256, &cached)?;
        return finish_csv_import(
            pool,
            workspace_id,
            idempotency_key,
            source_path,
            &parsed,
            cached.summary,
        )
        .await;
    }

    let summary = {
        let csv_import_id = Uuid::new_v4().to_string();
        let mut tx = pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO csv_imports (id, workspace_id, status)
            VALUES (?1, ?2, 'parsed')
            "#,
        )
        .bind(&csv_import_id)
        .bind(workspace_id)
        .execute(&mut *tx)
        .await?;

        let mut first_id: Option<String> = None;
        for row in &parsed {
            let staged_id = Uuid::new_v4().to_string();
            if first_id.is_none() {
                first_id = Some(staged_id.clone());
            }
            sqlx::query(
                r#"
                INSERT INTO staged_transactions (
                  id, workspace_id, csv_import_id, transaction_date, description, amount_minor, status
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'staged')
                "#,
            )
            .bind(&staged_id)
            .bind(workspace_id)
            .bind(&csv_import_id)
            .bind(&row.date)
            .bind(&row.description)
            .bind(row.amount_minor)
            .execute(&mut *tx)
            .await?;
        }

        let new_summary = CsvImportSummary {
            id: csv_import_id,
            staged_count: parsed.len() as i64,
            first_staged_transaction_id: first_id
                .ok_or_else(|| AppError::internal("Missing staged id"))?,
        };

        match record_idempotency(
            &mut tx,
            workspace_id,
            idempotency_key,
            &source_content_sha256,
            &new_summary,
        )
        .await
        {
            Ok(()) => {
                tx.commit().await?;
                new_summary
            }
            Err(error)
                if error.is_unique_violation() =>
            {
                tx.rollback().await?;
                let cached = check_idempotency(pool, workspace_id, idempotency_key)
                    .await?
                    .ok_or_else(|| AppError::internal("Idempotent CSV replay failed"))?;
                validate_csv_idempotency_inputs(&source_content_sha256, &cached)?;
                cached.summary
            }
            Err(error) => {
                tx.rollback().await?;
                return Err(error);
            }
        }
    };

    finish_csv_import(
        pool,
        workspace_id,
        idempotency_key,
        source_path,
        &parsed,
        summary,
    )
    .await
}

async fn finish_csv_import(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    source_path: &str,
    parsed: &[ParsedRow],
    summary: CsvImportSummary,
) -> Result<CsvImportSummary, AppError> {
    let csv_document = documents::document_import(
        pool,
        workspace_id,
        &documents::DocumentImportInput {
            source_path: source_path.to_string(),
            filename: "bank.csv".to_string(),
            mime_type: "text/csv".to_string(),
            idempotency_key: format!("csv-doc:{idempotency_key}"),
        },
    )
    .await?;

    // Best-effort link back to the source document; idempotent on retry.
    sqlx::query(
        r#"
        UPDATE csv_imports
        SET source_document_id = ?1
        WHERE id = ?2 AND workspace_id = ?3
        "#,
    )
    .bind(&csv_document.id)
    .bind(&summary.id)
    .bind(workspace_id)
    .execute(pool)
    .await?;

    let audit_exists: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM audit_events
        WHERE workspace_id = ?1
          AND action = 'csv_import_create'
          AND resource_id = ?2
        "#,
    )
    .bind(workspace_id)
    .bind(&summary.id)
    .fetch_one(pool)
    .await?;

    if audit_exists == 0 {
        record_event(
            pool,
            workspace_id,
            "csv_import_create",
            "csv_import",
            Some(&summary.id),
            &serde_json::json!({
                "rows": parsed.len(),
                "sourceDocumentId": csv_document.id,
            })
            .to_string(),
        )
        .await?;
    }

    Ok(summary)
}

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StagedTransactionsListInput {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub before_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StagedTransactionSummary {
    pub id: String,
    pub csv_import_id: Option<String>,
    pub transaction_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub status: String,
}

fn staged_list_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(100).clamp(1, 500)
}

pub async fn staged_transactions_count(
    pool: &SqlitePool,
    workspace_id: &str,
    status: Option<&str>,
) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM staged_transactions
        WHERE workspace_id = ?1
          AND (?2 IS NULL OR status = ?2)
        "#,
    )
    .bind(workspace_id)
    .bind(status)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub async fn staged_transactions_list(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &StagedTransactionsListInput,
) -> Result<Vec<StagedTransactionSummary>, AppError> {
    let limit = staged_list_limit(input.limit);
    let before_id = input
        .before_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let rows = sqlx::query(
        r#"
        SELECT id, csv_import_id, transaction_date, description, amount_minor, status
        FROM staged_transactions
        WHERE workspace_id = ?1
          AND (?2 IS NULL OR status = ?2)
          AND (
            ?3 IS NULL
            OR (
              transaction_date <
              (SELECT transaction_date FROM staged_transactions WHERE id = ?3 AND workspace_id = ?1)
              OR (
                transaction_date =
                (SELECT transaction_date FROM staged_transactions WHERE id = ?3 AND workspace_id = ?1)
                AND id < ?3
              )
            )
          )
        ORDER BY transaction_date DESC, id DESC
        LIMIT ?4
        "#,
    )
    .bind(workspace_id)
    .bind(input.status.as_deref())
    .bind(before_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| StagedTransactionSummary {
            id: row.get("id"),
            csv_import_id: row.get("csv_import_id"),
            transaction_date: row.get("transaction_date"),
            description: row.get("description"),
            amount_minor: row.get("amount_minor"),
            status: row.get("status"),
        })
        .collect())
}

