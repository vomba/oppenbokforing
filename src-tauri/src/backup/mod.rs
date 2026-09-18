use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{
    audit::{record_event, record_event_tx},
    db::{connect_workspace, open_existing_workspace},
    error::{redacted_internal_from, redacted_storage_from, AppError},
};

mod crypto;

pub use crypto::{backup_plaintext_is_sqlite, is_encrypted_backup_file, BACKUP_FILE_EXTENSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupClaimLeasePayload {
    token: String,
    expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentBackupPayload {
    idempotency_key: String,
    summary: BackupSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    staging_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    staging_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lease: Option<BackupClaimLeasePayload>,
}

const MANIFEST_VERSION: u32 = 1;
const BACKUP_CLAIM_LEASE_DURATION: &str = "+5 minutes";
const LEGACY_STALE_IDEMPOTENCY_CLAIM_AFTER: &str = "-5 minutes";
const BACKUP_CLAIM_FAILURE_REASON: &str = "Backup creation failed";
const BACKUP_CREATE_JOB_TYPE: &str = "workspace_backup_create";
const PRIVATE_STAGING_PREFIX: &str = "oppenbokforing-backup-";
const PRIVATE_STAGED_FILE_NAME: &str = "package.skatbackup";

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

fn validate_directory_source(path: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() || !file_type.is_dir() {
        return Err(AppError::validation(
            "Backup rejected symlink or special source directory",
            "backupPath",
        ));
    }
    Ok(())
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), AppError> {
    validate_directory_source(source)?;
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || (!file_type.is_dir() && !file_type.is_file()) {
            return Err(AppError::validation(
                "Backup rejected symlink or special source file",
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
    validate_directory_source(source)?;
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || (!file_type.is_dir() && !file_type.is_file()) {
            return Err(AppError::validation(
                "Backup rejected symlink or special source file",
                "backupPath",
            ));
        }
        let name = entry.file_name();
        let is_backup_artifact = {
            let name = name.to_string_lossy();
            name == "backups" || name.starts_with("backup-")
        };
        if is_backup_artifact {
            continue;
        }
        let target = destination.join(&name);
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn collect_backup_entries(backup_root: &Path) -> Result<Vec<BackupManifestEntry>, AppError> {
    validate_directory_source(backup_root)?;
    let mut entries = Vec::new();
    for entry in WalkDir::new(backup_root) {
        let entry = entry.map_err(redacted_storage_from)?;
        let path = entry.path();
        if path == backup_root || path == backup_root.join("manifest.json") {
            continue;
        }
        let file_type = entry.file_type();
        if file_type.is_symlink() || (!file_type.is_dir() && !file_type.is_file()) {
            return Err(AppError::validation(
                "Backup rejected symlink or special staged file",
                "backupPath",
            ));
        }
        let relative = path
            .strip_prefix(backup_root)
            .map_err(|_| AppError::storage("Invalid backup path"))?
            .to_string_lossy()
            .replace('\\', "/");

        if file_type.is_dir() {
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

#[derive(Debug, Clone)]
pub struct StagedBackupPackage {
    pub summary: BackupSummary,
    pub staging_path: PathBuf,
}

pub fn resolve_backup_file_path(
    destination_root: &Path,
    backup_file_path: Option<&str>,
) -> Result<PathBuf, AppError> {
    if let Some(path) = backup_file_path {
        return crate::paths::validate_backup_file_path(path);
    }

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let suffix = Uuid::new_v4().simple();
    Ok(destination_root.join("backups").join(format!(
        "backup-{timestamp}-{suffix}.{}",
        crypto::BACKUP_FILE_EXTENSION
    )))
}

fn canonicalize_backup_path(path: &Path) -> Result<PathBuf, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::validation("Backup path has no parent directory", "backupPath"))?
        .canonicalize()?;
    let name = path
        .file_name()
        .ok_or_else(|| AppError::validation("Backup path has no file name", "backupPath"))?;
    Ok(parent.join(name))
}

fn reject_backup_destination_overlap(data_dir: &Path, final_path: &Path) -> Result<(), AppError> {
    validate_directory_source(data_dir)?;
    let workspace_root = data_dir.canonicalize()?;
    let final_path = canonicalize_backup_path(final_path)?;
    for source in [
        workspace_root.clone(),
        workspace_root.join("documents"),
        workspace_root.join("exports"),
    ] {
        let source = if source.exists() {
            source.canonicalize()?
        } else {
            source
        };
        if final_path.starts_with(&source) || source.starts_with(&final_path) {
            return Err(AppError::validation(
                "Backup destination must be outside the workspace sources",
                "backupPath",
            ));
        }
    }
    Ok(())
}

fn private_staging_root_for(lease: &BackupCreateLease) -> Result<PathBuf, AppError> {
    let staging_root = tempfile::Builder::new()
        .prefix(&format!("{PRIVATE_STAGING_PREFIX}{}-", lease.token))
        .tempdir()
        .map_err(redacted_storage_from)?
        .keep();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&staging_root, fs::Permissions::from_mode(0o700))?;
    }
    Ok(staging_root)
}

fn staging_token_from_root(staging_root: &Path) -> Option<&str> {
    let name = staging_root.file_name()?.to_str()?;
    let staging_name = name.strip_prefix(PRIVATE_STAGING_PREFIX)?;
    let token = staging_name.get(..36)?;
    (Uuid::parse_str(token).is_ok() && staging_name[token.len()..].starts_with('-')).then_some(token)
}

fn private_staging_path_has_safe_shape(staging_path: &Path) -> bool {
    if staging_path.file_name().and_then(|name| name.to_str()) != Some(PRIVATE_STAGED_FILE_NAME) {
        return false;
    }
    let Some(staging_root) = staging_path.parent() else {
        return false;
    };
    if staging_token_from_root(staging_root).is_none() {
        return false;
    }
    let Ok(temp_root) = std::env::temp_dir().canonicalize() else {
        return false;
    };
    staging_root
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .as_deref()
        == Some(temp_root.as_path())
}

fn metadata_is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        return metadata.file_attributes() & 0x0400 != 0;
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn private_staging_path_is_safe(staging_path: &Path) -> bool {
    if !private_staging_path_has_safe_shape(staging_path) {
        return false;
    }
    let Some(staging_root) = staging_path.parent() else {
        return false;
    };
    let Ok(metadata) = fs::symlink_metadata(staging_root) else {
        return false;
    };
    if metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return false;
        }
    }
    true
}

fn private_staging_path_for(lease: &BackupCreateLease) -> Result<PathBuf, AppError> {
    Ok(private_staging_root_for(lease)?.join(PRIVATE_STAGED_FILE_NAME))
}

fn publication_path_for(final_path: &Path, staging_path: &Path) -> Result<PathBuf, AppError> {
    let token = staging_token_from_root(
        staging_path
            .parent()
            .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?,
    )
    .ok_or_else(|| AppError::storage("Invalid backup staging path"))?;
    let parent = final_path
        .parent()
        .ok_or_else(|| AppError::storage("Backup path has no parent directory"))?;
    let file_name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::storage("Backup path has no file name"))?;
    Ok(parent.join(format!(".{file_name}.{token}.publish")))
}

fn remove_private_staging(staging_path: &Path) -> Result<(), AppError> {
    if !private_staging_path_is_safe(staging_path) {
        return Err(AppError::storage("Backup staging path is not privately owned"));
    }
    let staging_root = staging_path
        .parent()
        .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?;
    match fs::symlink_metadata(staging_path) {
        Ok(metadata) if metadata.is_file() && !metadata_is_link_or_reparse_point(&metadata) => {
            fs::remove_file(staging_path)?
        }
        Ok(_) => return Err(AppError::storage("Backup staging artifact is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    sync_parent_directory(staging_path)?;
    fs::remove_dir(staging_root)?;
    sync_parent_directory(staging_root)
}

fn remove_private_staging_tree(staging_path: &Path) -> Result<(), AppError> {
    if !private_staging_path_is_safe(staging_path) {
        return Err(AppError::storage("Backup staging path is not privately owned"));
    }
    let staging_root = staging_path
        .parent()
        .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?;
    remove_private_build_directory(staging_root)?;
    sync_parent_directory(staging_root)
}

fn remove_private_build_directory(path: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)?;
    let file_type = metadata.file_type();
    if metadata_is_link_or_reparse_point(&metadata) || !file_type.is_dir() {
        return Err(AppError::storage("Backup build directory is not privately owned"));
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path)?;
        let file_type = metadata.file_type();
        if metadata_is_link_or_reparse_point(&metadata)
            || (!file_type.is_dir() && !file_type.is_file())
        {
            return Err(AppError::storage("Backup build directory contains an unsafe file"));
        }
        if file_type.is_dir() {
            remove_private_build_directory(&entry_path)?;
        } else {
            fs::remove_file(entry_path)?;
        }
    }
    fs::remove_dir(path)?;
    Ok(())
}

fn remove_publication_path(final_path: &Path, staging_path: &Path) -> Result<(), AppError> {
    let publication_path = publication_path_for(final_path, staging_path)?;
    match fs::symlink_metadata(&publication_path) {
        Ok(metadata) if metadata.file_type().is_file() => fs::remove_file(&publication_path)?,
        Ok(_) => return Err(AppError::storage("Backup publication path is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    sync_parent_directory(&publication_path)
}

fn sync_file(path: &Path) -> Result<(), AppError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn sync_parent_directory(path: &Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::storage("Backup path has no parent directory"))?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn write_staged_backup(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    sync_parent_directory(path)
}

fn verify_backup_artifact(
    path: &Path,
    passphrase: &str,
    expected_manifest: &BackupManifest,
) -> Result<(), AppError> {
    let encrypted = fs::read(path)?;
    let tar_bytes = crypto::decrypt_bytes(passphrase, &encrypted)?;
    let verification_dir = tempfile::tempdir().map_err(redacted_storage_from)?;
    crypto::extract_tar_archive(&tar_bytes, verification_dir.path())?;
    let manifest_path = verification_dir.path().join("manifest.json");
    let manifest_raw = fs::read_to_string(&manifest_path)?;
    let manifest_value: serde_json::Value = serde_json::from_str(&manifest_raw)
        .map_err(|_| AppError::validation("Invalid backup manifest", "backupPath"))?;
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
        &serde_json::to_string(&body_for_hash).map_err(redacted_internal_from)?,
    );
    if computed_hash != expected_hash || expected_hash != expected_manifest.manifest_sha256 {
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
        let file_path = safe_backup_join(verification_dir.path(), relative)?;
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
    Ok(())
}
async fn stage_backup_package_at_path(
    pool: &SqlitePool,
    workspace_id: &str,
    data_dir: &Path,
    _database_path: &Path,
    final_path: &Path,
    passphrase: &str,
    lease: &BackupCreateLease,
    staging_path: PathBuf,
) -> Result<StagedBackupPackage, AppError> {
    crypto::validate_passphrase(passphrase)?;
    let final_path = crate::paths::validate_backup_file_path(&final_path.to_string_lossy())?;
    reject_backup_destination_overlap(data_dir, &final_path)?;
    if final_path.exists() {
        return Err(AppError::storage("Backup path already exists"));
    }
    let row = sqlx::query("SELECT name FROM workspaces WHERE id = ?1 LIMIT 1")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::validation("Workspace not found", "workspace"))?;
    let workspace_name: String = row.get("name");
    let rule_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rule_versions")
        .fetch_one(pool)
        .await?;
    if rule_count == 0 {
        return Err(AppError::validation(
            "Backup requires active rule versions in workspace database",
            "ruleVersions",
        ));
    }
    if !private_staging_path_is_safe(&staging_path)
        || staging_token_from_root(
            staging_path
                .parent()
                .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?,
        ) != Some(lease.token.as_str())
    {
        return Err(AppError::storage("Backup staging path is not owned by this claim"));
    }
    let staging_root = staging_path
        .parent()
        .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?;
    let build_dir = staging_root.join("build");
    if let Err(error) = fs::create_dir(&build_dir) {
        let _ = remove_private_staging_tree(&staging_path);
        return Err(error.into());
    }

    let stage_result = async {
        let db_target = build_dir.join("workspace.sqlite");
        crate::db::wal_checkpoint_truncate(pool).await?;
        crate::db::vacuum_database_into(pool, &db_target).await?;
        let documents_source = data_dir.join("documents");
        let documents_target = build_dir.join("documents");
        match fs::symlink_metadata(&documents_source) {
            Ok(_) => copy_dir_all(&documents_source, &documents_target)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(&documents_target)?;
            }
            Err(error) => return Err(error.into()),
        }
        let exports_source = data_dir.join("exports");
        let exports_target = build_dir.join("exports");
        match fs::symlink_metadata(&exports_source) {
            Ok(_) => copy_exports_excluding_backups(&exports_source, &exports_target)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(&exports_target)?;
            }
            Err(error) => return Err(error.into()),
        }
        let mut entries = collect_backup_entries(&build_dir)?;
        entries.push(BackupManifestEntry {
            relative_path: "rule_versions/".to_string(),
            sha256: String::new(),
            bytes: rule_count as u64,
            entry_type: "database_table".to_string(),
        });
        entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let created_at = Utc::now().to_rfc3339();
        let manifest_body = serde_json::json!({
            "version": MANIFEST_VERSION,
            "workspaceId": workspace_id,
            "workspaceName": workspace_name,
            "createdAt": created_at,
            "entries": entries,
        });
        let manifest_sha256 = hash_string(
            &serde_json::to_string(&manifest_body).map_err(redacted_internal_from)?,
        );
        let manifest = BackupManifest {
            version: MANIFEST_VERSION,
            workspace_id: workspace_id.to_string(),
            workspace_name: workspace_name.clone(),
            created_at,
            entries: serde_json::from_value(manifest_body["entries"].clone())
                .map_err(redacted_internal_from)?,
            manifest_sha256,
        };
        let manifest_with_hash = serde_json::json!({
            "version": manifest.version,
            "workspaceId": manifest.workspace_id,
            "workspaceName": manifest.workspace_name,
            "createdAt": manifest.created_at,
            "entries": manifest.entries,
            "manifestSha256": manifest.manifest_sha256,
        });
        fs::write(
            build_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest_with_hash).map_err(redacted_internal_from)?,
        )?;
        let tar_bytes = tokio::task::spawn_blocking({
            let build_dir = build_dir.clone();
            move || crypto::create_tar_archive(&build_dir)
        })
        .await
        .map_err(redacted_internal_from)??;
        let passphrase_owned = passphrase.to_string();
        let encrypted = tokio::task::spawn_blocking(move || {
            crypto::encrypt_bytes(&passphrase_owned, &tar_bytes)
        })
        .await
        .map_err(redacted_internal_from)??;
        write_staged_backup(&staging_path, &encrypted)?;
        verify_backup_artifact(&staging_path, passphrase, &manifest)?;
        Ok(StagedBackupPackage {
            summary: BackupSummary {
                backup_path: final_path.to_string_lossy().to_string(),
                manifest,
            },
            staging_path: staging_path.clone(),
        })
    }
    .await;

    if let Err(error) = remove_private_build_directory(&build_dir) {
        let _ = remove_private_staging(&staging_path);
        return Err(error);
    }
    if stage_result.is_err() {
        remove_private_staging(&staging_path)?;
    }
    stage_result
}

pub async fn stage_backup_package(
    pool: &SqlitePool,
    workspace_id: &str,
    data_dir: &Path,
    database_path: &Path,
    final_path: &Path,
    passphrase: &str,
    lease: &BackupCreateLease,
) -> Result<StagedBackupPackage, AppError> {
    let staging_path = private_staging_path_for(lease)?;
    stage_backup_package_at_path(
        pool,
        workspace_id,
        data_dir,
        database_path,
        final_path,
        passphrase,
        lease,
        staging_path,
    )
    .await
}

pub async fn stage_backup_package_with_claim(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    data_dir: &Path,
    database_path: &Path,
    final_path: &Path,
    passphrase: &str,
    lease: &BackupCreateLease,
) -> Result<StagedBackupPackage, AppError> {
    let staging_path = prepare_backup_staging(
        pool,
        workspace_id,
        idempotency_key,
        job_type,
        lease,
    )
    .await?;
    stage_backup_package_at_path(
        pool,
        workspace_id,
        data_dir,
        database_path,
        final_path,
        passphrase,
        lease,
        staging_path,
    )
    .await
}

fn promote_staged_backup(
    staged: &StagedBackupPackage,
    lease: &BackupCreateLease,
) -> Result<(), AppError> {
    let final_path = Path::new(&staged.summary.backup_path);
    if final_path.exists() {
        return Err(AppError::storage("Backup path already exists"));
    }
    if !private_staging_path_is_safe(&staged.staging_path)
        || staging_token_from_root(
            staged
                .staging_path
                .parent()
                .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?,
        ) != Some(lease.token.as_str())
    {
        return Err(AppError::storage("Backup staging path is not owned by this claim"));
    }

    let staging_metadata = fs::symlink_metadata(&staged.staging_path)?;
    if staging_metadata.file_type().is_symlink() || !staging_metadata.is_file() {
        return Err(AppError::storage("Backup staging artifact is not a regular file"));
    }

    let publication_path = publication_path_for(final_path, &staged.staging_path)?;
    let mut source = fs::File::open(&staged.staging_path)?;
    let mut publication = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&publication_path)?;
    std::io::copy(&mut source, &mut publication)?;
    publication.sync_all()?;
    sync_parent_directory(&publication_path)?;
    fs::hard_link(&publication_path, final_path)?;
    sync_file(final_path)?;
    remove_publication_path(final_path, &staged.staging_path)?;
    remove_private_staging(&staged.staging_path)?;
    sync_parent_directory(final_path)
}

pub async fn publish_staged_backup(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
    staged: &StagedBackupPackage,
) -> Result<(), AppError> {
    renew_backup_create_lease(pool, workspace_id, idempotency_key, job_type, lease).await?;
    promote_staged_backup(staged, lease)
}

pub fn discard_staged_backup(
    staged: &StagedBackupPackage,
    lease: &BackupCreateLease,
) -> Result<(), AppError> {
    if !private_staging_path_is_safe(&staged.staging_path)
        || staging_token_from_root(
            staged
                .staging_path
                .parent()
                .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?,
        ) != Some(lease.token.as_str())
    {
        return Err(AppError::storage("Backup staging path is not owned by this claim"));
    }
    remove_publication_path(Path::new(&staged.summary.backup_path), &staged.staging_path)?;
    remove_private_staging(&staged.staging_path)
}

pub async fn create_backup_package(
    pool: &SqlitePool,
    workspace_id: &str,
    data_dir: &Path,
    database_path: &Path,
    destination_root: &Path,
    passphrase: &str,
    backup_file_path: Option<&str>,
) -> Result<BackupSummary, AppError> {
    let final_path = resolve_backup_file_path(destination_root, backup_file_path)?;
    let idempotency_key = Uuid::new_v4().to_string();
    let BackupCreateClaim::Proceed(lease) = claim_backup_create(
        pool,
        workspace_id,
        &idempotency_key,
        BACKUP_CREATE_JOB_TYPE,
    )
    .await?
    else {
        return Err(AppError::storage("Backup creation claim unexpectedly reused a completed job"));
    };
    let result = async {
        let staged = stage_backup_package_with_claim(
            pool,
            workspace_id,
            &idempotency_key,
            BACKUP_CREATE_JOB_TYPE,
            data_dir,
            database_path,
            &final_path,
            passphrase,
            &lease,
        )
        .await?;
        if let Err(error) = record_backup_staging(
            pool,
            workspace_id,
            &idempotency_key,
            BACKUP_CREATE_JOB_TYPE,
            &lease,
            &staged,
        )
        .await
        {
            let _ = discard_staged_backup(&staged, &lease);
            return Err(error);
        }
        if let Err(error) = publish_staged_backup(
            pool,
            workspace_id,
            &idempotency_key,
            BACKUP_CREATE_JOB_TYPE,
            &lease,
            &staged,
        )
        .await
        {
            let _ = discard_staged_backup(&staged, &lease);
            return Err(error);
        }
        finalize_backup_create(
            pool,
            workspace_id,
            &idempotency_key,
            BACKUP_CREATE_JOB_TYPE,
            &lease,
            &staged.summary,
        )
        .await?;
        Ok(staged.summary)
    }
    .await;
    match result {
        Ok(summary) => Ok(summary),
        Err(error) => {
            let _ = fail_backup_create_claim(
                pool,
                workspace_id,
                &idempotency_key,
                BACKUP_CREATE_JOB_TYPE,
                &lease,
            )
            .await;
            Err(error)
        }
    }
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

    let temp_dir = tempfile::tempdir().map_err(redacted_storage_from)?;
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
        .map_err(|_| AppError::validation("Invalid backup manifest", "backupPath"))?;
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
            .map_err(redacted_internal_from)?,
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
        let file_path = safe_backup_join(backup_path, relative)?;
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

    fs::create_dir_all(workspaces_root)?;
    let restore_workspace_id = Uuid::new_v4().to_string();
    let workspace_dir = workspaces_root.join(&restore_workspace_id);
    let staging_dir = workspaces_root.join(format!(".restore-{restore_workspace_id}.tmp"));
    if workspace_dir.exists() || staging_dir.exists() {
        return Err(AppError::storage("Restore target workspace directory already exists"));
    }
    let final_database_path = workspace_dir.join("workspace.sqlite");
    let final_documents_path = workspace_dir.join("documents");
    let final_exports_path = workspace_dir.join("exports");

    let restore_result = async {
        fs::create_dir_all(&staging_dir)?;
        let staging_documents_path = staging_dir.join("documents");
        let staging_exports_path = staging_dir.join("exports");
        fs::create_dir_all(&staging_documents_path)?;
        fs::create_dir_all(&staging_exports_path)?;

        let staging_database_path = staging_dir.join("workspace.sqlite");
        fs::copy(backup_path.join("workspace.sqlite"), &staging_database_path)?;
        let pool = connect_workspace(&staging_database_path).await?;

        if backup_path.join("documents").exists() {
            copy_dir_all(&backup_path.join("documents"), &staging_documents_path)?;
        }
        if backup_path.join("exports").exists() {
            copy_dir_all(&backup_path.join("exports"), &staging_exports_path)?;
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
        .bind(final_database_path.to_string_lossy().to_string())
        .bind(final_documents_path.to_string_lossy().to_string())
        .bind(final_exports_path.to_string_lossy().to_string())
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

        drop(pool);
        fs::rename(&staging_dir, &workspace_dir)?;
        Ok(BackupRestoreSummary {
            database_path: final_database_path.to_string_lossy().to_string(),
            workspace_id: workspace_id.clone(),
            workspace_name: workspace_name.clone(),
        })
    }
    .await;

    if restore_result.is_err() && staging_dir.exists() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    restore_result
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
        Some(requested) => Path::new(requested) == Path::new(summary.backup_path.as_str()),
        None => true,
    }
}

pub async fn check_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
) -> Result<Option<BackupSummary>, AppError> {
    let key = crate::idempotency::normalize_idempotency_key(idempotency_key)?;

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
    .bind(job_type)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    let Some(payload) = existing else {
        return Ok(None);
    };

    let parsed: IdempotentBackupPayload = serde_json::from_str(&payload)
        .map_err(redacted_internal_from)?;
    if parsed.summary.backup_path.is_empty() {
        return Ok(None);
    }
    Ok(Some(parsed.summary))
}

#[derive(Clone)]
pub struct BackupCreateLease {
    token: String,
}

impl BackupCreateLease {
    fn new() -> Self {
        Self {
            token: Uuid::new_v4().to_string(),
        }
    }
}

pub enum BackupCreateClaim {
    Proceed(BackupCreateLease),
    Cached(BackupSummary),
}

fn inactive_backup_claim_error() -> AppError {
    AppError::validation(
        "Backup claim is no longer active for this idempotency key",
        "idempotencyKey",
    )
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

    let key = crate::idempotency::normalize_idempotency_key(idempotency_key)?;
    let lease = BackupCreateLease::new();
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
        staging_path: None,
        publication_path: None,
        staging_root: None,
        lease: Some(BackupClaimLeasePayload {
            token: lease.token.clone(),
            expires_at: String::new(),
        }),
    };
    let payload_json = serde_json::to_string(&pending)
        .map_err(redacted_internal_from)?;

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
    .bind(job_type)
    .bind(&payload_json)
    .bind(BACKUP_CLAIM_LEASE_DURATION)
    .bind(key)
    .execute(pool)
    .await
    {
        Ok(_) => Ok(BackupCreateClaim::Proceed(lease)),
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            let reclaimed = sqlx::query(
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
                  AND (
                    status = 'failed'
                    OR (
                        status = 'running'
                        AND (
                            datetime(json_extract(payload_json, '$.lease.expiresAt')) <= CURRENT_TIMESTAMP
                            OR (
                                json_extract(payload_json, '$.lease.expiresAt') IS NULL
                                AND updated_at <= datetime('now', ?6)
                            )
                        )
                    )
                  )
                "#,
            )
            .bind(workspace_id)
            .bind(job_type)
            .bind(key)
            .bind(&lease.token)
            .bind(BACKUP_CLAIM_LEASE_DURATION)
            .bind(LEGACY_STALE_IDEMPOTENCY_CLAIM_AFTER)
            .execute(pool)
            .await?;
            if reclaimed.rows_affected() == 1 {
                cleanup_reclaimed_prebuild_staging(
                    pool,
                    workspace_id,
                    idempotency_key,
                    job_type,
                    &lease,
                )
                .await?;
                return Ok(BackupCreateClaim::Proceed(lease));
            }
            if let Some(summary) =
                check_idempotency(pool, workspace_id, idempotency_key, job_type).await?
            {
                return Ok(BackupCreateClaim::Cached(summary));
            }
            Err(AppError::validation(
                "Backup already in progress for this idempotency key",
                "idempotencyKey",
            ))
        }
        Err(error) => Err(error.into()),
    }
}

pub async fn renew_backup_create_lease(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
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
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(BACKUP_CLAIM_LEASE_DURATION)
    .execute(pool)
    .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(inactive_backup_claim_error())
    }
}

pub async fn record_backup_staging(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
    staged: &StagedBackupPackage,
) -> Result<(), AppError> {
    let summary_json = serde_json::to_string(&staged.summary).map_err(redacted_internal_from)?;
    let publication_path =
        publication_path_for(Path::new(&staged.summary.backup_path), &staged.staging_path)?;
    let staging_root = staged
        .staging_path
        .parent()
        .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?;
    if !private_staging_path_is_safe(&staged.staging_path)
        || staging_token_from_root(staging_root) != Some(lease.token.as_str())
    {
        return Err(AppError::storage("Backup staging path is not owned by this claim"));
    }
    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET payload_json = json_set(
                payload_json,
                '$.summary', json(?5),
                '$.stagingPath', ?6,
                '$.stagingRoot', ?7,
                '$.publicationPath', ?8
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
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(summary_json)
    .bind(staged.staging_path.to_string_lossy().to_string())
    .bind(staging_root.to_string_lossy().to_string())
    .bind(publication_path.to_string_lossy().to_string())
    .execute(pool)
    .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(inactive_backup_claim_error())
    }
}


pub async fn prepare_backup_staging(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
) -> Result<PathBuf, AppError> {
    let staging_path = private_staging_path_for(lease)?;
    let staging_root = staging_path
        .parent()
        .ok_or_else(|| AppError::storage("Backup staging path has no parent directory"))?;
    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET payload_json = json_set(
                payload_json,
                '$.stagingPath', ?5,
                '$.stagingRoot', ?6
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
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(staging_path.to_string_lossy().to_string())
    .bind(staging_root.to_string_lossy().to_string())
    .execute(pool)
    .await?;
    if result.rows_affected() == 1 {
        Ok(staging_path)
    } else {
        let _ = remove_private_staging_tree(&staging_path);
        Err(inactive_backup_claim_error())
    }
}
fn recorded_staging_paths_are_safe(
    final_path: &Path,
    staging_path: &Path,
    staging_root: Option<&Path>,
    publication_path: &Path,
) -> bool {
    if !private_staging_path_has_safe_shape(staging_path) {
        return false;
    }
    let Some(actual_root) = staging_path.parent() else {
        return false;
    };
    if staging_root.is_some_and(|recorded_root| recorded_root != actual_root) {
        return false;
    }
    match fs::symlink_metadata(actual_root) {
        Ok(_) if !private_staging_path_is_safe(staging_path) => return false,
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return false,
        _ => {}
    }
    publication_path_for(final_path, staging_path)
        .map(|expected| expected == publication_path)
        .unwrap_or(false)
}

fn recorded_staging_root_is_safe(staging_path: &Path, staging_root: Option<&Path>) -> bool {
    if !private_staging_path_has_safe_shape(staging_path) {
        return false;
    }
    let Some(actual_root) = staging_path.parent() else {
        return false;
    };
    if staging_root.is_some_and(|recorded_root| recorded_root != actual_root) {
        return false;
    }
    match fs::symlink_metadata(actual_root) {
        Ok(_) => private_staging_path_is_safe(staging_path),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn remove_recorded_staging_root(
    staging_path: &Path,
    staging_root: Option<&Path>,
) -> Result<(), AppError> {
    if !recorded_staging_root_is_safe(staging_path, staging_root) {
        return Err(AppError::storage("Invalid recorded backup staging path"));
    }
    if staging_path.parent().is_some_and(Path::exists) {
        remove_private_staging_tree(staging_path)?;
    }
    Ok(())
}

fn remove_recorded_staging_artifacts(
    final_path: &Path,
    staging_path: &Path,
    staging_root: Option<&Path>,
    publication_path: &Path,
) -> Result<(), AppError> {
    if !recorded_staging_paths_are_safe(final_path, staging_path, staging_root, publication_path) {
        return Err(AppError::storage("Invalid recorded backup staging path"));
    }
    remove_publication_path(final_path, staging_path)?;
    if staging_path.parent().is_some_and(Path::exists) {
        remove_private_staging_tree(staging_path)?;
    }
    Ok(())
}

async fn clear_recorded_backup_staging(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
) -> Result<(), AppError> {
    let empty_summary = BackupSummary {
        backup_path: String::new(),
        manifest: BackupManifest {
            version: 0,
            workspace_id: workspace_id.to_string(),
            workspace_name: String::new(),
            created_at: String::new(),
            entries: vec![],
            manifest_sha256: String::new(),
        },
    };
    let summary_json = serde_json::to_string(&empty_summary).map_err(redacted_internal_from)?;
    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET payload_json = json_remove(
                json_set(payload_json, '$.summary', json(?5)),
                '$.stagingPath',
                '$.stagingRoot',
                '$.publicationPath'
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
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(summary_json)
    .execute(pool)
    .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(inactive_backup_claim_error())
    }
}

pub async fn discard_recorded_backup_staging(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
) -> Result<(), AppError> {
    let payload_json: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND idempotency_key = ?3
          AND status = 'running'
          AND json_extract(payload_json, '$.lease.token') = ?4
          AND datetime(json_extract(payload_json, '$.lease.expiresAt')) > CURRENT_TIMESTAMP
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .fetch_optional(pool)
    .await?;
    let Some(payload_json) = payload_json else {
        return Err(inactive_backup_claim_error());
    };
    let payload: IdempotentBackupPayload =
        serde_json::from_str(&payload_json).map_err(redacted_internal_from)?;
    if let Some(staging_path) = payload.staging_path.as_deref().map(Path::new) {
        remove_recorded_staging_root(
            staging_path,
            payload.staging_root.as_deref().map(Path::new),
        )?;
    }
    clear_recorded_backup_staging(pool, workspace_id, idempotency_key, job_type, lease).await
}

async fn cleanup_reclaimed_prebuild_staging(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
) -> Result<(), AppError> {
    let payload_json: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND idempotency_key = ?3
          AND status = 'running'
          AND json_extract(payload_json, '$.lease.token') = ?4
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .fetch_optional(pool)
    .await?;
    let Some(payload_json) = payload_json else {
        return Err(inactive_backup_claim_error());
    };
    let payload: IdempotentBackupPayload =
        serde_json::from_str(&payload_json).map_err(redacted_internal_from)?;
    if !payload.summary.backup_path.is_empty() {
        return Ok(());
    }
    let Some(staging_path) = payload.staging_path.as_deref().map(Path::new) else {
        return Ok(());
    };
    let Some(staging_root) = staging_path.parent() else {
        return Err(AppError::storage("Invalid recorded backup staging path"));
    };
    if staging_token_from_root(staging_root) == Some(lease.token.as_str()) {
        return Ok(());
    }
    remove_recorded_staging_root(staging_path, payload.staging_root.as_deref().map(Path::new))?;
    clear_recorded_backup_staging(pool, workspace_id, idempotency_key, job_type, lease).await
}

pub async fn cleanup_stale_backup_staging_for_workspace(pool: &SqlitePool) -> Result<(), AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT id, payload_json FROM local_jobs
        WHERE job_type = ?1
          AND (
              status != 'running'
              OR datetime(json_extract(payload_json, '$.lease.expiresAt')) <= CURRENT_TIMESTAMP
          )
        "#,
    )
    .bind(BACKUP_CREATE_JOB_TYPE)
    .fetch_all(pool)
    .await?;
    for (job_id, payload_json) in rows {
        let payload: IdempotentBackupPayload =
            serde_json::from_str(&payload_json).map_err(redacted_internal_from)?;
        if !payload.summary.backup_path.is_empty() {
            continue;
        }
        let Some(staging_path) = payload.staging_path.as_deref().map(Path::new) else {
            continue;
        };
        remove_recorded_staging_root(staging_path, payload.staging_root.as_deref().map(Path::new))?;
        sqlx::query(
            r#"
            UPDATE local_jobs
            SET payload_json = json_remove(payload_json, '$.stagingPath', '$.stagingRoot', '$.publicationPath'),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
              AND (
                  status != 'running'
                  OR datetime(json_extract(payload_json, '$.lease.expiresAt')) <= CURRENT_TIMESTAMP
              )
            "#,
        )
        .bind(job_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn cleanup_stale_backup_staging_at_startup(app_data_dir: &Path) -> Result<(), AppError> {
    let mut first_error = None;
    for recent_workspace in crate::recent::list_recent_workspaces(app_data_dir)? {
        let Ok(pool) = open_existing_workspace(Path::new(&recent_workspace.database_path)).await else {
            continue;
        };
        if let Err(error) = cleanup_stale_backup_staging_for_workspace(&pool).await {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

pub async fn recover_backup_create(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
    passphrase: &str,
) -> Result<Option<BackupSummary>, AppError> {
    let payload_json: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND idempotency_key = ?3
          AND status = 'running'
          AND json_extract(payload_json, '$.lease.token') = ?4
          AND datetime(json_extract(payload_json, '$.lease.expiresAt')) > CURRENT_TIMESTAMP
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .fetch_optional(pool)
    .await?;
    let Some(payload_json) = payload_json else {
        return Err(inactive_backup_claim_error());
    };
    let payload: IdempotentBackupPayload =
        serde_json::from_str(&payload_json).map_err(redacted_internal_from)?;
    if payload.summary.backup_path.is_empty() {
        return Ok(None);
    }

    let final_path = Path::new(&payload.summary.backup_path);
    let (Some(staging_path), Some(publication_path)) = (
        payload.staging_path.as_deref().map(Path::new),
        payload.publication_path.as_deref().map(Path::new),
    ) else {
        return Err(AppError::storage(
            "Interrupted backup staging metadata is incomplete",
        ));
    };
    if !recorded_staging_paths_are_safe(
        final_path,
        staging_path,
        payload.staging_root.as_deref().map(Path::new),
        publication_path,
    ) {
        return Err(AppError::storage("Invalid recorded backup staging path"));
    }

    if final_path.exists() {
        match verify_backup_artifact(final_path, passphrase, &payload.summary.manifest) {
            Ok(()) => {
                remove_recorded_staging_artifacts(
                    final_path,
                    staging_path,
                    payload.staging_root.as_deref().map(Path::new),
                    publication_path,
                )?;
                clear_recorded_backup_staging(
                    pool,
                    workspace_id,
                    idempotency_key,
                    job_type,
                    lease,
                )
                .await?;
                return Ok(Some(payload.summary));
            }
            Err(_) => {
                remove_recorded_staging_artifacts(
                    final_path,
                    staging_path,
                    payload.staging_root.as_deref().map(Path::new),
                    publication_path,
                )?;
                clear_recorded_backup_staging(
                    pool,
                    workspace_id,
                    idempotency_key,
                    job_type,
                    lease,
                )
                .await?;
                return Err(AppError::storage(
                    "Existing backup path does not match the interrupted backup",
                ));
            }
        }
    }

    remove_recorded_staging_artifacts(
        final_path,
        staging_path,
        payload.staging_root.as_deref().map(Path::new),
        publication_path,
    )?;
    clear_recorded_backup_staging(pool, workspace_id, idempotency_key, job_type, lease).await?;
    Ok(None)
}

pub async fn fail_backup_create_claim(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
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
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(BACKUP_CLAIM_FAILURE_REASON)
    .execute(pool)
    .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(inactive_backup_claim_error())
    }
}

pub async fn finalize_backup_create(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    job_type: &str,
    lease: &BackupCreateLease,
    summary: &BackupSummary,
) -> Result<(), AppError> {
    let payload = IdempotentBackupPayload {
        idempotency_key: crate::idempotency::normalize_idempotency_key(idempotency_key)?.to_string(),
        summary: summary.clone(),
        staging_path: None,
        publication_path: None,
        staging_root: None,
        lease: None,
    };
    let payload_json = serde_json::to_string(&payload).map_err(redacted_internal_from)?;
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = 'succeeded',
            payload_json = ?5,
            last_error = NULL,
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
    .bind(job_type)
    .bind(crate::idempotency::normalize_idempotency_key(idempotency_key)?)
    .bind(&lease.token)
    .bind(payload_json)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        return Err(inactive_backup_claim_error());
    }
    record_event_tx(
        &mut *transaction,
        workspace_id,
        "workspace_backup_create",
        "backup",
        Some(&summary.backup_path),
        &serde_json::to_string(&summary.manifest).map_err(redacted_internal_from)?,
    )
    .await?;
    transaction.commit().await?;
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
        staging_path: None,
        publication_path: None,
        staging_root: None,
        lease: None,
    };
    let payload_json = serde_json::to_string(&payload)
        .map_err(redacted_internal_from)?;

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

#[cfg(test)]
mod recovery_tests {
    use super::{connect_workspace, hash_file, hash_string, restore_from_staged_backup};
    use std::fs;
    use tempfile::tempdir;

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_restore_never_publishes_a_cleartext_workspace_directory() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().expect("tempdir");
        let backup_root = dir.path().join("staged-backup");
        let documents_dir = backup_root.join("documents");
        fs::create_dir_all(&documents_dir).expect("documents");
        fs::create_dir_all(backup_root.join("exports")).expect("exports");

        let database_path = backup_root.join("workspace.sqlite");
        let pool = connect_workspace(&database_path).await.expect("database");
        crate::db::wal_checkpoint_truncate(&pool)
            .await
            .expect("checkpoint");
        drop(pool);

        let outside_file = dir.path().join("outside-evidence");
        fs::write(&outside_file, b"evidence").expect("outside evidence");
        symlink(&outside_file, documents_dir.join("linked-evidence")).expect("symlink");

        let (database_sha256, database_bytes) = hash_file(&database_path).expect("database hash");
        let manifest_body = serde_json::json!({
            "version": 1,
            "workspaceId": "source-workspace",
            "workspaceName": "Source workspace",
            "createdAt": "2026-01-01T00:00:00Z",
            "entries": [{
                "relativePath": "workspace.sqlite",
                "sha256": database_sha256,
                "bytes": database_bytes,
                "entryType": "file",
            }],
        });
        let manifest_hash = hash_string(
            &serde_json::to_string(&manifest_body).expect("serialize manifest body"),
        );
        let manifest = serde_json::json!({
            "version": manifest_body["version"],
            "workspaceId": manifest_body["workspaceId"],
            "workspaceName": manifest_body["workspaceName"],
            "createdAt": manifest_body["createdAt"],
            "entries": manifest_body["entries"],
            "manifestSha256": manifest_hash,
        });
        fs::write(
            backup_root.join("manifest.json"),
            serde_json::to_vec(&manifest).expect("serialize manifest"),
        )
        .expect("write manifest");

        let restores_root = dir.path().join("restores");
        fs::create_dir_all(&restores_root).expect("restores root");
        let error = restore_from_staged_backup(&backup_root, &restores_root)
            .await
            .expect_err("symlink must make restore fail");

        assert_eq!(error.code, "validation_error");
        assert!(
            fs::read_dir(&restores_root)
                .expect("read restores")
                .next()
                .is_none(),
            "failed restore must remove its unpublished cleartext workspace"
        );
    }
}
