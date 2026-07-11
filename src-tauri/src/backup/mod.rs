use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{audit::record_event, db::connect_workspace, error::AppError};

mod crypto;

pub use crypto::{backup_plaintext_is_sqlite, is_encrypted_backup_file, BACKUP_FILE_EXTENSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentBackupPayload {
    idempotency_key: String,
    summary: BackupSummary,
}

const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifestEntry {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
    pub entry_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub version: u32,
    pub workspace_id: String,
    pub workspace_name: String,
    pub created_at: String,
    pub entries: Vec<BackupManifestEntry>,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub backup_path: String,
    pub manifest: BackupManifest,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupCreateInput {
    pub idempotency_key: String,
    pub destination_path: Option<String>,
    pub backup_file_path: Option<String>,
    pub passphrase: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreInput {
    pub backup_path: String,
    pub confirm_overwrite: bool,
    pub passphrase: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreSummary {
    pub database_path: String,
    pub workspace_id: String,
    pub workspace_name: String,
}

pub fn hash_file(path: &Path) -> Result<(String, u64), AppError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut bytes = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes += read as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), bytes))
}

fn hash_string(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), AppError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(AppError::validation(
                "Backup restore rejected symlink entry",
                "backupPath",
            ));
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn copy_exports_excluding_backups(source: &Path, destination: &Path) -> Result<(), AppError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "backups" || name.starts_with("backup-") {
            continue;
        }
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn collect_backup_entries(backup_root: &Path) -> Result<Vec<BackupManifestEntry>, AppError> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(backup_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path == backup_root {
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("manifest.json") {
            continue;
        }
        let relative = path
            .strip_prefix(backup_root)
            .map_err(|_| AppError::storage("Invalid backup path"))?
            .to_string_lossy()
            .replace('\\', "/");

        if path.is_dir() {
            entries.push(BackupManifestEntry {
                relative_path: format!("{relative}/"),
                sha256: String::new(),
                bytes: 0,
                entry_type: "directory".to_string(),
            });
            continue;
        }

        let (sha256, bytes) = hash_file(path)?;
        entries.push(BackupManifestEntry {
            relative_path: relative,
            sha256,
            bytes,
            entry_type: "file".to_string(),
        });
    }
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(entries)
}

fn safe_backup_join(backup_root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    if relative.contains("..") {
        return Err(AppError::validation(
            "Manifest entry path escapes backup root",
            "backupPath",
        ));
    }
    Ok(backup_root.join(relative))
}

pub async fn create_backup_package(
    pool: &SqlitePool,
    workspace_id: &str,
    data_dir: &Path,
    _database_path: &Path,
    destination_root: &Path,
    passphrase: &str,
    backup_file_path: Option<&str>,
) -> Result<BackupSummary, AppError> {
    crypto::validate_passphrase(passphrase)?;
    let row = sqlx::query(
        r#"
        SELECT name FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::validation("Workspace not found", "workspace"))?;

    let workspace_name: String = row.get("name");

    let rule_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM rule_versions
        "#,
    )
    .fetch_one(pool)
    .await?;
    if rule_count == 0 {
        return Err(AppError::validation(
            "Backup requires active rule versions in workspace database",
            "ruleVersions",
        ));
    }

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let suffix = Uuid::new_v4().simple().to_string();
    let backup_dir = destination_root
        .join("backups")
        .join(format!("staging-{timestamp}-{suffix}"));
    let backup_file = if let Some(path) = backup_file_path {
        crate::paths::validate_backup_file_path(path)?
    } else {
        destination_root.join("backups").join(format!(
            "backup-{timestamp}-{suffix}.{}",
            crypto::BACKUP_FILE_EXTENSION
        ))
    };
    if backup_dir.exists() || backup_file.exists() {
        return Err(AppError::storage("Backup path already exists"));
    }
    fs::create_dir_all(&backup_dir)?;

    let backup_result = async {
        let db_target = backup_dir.join("workspace.sqlite");
        crate::db::wal_checkpoint_truncate(pool).await?;
        crate::db::vacuum_database_into(pool, &db_target).await?;

        let documents_source = data_dir.join("documents");
        let documents_target = backup_dir.join("documents");
        if documents_source.exists() {
            copy_dir_all(&documents_source, &documents_target)?;
        } else {
            fs::create_dir_all(&documents_target)?;
        }

        let exports_source = data_dir.join("exports");
        let exports_target = backup_dir.join("exports");
        if exports_source.exists() {
            copy_exports_excluding_backups(&exports_source, &exports_target)?;
        } else {
            fs::create_dir_all(&exports_target)?;
        }

        let mut entries = collect_backup_entries(&backup_dir)?;
        entries.push(BackupManifestEntry {
            relative_path: "rule_versions/".to_string(),
            sha256: String::new(),
            bytes: rule_count as u64,
            entry_type: "database_table".to_string(),
        });

        let created_at = Utc::now().to_rfc3339();
        let manifest_body = serde_json::json!({
            "version": MANIFEST_VERSION,
            "workspaceId": workspace_id,
            "workspaceName": workspace_name,
            "createdAt": created_at,
            "entries": entries,
        });
        let manifest_raw = serde_json::to_string(&manifest_body)
            .map_err(|error| AppError::internal(error.to_string()))?;
        let manifest_sha256 = hash_string(&manifest_raw);
        let manifest = BackupManifest {
            version: MANIFEST_VERSION,
            workspace_id: workspace_id.to_string(),
            workspace_name: workspace_name.clone(),
            created_at,
            entries: serde_json::from_value(manifest_body["entries"].clone())
                .map_err(|error| AppError::internal(error.to_string()))?,
            manifest_sha256: manifest_sha256.clone(),
        };

        let manifest_with_hash = serde_json::json!({
            "version": manifest.version,
            "workspaceId": manifest.workspace_id,
            "workspaceName": manifest.workspace_name,
            "createdAt": manifest.created_at,
            "entries": manifest.entries,
            "manifestSha256": manifest.manifest_sha256,
        });
        let manifest_path = backup_dir.join("manifest.json");
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest_with_hash)
                .map_err(|error| AppError::internal(error.to_string()))?,
        )?;

        let tar_bytes = tokio::task::spawn_blocking({
            let backup_dir = backup_dir.clone();
            move || crypto::create_tar_archive(&backup_dir)
        })
        .await
        .map_err(|error| AppError::internal(error.to_string()))??;
        let passphrase_owned = passphrase.to_string();
        let encrypted = tokio::task::spawn_blocking(move || crypto::encrypt_bytes(&passphrase_owned, &tar_bytes))
            .await
            .map_err(|error| AppError::internal(error.to_string()))??;
        fs::write(&backup_file, encrypted)?;
        fs::remove_dir_all(&backup_dir)?;

        record_event(
            pool,
            workspace_id,
            "workspace_backup_create",
            "backup",
            Some(&backup_file.to_string_lossy()),
            &serde_json::to_string(&manifest).unwrap_or_else(|_| "{}".to_string()),
        )
        .await?;

        Ok(BackupSummary {
            backup_path: backup_file.to_string_lossy().to_string(),
            manifest,
        })
    }
    .await;

    if backup_result.is_err() {
        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }
        if backup_file.exists() {
            let _ = fs::remove_file(&backup_file);
        }
    }

    backup_result
}

pub async fn restore_backup_package(
    input: &BackupRestoreInput,
    workspaces_root: &Path,
) -> Result<BackupRestoreSummary, AppError> {
    if !input.confirm_overwrite {
        return Err(AppError::validation(
            "Restore requires explicit confirmation (confirmOverwrite: true)",
            "confirmOverwrite",
        ));
    }

    let backup_path = PathBuf::from(input.backup_path.trim());
    if !backup_path.exists() {
        return Err(AppError::validation("Backup path not found", "backupPath"));
    }

    let temp_dir = tempfile::tempdir().map_err(|error| AppError::storage(error.to_string()))?;
    let backup_root = if crypto::is_encrypted_backup_file(&backup_path) {
        let metadata = fs::metadata(&backup_path)?;
        if metadata.len() > crypto::MAX_ENCRYPTED_BACKUP_BYTES {
            return Err(AppError::validation(
                "Backup file exceeds maximum allowed size",
                "backupPath",
            ));
        }
        let encrypted = fs::read(&backup_path)?;
        let tar_bytes = crypto::decrypt_bytes(&input.passphrase, &encrypted)?;
        crypto::extract_tar_archive(&tar_bytes, temp_dir.path())?;
        temp_dir.path().to_path_buf()
    } else if backup_path.is_dir() {
        return Err(AppError::validation(
            "Directory backups are not supported for restore; use an encrypted .skatbackup file",
            "backupPath",
        ));
    } else {
        return Err(AppError::validation("Unsupported backup format", "backupPath"));
    };

    restore_from_staged_backup(&backup_root, workspaces_root).await
}

async fn restore_from_staged_backup(
    backup_path: &Path,
    workspaces_root: &Path,
) -> Result<BackupRestoreSummary, AppError> {

    let manifest_path = backup_path.join("manifest.json");
    if !manifest_path.exists() {
        return Err(AppError::validation("Backup manifest missing", "backupPath"));
    }

    let manifest_raw = fs::read_to_string(&manifest_path)?;
    let manifest_value: serde_json::Value = serde_json::from_str(&manifest_raw)
        .map_err(|error| AppError::validation(error.to_string(), "backupPath"))?;
    let expected_hash = manifest_value["manifestSha256"]
        .as_str()
        .ok_or_else(|| AppError::validation("Manifest hash missing", "backupPath"))?;

    let body_for_hash = serde_json::json!({
        "version": manifest_value["version"],
        "workspaceId": manifest_value["workspaceId"],
        "workspaceName": manifest_value["workspaceName"],
        "createdAt": manifest_value["createdAt"],
        "entries": manifest_value["entries"],
    });
    let computed_hash = hash_string(
        &serde_json::to_string(&body_for_hash)
            .map_err(|error| AppError::internal(error.to_string()))?,
    );
    if computed_hash != expected_hash {
        return Err(AppError::validation("Backup manifest hash mismatch", "backupPath"));
    }

    for entry in manifest_value["entries"]
        .as_array()
        .ok_or_else(|| AppError::validation("Invalid manifest entries", "backupPath"))?
    {
        if entry["entryType"].as_str() != Some("file") {
            continue;
        }
        let relative = entry["relativePath"]
            .as_str()
            .ok_or_else(|| AppError::validation("Invalid manifest entry", "backupPath"))?;
        let file_path = safe_backup_join(&backup_path, relative)?;
        let (actual_hash, actual_bytes) = hash_file(&file_path)?;
        if actual_hash != entry["sha256"].as_str().unwrap_or_default()
            || actual_bytes != entry["bytes"].as_u64().unwrap_or(0)
        {
            return Err(AppError::validation(
                format!("Backup file hash mismatch for {relative}"),
                "backupPath",
            ));
        }
    }

    let workspace_id = manifest_value["workspaceId"]
        .as_str()
        .ok_or_else(|| AppError::validation("Workspace id missing in manifest", "backupPath"))?
        .to_string();
    let workspace_name = manifest_value["workspaceName"]
        .as_str()
        .unwrap_or("Restored workspace")
        .to_string();

    let restore_workspace_id = Uuid::new_v4().to_string();
    let workspace_dir = workspaces_root.join(&restore_workspace_id);
    if workspace_dir.exists() {
        return Err(AppError::storage("Restore target workspace directory already exists"));
    }
    fs::create_dir_all(&workspace_dir)?;

    let documents_path = workspace_dir.join("documents");
    let exports_path = workspace_dir.join("exports");
    fs::create_dir_all(&documents_path)?;
    fs::create_dir_all(&exports_path)?;

    let target_database_path = workspace_dir.join("workspace.sqlite");
    fs::copy(backup_path.join("workspace.sqlite"), &target_database_path)?;

    let pool = connect_workspace(&target_database_path).await?;

    if backup_path.join("documents").exists() {
        copy_dir_all(&backup_path.join("documents"), &documents_path)?;
    }

    if backup_path.join("exports").exists() {
        copy_dir_all(&backup_path.join("exports"), &exports_path)?;
    }

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(id) DO UPDATE SET
          name = excluded.name,
          database_path = excluded.database_path,
          documents_path = excluded.documents_path,
          exports_path = excluded.exports_path,
          updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(&workspace_id)
    .bind(&workspace_name)
    .bind(target_database_path.to_string_lossy().to_string())
    .bind(documents_path.to_string_lossy().to_string())
    .bind(exports_path.to_string_lossy().to_string())
    .execute(&pool)
    .await?;

    record_event(
        &pool,
        &workspace_id,
        "workspace_backup_restore",
        "backup",
        Some(&backup_path.to_string_lossy()),
        &serde_json::json!({ "confirmOverwrite": true }).to_string(),
    )
    .await?;

    Ok(BackupRestoreSummary {
        database_path: target_database_path.to_string_lossy().to_string(),
        workspace_id,
        workspace_name,
    })
}

pub async fn profiles_preserved_after_restore(pool: &SqlitePool, workspace_id: &str) -> Result<bool, AppError> {
    let tax_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM tax_profiles WHERE workspace_id = ?1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let vat_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM vat_profiles WHERE workspace_id = ?1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let rule_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM rule_versions
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(tax_count > 0 && vat_count > 0 && rule_count > 0)
}

pub fn idempotent_backup_matches_request(
    summary: &BackupSummary,
    backup_file_path: Option<&str>,
) -> bool {
    match backup_file_path {
        Some(requested) => PathBuf::from(requested) == PathBuf::from(&summary.backup_path),
        None => true,
    }
}

pub async fn check_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
) -> Result<Option<BackupSummary>, AppError> {
    let key = idempotency_key.trim();
    if key.is_empty() {
        return Err(AppError::validation("Idempotency key is required", "idempotencyKey"));
    }

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

    let Some(payload) = existing else {
        return Ok(None);
    };

    let parsed: IdempotentBackupPayload = serde_json::from_str(&payload)
        .map_err(|error| AppError::internal(error.to_string()))?;
    if parsed.summary.backup_path.is_empty() {
        return Ok(None);
    }
    Ok(Some(parsed.summary))
}

#[derive(Debug)]
pub enum BackupCreateClaim {
    Proceed,
    Cached(BackupSummary),
}

pub async fn claim_backup_create(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
) -> Result<BackupCreateClaim, AppError> {
    if let Some(summary) = check_idempotency(pool, workspace_id, idempotency_key, job_type).await? {
        return Ok(BackupCreateClaim::Cached(summary));
    }

    let key = idempotency_key.trim();
    let pending = IdempotentBackupPayload {
        idempotency_key: key.to_string(),
        summary: BackupSummary {
            backup_path: String::new(),
            manifest: BackupManifest {
                version: 0,
                workspace_id: workspace_id.to_string(),
                workspace_name: String::new(),
                created_at: String::new(),
                entries: vec![],
                manifest_sha256: String::new(),
            },
        },
    };
    let payload_json = serde_json::to_string(&pending)
        .map_err(|error| AppError::internal(error.to_string()))?;

    let id = Uuid::new_v4().to_string();
    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'running', ?4, ?5)
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(job_type)
    .bind(&payload_json)
    .bind(key)
    .execute(pool)
    .await
    {
        Ok(_) => Ok(BackupCreateClaim::Proceed),
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            if let Some(summary) =
                check_idempotency(pool, workspace_id, idempotency_key, job_type).await?
            {
                Ok(BackupCreateClaim::Cached(summary))
            } else {
                Err(AppError::validation(
                    "Backup already in progress for this idempotency key",
                    "idempotencyKey",
                ))
            }
        }
        Err(error) => Err(error.into()),
    }
}

pub async fn finalize_backup_create(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    summary: &BackupSummary,
) -> Result<(), AppError> {
    let payload = IdempotentBackupPayload {
        idempotency_key: idempotency_key.trim().to_string(),
        summary: summary.clone(),
    };
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| AppError::internal(error.to_string()))?;

    sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = 'succeeded', payload_json = ?4
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        "#,
    )
    .bind(workspace_id)
    .bind(job_type)
    .bind(idempotency_key.trim())
    .bind(payload_json)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn record_idempotent_job(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    summary: &BackupSummary,
) -> Result<(), AppError> {
    let payload = IdempotentBackupPayload {
        idempotency_key: idempotency_key.trim().to_string(),
        summary: summary.clone(),
    };
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| AppError::internal(error.to_string()))?;

    let id = Uuid::new_v4().to_string();
    let key = idempotency_key.trim();
    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(job_type)
    .bind(&payload_json)
    .bind(key)
    .execute(pool)
    .await
    {
        Ok(_) => Ok(()),
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
