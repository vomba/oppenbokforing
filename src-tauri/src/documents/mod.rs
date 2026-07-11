use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

use crate::{audit::record_event, error::AppError, workspace::{ensure_path_within_root, safe_join_under}};

const JOB_DOCUMENT_IMPORT: &str = "document_import";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub object_path: String,
    pub content_sha256: String,
    pub mime_type: String,
    pub original_filename: String,
    pub retention_years: i64,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentImportInput {
    pub source_path: String,
    pub filename: String,
    pub mime_type: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentDocumentPayload {
    idempotency_key: String,
    content_sha256: String,
    document: Option<Document>,
}

#[derive(Debug)]
enum DocumentImportClaim {
    Proceed,
    Cached(Document),
}

async fn claim_document_import(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    content_sha256: &str,
) -> Result<DocumentImportClaim, AppError> {
    if let Some(existing) = check_idempotency(pool, workspace_id, idempotency_key).await? {
        if existing.content_sha256 != content_sha256 {
            return Err(AppError::validation(
                "Idempotency key was already used for a different document",
                "idempotencyKey",
            ));
        }
        return Ok(DocumentImportClaim::Cached(existing));
    }

    let key = normalize_idempotency_key(idempotency_key)?;
    let payload = IdempotentDocumentPayload {
        idempotency_key: key.to_string(),
        content_sha256: content_sha256.to_string(),
        document: None,
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| AppError::internal(error.to_string()))?;

    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'running', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(payload_json)
    .bind(key)
    .execute(pool)
    .await
    {
        Ok(_) => Ok(DocumentImportClaim::Proceed),
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            wait_for_document_import_winner(pool, workspace_id, idempotency_key, content_sha256)
                .await
                .map(DocumentImportClaim::Cached)
        }
        Err(error) => Err(error.into()),
    }
}

async fn wait_for_document_import_winner(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    content_sha256: &str,
) -> Result<Document, AppError> {
    for _ in 0..100 {
        if let Some(existing) = check_idempotency(pool, workspace_id, idempotency_key).await? {
            if existing.content_sha256 != content_sha256 {
                return Err(AppError::validation(
                    "Idempotency key was already used for a different document",
                    "idempotencyKey",
                ));
            }
            return Ok(existing);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    Err(AppError::validation(
        "Document import already in progress for this idempotency key",
        "idempotencyKey",
    ))
}

async fn finalize_document_import(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    content_sha256: &str,
    document: &Document,
) -> Result<(), AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload = IdempotentDocumentPayload {
        idempotency_key: key.to_string(),
        content_sha256: content_sha256.to_string(),
        document: Some(document.clone()),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| AppError::internal(error.to_string()))?;

    sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = 'succeeded', payload_json = ?4
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(key)
    .bind(payload_json)
    .execute(pool)
    .await?;

    Ok(())
}

fn normalize_idempotency_key(key: &str) -> Result<&str, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Idempotency key is required", "idempotencyKey"));
    }
    Ok(trimmed)
}

async fn load_documents_dir(pool: &SqlitePool, workspace_id: &str) -> Result<PathBuf, AppError> {
    let path: Option<String> = sqlx::query_scalar(
        r#"
        SELECT documents_path FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    let path = path.ok_or_else(|| AppError::validation("Workspace not found", "workspaceId"))?;
    Ok(PathBuf::from(path))
}

async fn check_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<Document>, AppError> {
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
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(payload) = existing else {
        return Ok(None);
    };

    let parsed: IdempotentDocumentPayload = serde_json::from_str(&payload)
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(parsed.document)
}

async fn record_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    content_sha256: &str,
    document: &Document,
) -> Result<(), AppError> {
    finalize_document_import(pool, workspace_id, idempotency_key, content_sha256, document).await
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

pub async fn document_import(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &DocumentImportInput,
) -> Result<Document, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;

    let source_path = Path::new(input.source_path.trim());
    if input.source_path.trim().is_empty() || !source_path.exists() {
        return Err(AppError::validation("Source file not found", "sourcePath"));
    }
    if input.filename.trim().is_empty() {
        return Err(AppError::validation("Filename is required", "filename"));
    }
    if input.mime_type.trim().is_empty() {
        return Err(AppError::validation("MIME type is required", "mimeType"));
    }

    // Basic size guard to avoid importing unreasonably large documents into the
    // local workspace archive.
    const MAX_DOCUMENT_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB
    let metadata = std::fs::metadata(source_path)?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(AppError::validation(
            "Source file is too large to import",
            "sourcePath",
        ));
    }

    let bytes = std::fs::read(source_path)?;
    let content_sha256 = sha256_hex(&bytes);

    match claim_document_import(pool, workspace_id, idempotency_key, &content_sha256).await? {
        DocumentImportClaim::Cached(existing) => return Ok(existing),
        DocumentImportClaim::Proceed => {}
    }

    let documents_dir = load_documents_dir(pool, workspace_id).await?;
    std::fs::create_dir_all(&documents_dir)?;

    let object_rel = format!("objects/{content_sha256}");
    let object_abs = documents_dir.join(&object_rel);
    if let Some(parent) = object_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !object_abs.exists() {
        std::fs::write(&object_abs, &bytes)?;
    }

    let id = Uuid::new_v4().to_string();
    let retention_years = 7i64;

    sqlx::query(
        r#"
        INSERT INTO documents (
          id, workspace_id, object_path, content_sha256, mime_type, original_filename, retention_years
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(workspace_id, content_sha256) DO UPDATE SET
          original_filename = excluded.original_filename
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(&object_rel)
    .bind(&content_sha256)
    .bind(input.mime_type.trim())
    .bind(input.filename.trim())
    .bind(retention_years)
    .execute(pool)
    .await?;

    let row = sqlx::query(
        r#"
        SELECT id, object_path, content_sha256, mime_type, original_filename, retention_years
        FROM documents
        WHERE workspace_id = ?1 AND content_sha256 = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&content_sha256)
    .fetch_one(pool)
    .await?;

    let document = Document {
        id: row.get("id"),
        object_path: row.get("object_path"),
        content_sha256: row.get("content_sha256"),
        mime_type: row.get("mime_type"),
        original_filename: row.get("original_filename"),
        retention_years: row.get("retention_years"),
    };

    record_idempotency(
        pool,
        workspace_id,
        idempotency_key,
        &content_sha256,
        &document,
    )
    .await?;

    record_event(
        pool,
        workspace_id,
        "document_import",
        "document",
        Some(&document.id),
        &serde_json::to_string(&document).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(document)
}

pub async fn store_document_bytes(
    pool: &SqlitePool,
    workspace_id: &str,
    bytes: &[u8],
    filename: &str,
    mime_type: &str,
) -> Result<Document, AppError> {
    if filename.trim().is_empty() {
        return Err(AppError::validation("Filename is required", "filename"));
    }
    if mime_type.trim().is_empty() {
        return Err(AppError::validation("MIME type is required", "mimeType"));
    }
    const MAX_DOCUMENT_BYTES: u64 = 10 * 1024 * 1024;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(AppError::validation(
            "Document is too large to store",
            "document",
        ));
    }

    let content_sha256 = sha256_hex(bytes);
    let documents_dir = load_documents_dir(pool, workspace_id).await?;
    std::fs::create_dir_all(&documents_dir)?;

    let object_rel = format!("objects/{content_sha256}");
    let object_abs = documents_dir.join(&object_rel);
    if let Some(parent) = object_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !object_abs.exists() {
        std::fs::write(&object_abs, bytes)?;
    }

    let id = Uuid::new_v4().to_string();
    let retention_years = 7i64;

    sqlx::query(
        r#"
        INSERT INTO documents (
          id, workspace_id, object_path, content_sha256, mime_type, original_filename, retention_years
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(workspace_id, content_sha256) DO UPDATE SET
          original_filename = excluded.original_filename
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(&object_rel)
    .bind(&content_sha256)
    .bind(mime_type.trim())
    .bind(filename.trim())
    .bind(retention_years)
    .execute(pool)
    .await?;

    let row = sqlx::query(
        r#"
        SELECT id, object_path, content_sha256, mime_type, original_filename, retention_years
        FROM documents
        WHERE workspace_id = ?1 AND content_sha256 = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(&content_sha256)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::storage("Stored document could not be loaded"))?;

    Ok(Document {
        id: row.get("id"),
        object_path: row.get("object_path"),
        content_sha256: row.get("content_sha256"),
        mime_type: row.get("mime_type"),
        original_filename: row.get("original_filename"),
        retention_years: row.get("retention_years"),
    })
}

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentListInput {
    pub unattached_only: Option<bool>,
    pub limit: Option<i64>,
    pub before_id: Option<String>,
}

fn document_list_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(100).clamp(1, 500)
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentGetInput {
    pub document_id: String,
}

fn map_document_row(row: sqlx::sqlite::SqliteRow) -> Document {
    Document {
        id: row.get("id"),
        object_path: row.get("object_path"),
        content_sha256: row.get("content_sha256"),
        mime_type: row.get("mime_type"),
        original_filename: row.get("original_filename"),
        retention_years: row.get("retention_years"),
    }
}

pub async fn document_get(
    pool: &SqlitePool,
    workspace_id: &str,
    document_id: &str,
) -> Result<Document, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, object_path, content_sha256, mime_type, original_filename, retention_years
        FROM documents
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(document_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Document not found", "documentId"))?;

    Ok(map_document_row(row))
}

fn prepare_reveal_path(documents_dir: &Path, object_path: &str) -> Result<PathBuf, AppError> {
    let joined = safe_join_under(documents_dir, object_path, "objectPath")?;
    let meta = std::fs::symlink_metadata(&joined).map_err(|_| {
        AppError::validation("Document file not found", "documentId")
    })?;
    if meta.file_type().is_symlink() {
        return Err(AppError::validation(
            "Document path must be a regular file",
            "documentId",
        ));
    }
    if !meta.is_file() {
        return Err(AppError::validation("Document file not found", "documentId"));
    }
    let canonical = joined
        .canonicalize()
        .map_err(|_| AppError::validation("Document file not found", "documentId"))?;
    ensure_path_within_root(&canonical, documents_dir, "objectPath")?;
    Ok(canonical)
}

fn reveal_in_system_viewer(full_path: &Path) -> Result<(), AppError> {
    const REVEAL_FAILED: &str = "Could not open document in the system viewer";

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(full_path)
            .spawn()
            .map_err(|_| AppError::internal(REVEAL_FAILED))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(full_path)
            .spawn()
            .map_err(|_| AppError::internal(REVEAL_FAILED))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(full_path)
            .spawn()
            .map_err(|_| AppError::internal(REVEAL_FAILED))?;
    }

    Ok(())
}

pub async fn document_reveal(
    pool: &SqlitePool,
    workspace_id: &str,
    document_id: &str,
) -> Result<(), AppError> {
    let documents_dir = load_documents_dir(pool, workspace_id).await?;
    let document = document_get(pool, workspace_id, document_id).await?;
    let full_path = prepare_reveal_path(&documents_dir, &document.object_path)?;
    reveal_in_system_viewer(&full_path)
}

pub async fn document_list(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &DocumentListInput,
) -> Result<Vec<Document>, AppError> {
    let limit = document_list_limit(input.limit);
    let unattached_only = input.unattached_only.unwrap_or(false);
    let before_id = input
        .before_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let rows = sqlx::query(
        r#"
        SELECT d.id, d.object_path, d.content_sha256, d.mime_type,
               d.original_filename, d.retention_years
        FROM documents d
        WHERE d.workspace_id = ?1
          AND (
            ?2 = 0
            OR NOT EXISTS (
              SELECT 1 FROM vouchers v
              WHERE v.document_id = d.id AND v.workspace_id = ?1
            )
          )
          AND (
            ?3 IS NULL
            OR (
              d.created_at <
              (SELECT created_at FROM documents WHERE id = ?3 AND workspace_id = ?1)
              OR (
                d.created_at =
                (SELECT created_at FROM documents WHERE id = ?3 AND workspace_id = ?1)
                AND d.id < ?3
              )
            )
          )
        ORDER BY d.created_at DESC, d.id DESC
        LIMIT ?4
        "#,
    )
    .bind(workspace_id)
    .bind(if unattached_only { 1 } else { 0 })
    .bind(before_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(map_document_row)
        .collect())
}

