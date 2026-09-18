use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, SqlitePool};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

use crate::{
    audit::record_event_tx,
    error::AppError,
    workspace::{ensure_path_within_root, resolve_workspace_exports_dir, safe_join_under},
};

const JOB_DOCUMENT_IMPORT: &str = "document_import";

const RETAINED_DOCUMENT_INTEGRITY_ERROR: &str = "Retained document integrity check failed";

pub fn is_pdf_mime(mime_type: &str) -> bool {
    mime_type.trim().eq_ignore_ascii_case("application/pdf")
}

/// Sniff common document MIME types from magic bytes.
pub fn sniff_document_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }
    if bytes.len() >= 8 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some("image/jpeg");
    }
    None
}

fn normalize_declared_mime(mime_type: &str) -> String {
    let trimmed = mime_type.trim();
    if trimmed.eq_ignore_ascii_case("image/jpg") {
        "image/jpeg".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

/// Resolve a trusted MIME type: sniffed content wins; declared type must match when sniffable.
/// `application/octet-stream` is treated as undeclared (Documents picker / OS dialogs).
pub fn resolve_document_mime(declared: &str, bytes: &[u8]) -> Result<String, AppError> {
    let declared = normalize_declared_mime(declared);
    if declared.is_empty() {
        return Err(AppError::validation("MIME type is required", "mimeType"));
    }
    match sniff_document_mime(bytes) {
        Some(sniffed)
            if sniffed == declared || declared == "application/octet-stream" =>
        {
            Ok(sniffed.to_string())
        }
        Some(sniffed) => Err(AppError::validation(
            format!("Declared MIME type does not match file content (expected {sniffed})"),
            "mimeType",
        )),
        None => {
            // CSV/plain text imports are not magic-byte sniffable; allow when declared.
            if matches!(declared.as_str(), "text/csv" | "text/plain") {
                Ok(declared)
            } else {
                Err(AppError::validation(
                    "Unsupported document content; expected PDF, PNG, JPEG, or CSV",
                    "mimeType",
                ))
            }
        }
    }
}

/// Extension MIME for reveal: prefer sniffed bytes so legacy `octet-stream` rows still open.
fn resolve_reveal_mime(declared: &str, header: &[u8]) -> Result<String, AppError> {
    if let Some(sniffed) = sniff_document_mime(header) {
        return Ok(sniffed.to_string());
    }
    let declared = normalize_declared_mime(declared);
    reveal_extension_for_mime(&declared)?;
    Ok(declared)
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
struct DocumentImportLeasePayload {
    token: String,
    expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentDocumentPayload {
    idempotency_key: String,
    content_sha256: String,
    document: Option<Document>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lease: Option<DocumentImportLeasePayload>,
}

#[derive(Debug, Clone)]
struct DocumentImportLease {
    token: String,
}

impl DocumentImportLease {
    fn new() -> Self {
        Self {
            token: Uuid::new_v4().to_string(),
        }
    }
}

#[derive(Debug)]
enum DocumentImportClaim {
    Proceed(DocumentImportLease),
    Cached(Document),
}

const DOCUMENT_IMPORT_CLAIM_LEASE_DURATION: &str = "+5 minutes";
const LEGACY_STALE_IDEMPOTENCY_CLAIM_AFTER: &str = "-5 minutes";
const DOCUMENT_IMPORT_CLAIM_FAILURE_REASON: &str = "Document import failed";

fn inactive_document_import_claim_error() -> AppError {
    AppError::validation(
        "Document import claim is no longer active for this idempotency key",
        "idempotencyKey",
    )
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
    let lease = DocumentImportLease::new();
    let payload = IdempotentDocumentPayload {
        idempotency_key: key.to_string(),
        content_sha256: content_sha256.to_string(),
        document: None,
        lease: Some(DocumentImportLeasePayload {
            token: lease.token.clone(),
            expires_at: String::new(),
        }),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| AppError::internal(error.to_string()))?;

    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'running',
            json_set(?4, '$.lease.expiresAt', datetime('now', ?5)),
            ?6)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(payload_json)
    .bind(DOCUMENT_IMPORT_CLAIM_LEASE_DURATION)
    .bind(key)
    .execute(pool)
    .await
    {
        Ok(_) => Ok(DocumentImportClaim::Proceed(lease)),
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            if reclaim_document_import_claim(
                pool,
                workspace_id,
                idempotency_key,
                content_sha256,
                &lease,
            )
            .await?
            {
                return Ok(DocumentImportClaim::Proceed(lease));
            }
            if let Some(existing) = check_idempotency(pool, workspace_id, idempotency_key).await? {
                if existing.content_sha256 != content_sha256 {
                    return Err(AppError::validation(
                        "Idempotency key was already used for a different document",
                        "idempotencyKey",
                    ));
                }
                return Ok(DocumentImportClaim::Cached(existing));
            }
            Err(AppError::validation(
                "Document import already in progress for this idempotency key",
                "idempotencyKey",
            ))
        }
        Err(error) => Err(error.into()),
    }
}

async fn reclaim_document_import_claim(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    content_sha256: &str,
    lease: &DocumentImportLease,
) -> Result<bool, AppError> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let payload: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(&key)
    .fetch_optional(pool)
    .await?;
    let Some(payload) = payload else {
        return Ok(false);
    };
    let existing: IdempotentDocumentPayload =
        serde_json::from_str(&payload).map_err(|error| AppError::internal(error.to_string()))?;
    if existing.content_sha256 != content_sha256 {
        return Err(AppError::validation(
            "Idempotency key was already used for a different document",
            "idempotencyKey",
        ));
    }

    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = 'running',
            attempts = attempts + 1,
            payload_json = json_set(
                payload_json,
                '$.lease',
                json_object(
                    'token', ?4,
                    'expiresAt', datetime('now', ?5)
                )
            ),
            last_error = NULL,
            updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND idempotency_key = ?3
          AND json_extract(payload_json, '$.contentSha256') = ?6
          AND (
            status = 'failed'
            OR (
                status = 'running'
                AND (
                    datetime(json_extract(payload_json, '$.lease.expiresAt')) <= CURRENT_TIMESTAMP
                    OR (
                        json_extract(payload_json, '$.lease.expiresAt') IS NULL
                        AND updated_at <= datetime('now', ?7)
                    )
                )
            )
          )
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(&key)
    .bind(&lease.token)
    .bind(DOCUMENT_IMPORT_CLAIM_LEASE_DURATION)
    .bind(content_sha256)
    .bind(LEGACY_STALE_IDEMPOTENCY_CLAIM_AFTER)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

async fn renew_document_import_lease(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    lease: &DocumentImportLease,
) -> Result<(), AppError> {
    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET payload_json = json_set(
                payload_json,
                '$.lease.expiresAt',
                datetime('now', ?5)
            ),
            updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND idempotency_key = ?3
          AND status = 'running'
          AND json_extract(payload_json, '$.lease.token') = ?4
          AND datetime(json_extract(payload_json, '$.lease.expiresAt')) > CURRENT_TIMESTAMP
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(DOCUMENT_IMPORT_CLAIM_LEASE_DURATION)
    .execute(pool)
    .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(inactive_document_import_claim_error())
    }
}

async fn fail_document_import_claim(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    lease: &DocumentImportLease,
) -> Result<(), AppError> {
    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = 'failed',
            payload_json = json_remove(payload_json, '$.lease'),
            last_error = ?5,
            updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND idempotency_key = ?3
          AND status = 'running'
          AND json_extract(payload_json, '$.lease.token') = ?4
          AND datetime(json_extract(payload_json, '$.lease.expiresAt')) > CURRENT_TIMESTAMP
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(DOCUMENT_IMPORT_CLAIM_FAILURE_REASON)
    .execute(pool)
    .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(inactive_document_import_claim_error())
    }
}

#[cfg(test)]
async fn finalize_document_import_claim(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    lease: &DocumentImportLease,
    document: &Document,
) -> Result<(), AppError> {
    let payload = IdempotentDocumentPayload {
        idempotency_key: normalize_idempotency_key(idempotency_key)?.to_string(),
        content_sha256: document.content_sha256.clone(),
        document: Some(document.clone()),
        lease: None,
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| AppError::internal(error.to_string()))?;
    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = 'succeeded', payload_json = ?5, last_error = NULL, updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND idempotency_key = ?3
          AND status = 'running'
          AND json_extract(payload_json, '$.lease.token') = ?4
          AND datetime(json_extract(payload_json, '$.lease.expiresAt')) > CURRENT_TIMESTAMP
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_DOCUMENT_IMPORT)
    .bind(normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(payload_json)
    .execute(pool)
    .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(inactive_document_import_claim_error())
    }
}

use crate::idempotency::normalize_idempotency_key;

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

fn retained_document_integrity_error() -> AppError {
    AppError::storage(RETAINED_DOCUMENT_INTEGRITY_ERROR)
}

fn canonical_documents_root(
    documents_dir: &Path,
    database_path: &str,
) -> Result<PathBuf, AppError> {
    let metadata =
        std::fs::symlink_metadata(documents_dir).map_err(|_| retained_document_integrity_error())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(retained_document_integrity_error());
    }
    #[cfg(windows)]
    if metadata_is_reparse_point(&metadata) {
        return Err(retained_document_integrity_error());
    }

    resolve_workspace_exports_dir(
        &documents_dir.to_string_lossy(),
        database_path,
    )
    .map_err(|_| retained_document_integrity_error())
}

fn retained_document_object_path(
    documents_dir: &Path,
    database_path: &str,
    object_path: &str,
) -> Result<PathBuf, AppError> {
    let documents_root = canonical_documents_root(documents_dir, database_path)?;
    let root_metadata = std::fs::symlink_metadata(&documents_root)
        .map_err(|_| retained_document_integrity_error())?;
    if !root_metadata.is_dir() {
        return Err(retained_document_integrity_error());
    }

    let relative = Path::new(object_path);
    if object_path.trim().is_empty() || relative.is_absolute() {
        return Err(retained_document_integrity_error());
    }

    let mut current = documents_root.clone();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let std::path::Component::Normal(name) = component else {
            return Err(retained_document_integrity_error());
        };
        current.push(name);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| retained_document_integrity_error())?;
        if metadata.file_type().is_symlink() {
            return Err(retained_document_integrity_error());
        }
        #[cfg(windows)]
        if metadata_is_reparse_point(&metadata) {
            return Err(retained_document_integrity_error());
        }
        if components.peek().is_some() {
            if !metadata.is_dir() {
                return Err(retained_document_integrity_error());
            }
        } else if !metadata.is_file() {
            return Err(retained_document_integrity_error());
        }
    }

    if !current.starts_with(&documents_root) {
        return Err(retained_document_integrity_error());
    }
    Ok(current)
}

fn verify_retained_document_object(
    documents_dir: &Path,
    database_path: &str,
    object_path: &str,
    expected_sha256: &str,
) -> Result<(), AppError> {
    let object_path = retained_document_object_path(documents_dir, database_path, object_path)?;
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&object_path)
            .map_err(|_| retained_document_integrity_error())?
    };
    #[cfg(not(unix))]
    let mut file = std::fs::File::open(&object_path).map_err(|_| retained_document_integrity_error())?;

    let metadata = file
        .metadata()
        .map_err(|_| retained_document_integrity_error())?;
    if !metadata.is_file() {
        return Err(retained_document_integrity_error());
    }
    #[cfg(windows)]
    if metadata_is_reparse_point(&metadata) {
        return Err(retained_document_integrity_error());
    }

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| retained_document_integrity_error())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if format!("{:x}", hasher.finalize()) != expected_sha256 {
        return Err(retained_document_integrity_error());
    }
    Ok(())
}

pub async fn verify_retained_document(
    pool: &SqlitePool,
    workspace_id: &str,
    document_id: &str,
) -> Result<Document, AppError> {
    let document = document_get(pool, workspace_id, document_id).await?;
    let documents_dir = load_documents_dir(pool, workspace_id).await?;
    let database_path: String =
        sqlx::query_scalar("SELECT database_path FROM workspaces WHERE id = ?1 LIMIT 1")
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::validation("Workspace not found", "workspaceId"))?;
    verify_retained_document_object(
        &documents_dir,
        &database_path,
        &document.object_path,
        &document.content_sha256,
    )?;
    Ok(document)
}

pub async fn verify_retained_document_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::validation("Document not found in workspace", "documentId"))?;
    let document = map_document_row(row);
    let (documents_dir, database_path): (String, String) = sqlx::query_as(
        "SELECT documents_path, database_path FROM workspaces WHERE id = ?1 LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::validation("Workspace not found", "workspaceId"))?;
    verify_retained_document_object(
        Path::new(&documents_dir),
        &database_path,
        &document.object_path,
        &document.content_sha256,
    )?;
    Ok(document)
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
          AND status = 'succeeded'
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

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sync_document_object_parent(path: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn verify_content_addressed_object(path: &Path, expected_sha256: &str) -> Result<(), AppError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::storage(
            "Existing document object is not a regular file",
        ));
    }
    if sha256_file(path)? != expected_sha256 {
        return Err(AppError::storage("Retained document integrity check failed"));
    }
    Ok(())
}

fn stage_document_object(
    parent: &Path,
    bytes: &[u8],
    expected_sha256: &str,
) -> Result<tempfile::NamedTempFile, AppError> {
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    if sha256_file(staged.path())? != expected_sha256 {
        return Err(AppError::storage(
            "Staged document object content does not match its hash",
        ));
    }
    Ok(staged)
}

fn stage_and_promote_document_object(
    object_path: &Path,
    bytes: &[u8],
    expected_sha256: &str,
) -> Result<(), AppError> {
    let parent = object_path
        .parent()
        .ok_or_else(|| AppError::storage("Document object path has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    match std::fs::symlink_metadata(object_path) {
        Ok(_) => return verify_content_addressed_object(object_path, expected_sha256),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        Err(_) => {}
    }

    let staged = stage_document_object(parent, bytes, expected_sha256)?;
    match staged.persist_noclobber(object_path) {
        Ok(_) => sync_document_object_parent(parent),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_content_addressed_object(object_path, expected_sha256)
        }
        Err(error) => Err(error.error.into()),
    }
}


fn ensure_content_addressed_object(
    object_path: &Path,
    bytes: &[u8],
    expected_sha256: &str,
) -> Result<(), AppError> {
    match std::fs::symlink_metadata(object_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            stage_and_promote_document_object(object_path, bytes, expected_sha256)
        }
        Err(error) => Err(error.into()),
        Ok(_) => verify_content_addressed_object(object_path, expected_sha256),
    }
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

    const MAX_DOCUMENT_BYTES: u64 = 10 * 1024 * 1024;
    let metadata = std::fs::metadata(source_path)?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(AppError::validation(
            "Source file is too large to import",
            "sourcePath",
        ));
    }

    let bytes = std::fs::read(source_path)?;
    let mime_type = resolve_document_mime(&input.mime_type, &bytes)?;
    let content_sha256 = sha256_hex(&bytes);

    let lease = match claim_document_import(pool, workspace_id, idempotency_key, &content_sha256).await? {
        DocumentImportClaim::Cached(existing) => {
            let documents_dir = load_documents_dir(pool, workspace_id).await?;
            let object_path = safe_join_under(&documents_dir, &existing.object_path, "objectPath")?;
            ensure_content_addressed_object(&object_path, &bytes, &content_sha256)?;
            return Ok(existing);
        }
        DocumentImportClaim::Proceed(lease) => lease,
    };

    let result = async {
        let documents_dir = load_documents_dir(pool, workspace_id).await?;
        let object_rel = format!("objects/{content_sha256}");
        ensure_content_addressed_object(&documents_dir.join(&object_rel), &bytes, &content_sha256)?;
        renew_document_import_lease(pool, workspace_id, idempotency_key, &lease).await?;

        let mut transaction = pool.begin().await?;
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
        .bind(&mime_type)
        .bind(input.filename.trim())
        .bind(retention_years)
        .execute(&mut *transaction)
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
        .fetch_one(&mut *transaction)
        .await?;
        let document = Document {
            id: row.get("id"),
            object_path: row.get("object_path"),
            content_sha256: row.get("content_sha256"),
            mime_type: row.get("mime_type"),
            original_filename: row.get("original_filename"),
            retention_years: row.get("retention_years"),
        };
        record_event_tx(
            &mut *transaction,
            workspace_id,
            "document_import",
            "document",
            Some(&document.id),
            &serde_json::to_string(&document).unwrap_or_else(|_| "{}".to_string()),
        )
        .await?;

        let payload = IdempotentDocumentPayload {
            idempotency_key: idempotency_key.to_string(),
            content_sha256: content_sha256.to_string(),
            document: Some(document.clone()),
            lease: None,
        };
        let payload_json =
            serde_json::to_string(&payload).map_err(|error| AppError::internal(error.to_string()))?;
        let finalized = sqlx::query(
            r#"
            UPDATE local_jobs
            SET status = 'succeeded', payload_json = ?5, last_error = NULL, updated_at = CURRENT_TIMESTAMP
            WHERE workspace_id = ?1
              AND job_type = ?2
              AND idempotency_key = ?3
              AND status = 'running'
              AND json_extract(payload_json, '$.lease.token') = ?4
              AND datetime(json_extract(payload_json, '$.lease.expiresAt')) > CURRENT_TIMESTAMP
            "#,
        )
        .bind(workspace_id)
        .bind(JOB_DOCUMENT_IMPORT)
        .bind(idempotency_key)
        .bind(&lease.token)
        .bind(payload_json)
        .execute(&mut *transaction)
        .await?;
        if finalized.rows_affected() != 1 {
            return Err(inactive_document_import_claim_error());
        }
        transaction.commit().await?;

        Ok(document)
    }
    .await;

    if result.is_err() {
        let _ = fail_document_import_claim(pool, workspace_id, idempotency_key, &lease).await;
    }
    result
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
    const MAX_DOCUMENT_BYTES: u64 = 10 * 1024 * 1024;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(AppError::validation(
            "Document is too large to store",
            "document",
        ));
    }

    let mime_type = resolve_document_mime(mime_type, bytes)?;
    let content_sha256 = sha256_hex(bytes);
    let documents_dir = load_documents_dir(pool, workspace_id).await?;

    let object_rel = format!("objects/{content_sha256}");
    let object_abs = documents_dir.join(&object_rel);
    ensure_content_addressed_object(&object_abs, bytes, &content_sha256)?;

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
    .bind(&mime_type)
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
    expected_sha256: &str,
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

    verify_content_addressed_object(&canonical, expected_sha256)?;

    let extension = {
        let mut header = [0u8; 16];
        let mut file = open_reveal_source(&canonical)?;
        let read = std::io::Read::read(&mut file, &mut header).unwrap_or(0);
        let trusted = resolve_reveal_mime(mime_type, &header[..read])?;
        reveal_extension_for_mime(&trusted)?
    };
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
        temp.as_file_mut().sync_all()?;
        verify_content_addressed_object(temp.path(), expected_sha256)?;
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
    content_sha256: &str,
) -> Result<(), AppError> {
    let full_path = prepare_reveal_path(documents_dir, object_path)?;
    let staged = stage_reveal_copy(&full_path, documents_dir, mime_type, content_sha256)?;
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
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                hwnd: *mut core::ffi::c_void,
                lp_operation: *const u16,
                lp_file: *const u16,
                lp_parameters: *const u16,
                lp_directory: *const u16,
                n_show_cmd: i32,
            ) -> isize;
        }

        const SW_SHOWNORMAL: i32 = 1;
        let operation: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let file: Vec<u16> = full_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // ShellExecuteW returns a value > 32 on success; avoids PowerShell/cmd parsing.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result <= 32 {
            return Err(AppError::internal(REVEAL_FAILED));
        }
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
    let content_sha256 = document.content_sha256.clone();

    tokio::task::spawn_blocking(move || {
        reveal_document_blocking(&documents_dir, &object_path, &mime_type, &content_sha256)
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
        purge_stale_reveal_staging, reveal_extension_for_mime, sha256_hex, stage_reveal_copy,
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

        let staged = stage_reveal_copy(
            &source,
            dir.path(),
            "application/pdf",
            &sha256_hex(b"%PDF-1.3 test"),
        )
        .expect("stage reveal copy");

        assert_eq!(staged.extension().and_then(|ext| ext.to_str()), Some("pdf"));
        assert!(staged
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(REVEAL_STAGING_PREFIX)));
        assert_eq!(fs::read(&staged).expect("read staged"), b"%PDF-1.3 test");
        validate_staged_reveal_path(&staged).expect("staged path is safe to reveal");
        let _ = fs::remove_file(staged);
    }

    #[test]
    fn stage_reveal_copy_accepts_legacy_octet_stream_mime() {
        let dir = tempdir().expect("tempdir");
        let source = dir.path().join("objects").join("legacy");
        fs::create_dir_all(source.parent().expect("parent")).expect("objects dir");
        fs::write(&source, b"%PDF-1.4 legacy").expect("source pdf");

        let staged = stage_reveal_copy(
            &source,
            dir.path(),
            "application/octet-stream",
            &sha256_hex(b"%PDF-1.4 legacy"),
        )
        .expect("legacy octet-stream reveal");
        assert_eq!(staged.extension().and_then(|ext| ext.to_str()), Some("pdf"));
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

        let error = stage_reveal_copy(
            &link,
            dir.path(),
            "application/pdf",
            &sha256_hex(b"%PDF-1.3"),
        )
        .expect_err("symlink source");
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

#[cfg(test)]
mod import_recovery_tests {
    use super::{
        claim_document_import, document_import, document_reveal, fail_document_import_claim,
        finalize_document_import_claim, renew_document_import_lease, sha256_hex, Document,
        DocumentImportClaim, DocumentImportInput, JOB_DOCUMENT_IMPORT, REVEAL_STAGING_PREFIX,
    };
    use crate::db::connect_workspace;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::{tempdir, TempDir};
    use uuid::Uuid;

    async fn setup_workspace() -> (TempDir, String, PathBuf, sqlx::SqlitePool) {
        let dir = tempdir().expect("tempdir");
        let workspace_id = Uuid::new_v4().to_string();
        let data_dir = dir.path().join(&workspace_id);
        let documents_dir = data_dir.join("documents");
        let exports_dir = data_dir.join("exports");
        fs::create_dir_all(&documents_dir).expect("documents");
        fs::create_dir_all(&exports_dir).expect("exports");
        let database_path = data_dir.join("workspace.sqlite");
        let pool = connect_workspace(&database_path).await.expect("database");

        sqlx::query(
            "INSERT INTO workspaces (id, name, database_path, documents_path, exports_path) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&workspace_id)
        .bind("Document recovery")
        .bind(database_path.to_string_lossy().to_string())
        .bind(documents_dir.to_string_lossy().to_string())
        .bind(exports_dir.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("workspace");

        (dir, workspace_id, documents_dir, pool)
    }

    fn staged_reveal_paths() -> BTreeSet<PathBuf> {
        fs::read_dir(std::env::temp_dir())
            .expect("read staging directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(REVEAL_STAGING_PREFIX))
            })
            .collect()
    }

    #[tokio::test]
    async fn replaced_claim_token_cannot_finalize_or_fail_document_import() {
        let (_dir, workspace_id, _documents_dir, pool) = setup_workspace().await;
        let idempotency_key = "replaced-document-import";
        let content_sha256 = sha256_hex(b"\x89PNG\r\n\x1a\nreceipt");
        let DocumentImportClaim::Proceed(original_lease) =
            claim_document_import(&pool, &workspace_id, idempotency_key, &content_sha256)
                .await
                .expect("original claim")
        else {
            panic!("original claim must proceed");
        };

        sqlx::query(
            "UPDATE local_jobs SET payload_json = json_set(payload_json, '$.lease.expiresAt', datetime('now', '-1 second')) WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
        )
        .bind(&workspace_id)
        .bind(JOB_DOCUMENT_IMPORT)
        .bind(idempotency_key)
        .execute(&pool)
        .await
        .expect("expire original lease");

        let DocumentImportClaim::Proceed(reclaimed_lease) =
            claim_document_import(&pool, &workspace_id, idempotency_key, &content_sha256)
                .await
                .expect("reclaimed claim")
        else {
            panic!("expired claim must be reclaimed");
        };
        assert!(
            renew_document_import_lease(
                &pool,
                &workspace_id,
                idempotency_key,
                &original_lease,
            )
            .await
            .is_err(),
            "replaced worker must not renew the claim"
        );
        let document = Document {
            id: Uuid::new_v4().to_string(),
            object_path: format!("objects/{content_sha256}"),
            content_sha256: content_sha256.clone(),
            mime_type: "image/png".to_string(),
            original_filename: "receipt.png".to_string(),
            retention_years: 7,
        };

        assert!(
            finalize_document_import_claim(
                &pool,
                &workspace_id,
                idempotency_key,
                &original_lease,
                &document,
            )
            .await
            .is_err(),
            "replaced worker must not finalize the claim"
        );
        assert!(
            fail_document_import_claim(
                &pool,
                &workspace_id,
                idempotency_key,
                &original_lease,
            )
            .await
            .is_err(),
            "replaced worker must not fail the claim"
        );

        let (status, token): (String, String) = sqlx::query_as(
            "SELECT status, json_extract(payload_json, '$.lease.token') FROM local_jobs WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
        )
        .bind(&workspace_id)
        .bind(JOB_DOCUMENT_IMPORT)
        .bind(idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("reclaimed job");
        assert_eq!(status, "running");
        assert_eq!(token, reclaimed_lease.token);
    }

    #[tokio::test]
    async fn renewed_claim_cannot_be_reclaimed_using_stale_updated_at() {
        let (_dir, workspace_id, _documents_dir, pool) = setup_workspace().await;
        let idempotency_key = "renewed-document-import";
        let content_sha256 = sha256_hex(b"\x89PNG\r\n\x1a\nreceipt");
        let DocumentImportClaim::Proceed(lease) =
            claim_document_import(&pool, &workspace_id, idempotency_key, &content_sha256)
                .await
                .expect("claim")
        else {
            panic!("claim must proceed");
        };

        sqlx::query(
            "UPDATE local_jobs SET updated_at = datetime('now', '-6 minutes') WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
        )
        .bind(&workspace_id)
        .bind(JOB_DOCUMENT_IMPORT)
        .bind(idempotency_key)
        .execute(&pool)
        .await
        .expect("make legacy timestamp stale");
        renew_document_import_lease(&pool, &workspace_id, idempotency_key, &lease)
            .await
            .expect("renew active lease");

        let error = claim_document_import(&pool, &workspace_id, idempotency_key, &content_sha256)
            .await
            .expect_err("renewed lease must remain exclusively owned");
        assert_eq!(error.code, "validation_error");
    }

    #[tokio::test]
    async fn cached_success_restores_content_addressed_evidence_without_new_audit_event() {
        let (_dir, workspace_id, documents_dir, pool) = setup_workspace().await;
        let bytes = b"\x89PNG\r\n\x1a\nreceipt";
        let source_path = documents_dir.parent().expect("data dir").join("receipt.png");
        fs::write(&source_path, bytes).expect("source");
        let input = DocumentImportInput {
            source_path: source_path.to_string_lossy().to_string(),
            filename: "receipt.png".to_string(),
            mime_type: "image/png".to_string(),
            idempotency_key: "cached-document-import".to_string(),
        };
        let imported = document_import(&pool, &workspace_id, &input)
            .await
            .expect("initial import");
        let object_path = documents_dir.join(&imported.object_path);
        fs::remove_file(&object_path).expect("remove stored evidence");

        let cached = document_import(&pool, &workspace_id, &input)
            .await
            .expect("cached import recovers evidence");

        assert_eq!(cached.id, imported.id);
        assert_eq!(fs::read(&object_path).expect("restored evidence"), bytes);
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1 AND action = 'document_import' AND resource_id = ?2",
        )
        .bind(&workspace_id)
        .bind(&imported.id)
        .fetch_one(&pool)
        .await
        .expect("audit event count");
        assert_eq!(audit_count, 1);
    }

    #[tokio::test]
    async fn document_reveal_rejects_hash_mismatched_same_mime_evidence_without_staging() {
        let (_dir, workspace_id, documents_dir, pool) = setup_workspace().await;
        let source_path = documents_dir.parent().expect("data dir").join("receipt.pdf");
        fs::write(&source_path, b"%PDF-1.4 retained").expect("source");
        let input = DocumentImportInput {
            source_path: source_path.to_string_lossy().to_string(),
            filename: "receipt.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            idempotency_key: "tampered-document-reveal".to_string(),
        };
        let imported = document_import(&pool, &workspace_id, &input)
            .await
            .expect("initial import");
        let object_path = documents_dir.join(&imported.object_path);
        fs::write(&object_path, b"%PDF-1.4 tampered").expect("tamper stored evidence");
        let staged_before = staged_reveal_paths();

        let error = document_reveal(&pool, &workspace_id, &imported.id)
            .await
            .expect_err("hash-mismatched evidence must not be revealed");

        assert_eq!(error.code, "storage_error");
        assert_eq!(error.message, "Retained document integrity check failed");
        assert_eq!(staged_reveal_paths(), staged_before);
    }

    #[tokio::test]
    async fn cached_import_rejects_tampered_evidence_without_replacing_or_auditing_it() {
        let (_dir, workspace_id, documents_dir, pool) = setup_workspace().await;
        let bytes = b"\x89PNG\r\n\x1a\nreceipt";
        let source_path = documents_dir.parent().expect("data dir").join("receipt.png");
        fs::write(&source_path, bytes).expect("source");
        let input = DocumentImportInput {
            source_path: source_path.to_string_lossy().to_string(),
            filename: "receipt.png".to_string(),
            mime_type: "image/png".to_string(),
            idempotency_key: "tampered-cached-document-import".to_string(),
        };
        let imported = document_import(&pool, &workspace_id, &input)
            .await
            .expect("initial import");
        let object_path = documents_dir.join(&imported.object_path);
        let tampered_bytes = b"tampered evidence";
        fs::write(&object_path, tampered_bytes).expect("tamper stored evidence");

        let error = document_import(&pool, &workspace_id, &input)
            .await
            .expect_err("cached import must reject tampered evidence");

        assert_eq!(error.code, "storage_error");
        assert_eq!(error.message, "Retained document integrity check failed");
        assert_eq!(fs::read(&object_path).expect("preserved evidence"), tampered_bytes);
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1 AND action = 'document_import'",
        )
        .bind(&workspace_id)
        .fetch_one(&pool)
        .await
        .expect("audit event count");
        assert_eq!(audit_count, 1);
    }

    #[tokio::test]
    async fn ordinary_import_rejects_tampered_evidence_without_replacing_or_auditing_it() {
        let (_dir, workspace_id, documents_dir, pool) = setup_workspace().await;
        let bytes = b"\x89PNG\r\n\x1a\nreceipt";
        let source_path = documents_dir.parent().expect("data dir").join("receipt.png");
        fs::write(&source_path, bytes).expect("source");
        let initial_input = DocumentImportInput {
            source_path: source_path.to_string_lossy().to_string(),
            filename: "receipt.png".to_string(),
            mime_type: "image/png".to_string(),
            idempotency_key: "tampered-initial-document-import".to_string(),
        };
        let imported = document_import(&pool, &workspace_id, &initial_input)
            .await
            .expect("initial import");
        let object_path = documents_dir.join(&imported.object_path);
        let tampered_bytes = b"tampered evidence";
        fs::write(&object_path, tampered_bytes).expect("tamper stored evidence");
        let replay_input = DocumentImportInput {
            idempotency_key: "tampered-ordinary-document-import".to_string(),
            ..initial_input
        };

        let error = document_import(&pool, &workspace_id, &replay_input)
            .await
            .expect_err("ordinary import must reject tampered evidence");

        assert_eq!(error.code, "storage_error");
        assert_eq!(error.message, "Retained document integrity check failed");
        assert_eq!(fs::read(&object_path).expect("preserved evidence"), tampered_bytes);
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1 AND action = 'document_import'",
        )
        .bind(&workspace_id)
        .fetch_one(&pool)
        .await
        .expect("audit event count");
        assert_eq!(audit_count, 1);
        let status: String = sqlx::query_scalar(
            "SELECT status FROM local_jobs WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
        )
        .bind(&workspace_id)
        .bind(JOB_DOCUMENT_IMPORT)
        .bind(&replay_input.idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("failed import claim");
        assert_eq!(status, "failed");
    }

    #[tokio::test]
    async fn failed_and_stale_import_claims_are_reclaimed_when_evidence_is_missing() {
        let dir = tempdir().expect("tempdir");
        let workspace_id = Uuid::new_v4().to_string();
        let data_dir = dir.path().join(&workspace_id);
        let documents_dir = data_dir.join("documents");
        let exports_dir = data_dir.join("exports");
        fs::create_dir_all(&documents_dir).expect("documents");
        fs::create_dir_all(&exports_dir).expect("exports");
        let database_path = data_dir.join("workspace.sqlite");
        let pool = connect_workspace(&database_path).await.expect("database");

        sqlx::query(
            "INSERT INTO workspaces (id, name, database_path, documents_path, exports_path) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&workspace_id)
        .bind("Document recovery")
        .bind(database_path.to_string_lossy().to_string())
        .bind(documents_dir.to_string_lossy().to_string())
        .bind(exports_dir.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("workspace");

        let bytes = b"\x89PNG\r\n\x1a\nreceipt";
        let source_path = data_dir.join("receipt.png");
        fs::write(&source_path, bytes).expect("source");
        let content_sha256 = sha256_hex(bytes);
        let idempotency_key = "interrupted-document-import";
        let payload = serde_json::json!({
            "idempotencyKey": idempotency_key,
            "contentSha256": content_sha256,
            "document": null,
        });
        sqlx::query(
            "INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key) VALUES (?1, ?2, 'document_import', 'failed', ?3, ?4)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&workspace_id)
        .bind(payload.to_string())
        .bind(idempotency_key)
        .execute(&pool)
        .await
        .expect("failed claim");
        let object_path = documents_dir.join("objects").join(&content_sha256);

        let input = DocumentImportInput {
            source_path: source_path.to_string_lossy().to_string(),
            filename: "receipt.png".to_string(),
            mime_type: "image/png".to_string(),
            idempotency_key: idempotency_key.to_string(),
        };
        let imported = document_import(&pool, &workspace_id, &input)
            .await
            .expect("reclaim failed import and store evidence");

        assert_eq!(fs::read(object_path).expect("stored object"), bytes);
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1 AND action = 'document_import' AND resource_id = ?2",
        )
        .bind(&workspace_id)
        .bind(&imported.id)
        .fetch_one(&pool)
        .await
        .expect("audit event");
        assert_eq!(audit_count, 1);
        assert_eq!(imported.content_sha256, content_sha256);

        let stale_key = "stale-document-import";
        let stale_payload = serde_json::json!({
            "idempotencyKey": stale_key,
            "contentSha256": content_sha256,
            "document": null,
        });
        sqlx::query(
            "INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key, updated_at) VALUES (?1, ?2, 'document_import', 'running', ?3, ?4, datetime('now', '-6 minutes'))",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&workspace_id)
        .bind(stale_payload.to_string())
        .bind(stale_key)
        .execute(&pool)
        .await
        .expect("stale claim");

        let recovered = document_import(
            &pool,
            &workspace_id,
            &DocumentImportInput {
                source_path: source_path.to_string_lossy().to_string(),
                filename: "receipt.png".to_string(),
                mime_type: "image/png".to_string(),
                idempotency_key: stale_key.to_string(),
            },
        )
        .await
        .expect("reclaim stale import");
        assert_eq!(recovered.id, imported.id);
    }
}
