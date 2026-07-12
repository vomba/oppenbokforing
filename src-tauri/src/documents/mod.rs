use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, SqlitePool};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

use crate::{audit::record_event, error::AppError, workspace::{ensure_path_within_root, safe_join_under}};

const JOB_DOCUMENT_IMPORT: &str = "document_import";

pub fn is_pdf_mime(mime_type: &str) -> bool {
    mime_type.trim().eq_ignore_ascii_case("application/pdf")
}

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
          original_filename = excluded.original_filename,
          mime_type = excluded.mime_type
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
          original_filename = excluded.original_filename,
          mime_type = excluded.mime_type
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
    #[cfg(windows)]
    if metadata_is_reparse_point(&meta) {
        return Err(AppError::validation(
            "Document path must be a regular file",
            "documentId",
        ));
    }
    let canonical = joined
        .canonicalize()
        .map_err(|_| AppError::validation("Document file not found", "documentId"))?;
    ensure_path_within_root(&canonical, documents_dir, "objectPath")?;
    Ok(canonical)
}

#[cfg(unix)]
fn open_reveal_source(source: &Path) -> Result<std::fs::File, AppError> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(source)
        .map_err(|_| AppError::validation("Document file not found", "documentId"))
}

#[cfg(windows)]
fn metadata_is_reparse_point(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(unix))]
fn open_reveal_source(source: &Path) -> Result<std::fs::File, AppError> {
    let file = std::fs::File::open(source)
        .map_err(|_| AppError::validation("Document file not found", "documentId"))?;
    let meta = file
        .metadata()
        .map_err(|_| AppError::validation("Document file not found", "documentId"))?;
    #[cfg(windows)]
    if metadata_is_reparse_point(&meta) {
        return Err(AppError::validation(
            "Document path must be a regular file",
            "documentId",
        ));
    }
    Ok(file)
}

fn reveal_extension_for_mime(mime_type: &str) -> Result<&'static str, AppError> {
    match mime_type.trim() {
        "application/pdf" => Ok("pdf"),
        "image/png" => Ok("png"),
        "image/jpeg" | "image/jpg" => Ok("jpg"),
        _ => Err(AppError::validation(
            "Document type cannot be opened in the system viewer",
            "documentId",
        )),
    }
}

#[cfg(unix)]
fn restrict_temp_permissions(file: &std::fs::File) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)?;
    Ok(())
}

const REVEAL_STAGING_PREFIX: &str = "oppenbokforing-reveal-";
/// Staged reveal copies are kept for the OS viewer; delete after a bounded TTL.
const REVEAL_STAGED_TTL: Duration = Duration::from_secs(3600);
const REVEAL_TASK_FAILED: &str = "Could not open document in the system viewer";

fn stage_reveal_copy(
    source: &Path,
    documents_dir: &Path,
    mime_type: &str,
) -> Result<PathBuf, AppError> {
    let source_meta = std::fs::symlink_metadata(source).map_err(|_| {
        AppError::validation("Document file not found", "documentId")
    })?;
    if source_meta.file_type().is_symlink() {
        return Err(AppError::validation(
            "Document path must be a regular file",
            "documentId",
        ));
    }

    let canonical = source
        .canonicalize()
        .map_err(|_| AppError::validation("Document file not found", "documentId"))?;
    ensure_path_within_root(&canonical, documents_dir, "objectPath")?;

    let meta = std::fs::symlink_metadata(&canonical).map_err(|_| {
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
    #[cfg(windows)]
    if metadata_is_reparse_point(&meta) {
        return Err(AppError::validation(
            "Document path must be a regular file",
            "documentId",
        ));
    }

    let extension = reveal_extension_for_mime(mime_type)?;
    let mut temp = tempfile::Builder::new()
        .prefix(REVEAL_STAGING_PREFIX)
        .suffix(&format!(".{extension}"))
        .tempfile()
        .map_err(|_| AppError::internal("Could not stage document for reveal"))?;

    #[cfg(unix)]
    restrict_temp_permissions(temp.as_file())?;

    {
        let mut input = open_reveal_source(&canonical)?;
        std::io::copy(&mut input, temp.as_file_mut())?;
    }

    let staged = temp
        .into_temp_path()
        .keep()
        .map_err(|_| AppError::internal("Could not stage document for reveal"))?;
    Ok(staged)
}

fn validate_staged_reveal_path(staged: &Path) -> Result<(), AppError> {
    let meta = std::fs::symlink_metadata(staged).map_err(|_| {
        AppError::internal(REVEAL_TASK_FAILED)
    })?;
    if meta.file_type().is_symlink() {
        return Err(AppError::internal(REVEAL_TASK_FAILED));
    }
    if !meta.is_file() {
        return Err(AppError::internal(REVEAL_TASK_FAILED));
    }
    #[cfg(windows)]
    if metadata_is_reparse_point(&meta) {
        return Err(AppError::internal(REVEAL_TASK_FAILED));
    }

    let file_name = staged
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::internal(REVEAL_TASK_FAILED))?;
    if !file_name.starts_with(REVEAL_STAGING_PREFIX) {
        return Err(AppError::internal(REVEAL_TASK_FAILED));
    }

    let temp_dir = std::env::temp_dir();
    ensure_path_within_root(staged, &temp_dir, "stagedPath")?;
    Ok(())
}

fn reveal_cleanup_sender() -> &'static mpsc::Sender<PathBuf> {
    static SENDER: OnceLock<mpsc::Sender<PathBuf>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("reveal-cleanup".into())
            .spawn(move || {
                let mut due: BinaryHeap<Reverse<(Instant, PathBuf)>> = BinaryHeap::new();
                loop {
                    let now = Instant::now();
                    while due
                        .peek()
                        .is_some_and(|Reverse((deadline, _))| *deadline <= now)
                    {
                        if let Some(Reverse((_, path))) = due.pop() {
                            let _ = std::fs::remove_file(path);
                        }
                    }

                    let timeout = due
                        .peek()
                        .map(|Reverse((deadline, _))| deadline.saturating_duration_since(now))
                        .unwrap_or(REVEAL_STAGED_TTL);

                    match rx.recv_timeout(timeout) {
                        Ok(path) => {
                            due.push(Reverse((Instant::now() + REVEAL_STAGED_TTL, path)));
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            while let Some(Reverse((_, path))) = due.pop() {
                                let _ = std::fs::remove_file(path);
                            }
                            break;
                        }
                    }
                }
            })
            .expect("reveal cleanup worker");
        tx
    })
}

fn schedule_reveal_cleanup(path: PathBuf) {
    let _ = reveal_cleanup_sender().send(path);
}

/// Remove leftover reveal staging files from prior app sessions.
pub fn purge_stale_reveal_staging() {
    let temp_dir = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&temp_dir) else {
        return;
    };
    let cutoff = SystemTime::now()
        .checked_sub(REVEAL_STAGED_TTL)
        .unwrap_or(SystemTime::UNIX_EPOCH);

    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !name.starts_with(REVEAL_STAGING_PREFIX) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified < cutoff {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn reveal_document_blocking(
    documents_dir: &Path,
    object_path: &str,
    mime_type: &str,
) -> Result<(), AppError> {
    let full_path = prepare_reveal_path(documents_dir, object_path)?;
    let staged = stage_reveal_copy(&full_path, documents_dir, mime_type)?;
    validate_staged_reveal_path(&staged)?;
    schedule_reveal_cleanup(staged.clone());
    reveal_in_system_viewer(&staged)?;
    Ok(())
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
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let path = full_path.to_string_lossy().replace('\'', "''");
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("Start-Process -LiteralPath '{path}'"),
            ])
            .creation_flags(CREATE_NO_WINDOW)
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
    let object_path = document.object_path.clone();
    let mime_type = document.mime_type.clone();

    tokio::task::spawn_blocking(move || {
        reveal_document_blocking(&documents_dir, &object_path, &mime_type)
    })
    .await
    .map_err(|_| AppError::internal(REVEAL_TASK_FAILED))??;

    Ok(())
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

#[cfg(test)]
mod reveal_tests {
    use super::{
        purge_stale_reveal_staging, reveal_extension_for_mime, stage_reveal_copy,
        validate_staged_reveal_path, REVEAL_STAGING_PREFIX,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reveal_extension_rejects_unsupported_mime_types() {
        let error = reveal_extension_for_mime("application/x-msdownload").expect_err("reject exe");
        assert_eq!(error.code, "validation_error");
    }

    #[test]
    fn stage_reveal_copy_writes_randomized_pdf_with_extension() {
        let dir = tempdir().expect("tempdir");
        let source = dir.path().join("objects").join("deadbeef");
        fs::create_dir_all(source.parent().expect("parent")).expect("objects dir");
        fs::write(&source, b"%PDF-1.3 test").expect("source pdf");

        let staged =
            stage_reveal_copy(&source, dir.path(), "application/pdf").expect("stage reveal copy");

        assert_eq!(staged.extension().and_then(|ext| ext.to_str()), Some("pdf"));
        assert!(staged
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(REVEAL_STAGING_PREFIX)));
        assert_eq!(fs::read(&staged).expect("read staged"), b"%PDF-1.3 test");
        validate_staged_reveal_path(&staged).expect("staged path is safe to reveal");
        let _ = fs::remove_file(staged);
    }

    #[cfg(unix)]
    #[test]
    fn stage_reveal_copy_rejects_symlink_source() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("real.pdf");
        fs::write(&target, b"%PDF-1.3").expect("target pdf");
        let link = dir.path().join("link.pdf");
        symlink(&target, &link).expect("symlink");

        let error =
            stage_reveal_copy(&link, dir.path(), "application/pdf").expect_err("symlink source");
        assert_eq!(error.code, "validation_error");
    }

    #[cfg(unix)]
    #[test]
    fn validate_staged_reveal_path_rejects_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("real.pdf");
        fs::write(&target, b"%PDF-1.3").expect("target pdf");
        let link = dir.path().join("link.pdf");
        symlink(&target, &link).expect("symlink");

        let error = validate_staged_reveal_path(&link).expect_err("symlink staged path");
        assert_eq!(error.code, "internal_error");
    }

    #[test]
    fn purge_stale_reveal_staging_keeps_fresh_temp_files() {
        let temp_dir = std::env::temp_dir();
        let fresh_name = format!("{REVEAL_STAGING_PREFIX}test-fresh.pdf");
        let fresh_path = temp_dir.join(fresh_name);
        fs::write(&fresh_path, b"fresh").expect("write fresh reveal temp");

        purge_stale_reveal_staging();

        assert!(
            fresh_path.exists(),
            "fresh reveal temp should survive startup sweep"
        );
        let _ = fs::remove_file(fresh_path);
    }
}
