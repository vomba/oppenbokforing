use std::collections::HashSet;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use super::*;

fn invalid(message: &str) -> AppError {
    AppError::validation(message, "backupPath")
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn manifest_hash(manifest: &BackupManifest) -> Result<String, AppError> {
    let body = serde_json::json!({
        "version": manifest.version,
        "workspaceId": manifest.workspace_id,
        "workspaceName": manifest.workspace_name,
        "createdAt": manifest.created_at,
        "entries": manifest.entries,
    });
    Ok(hash_string(
        &serde_json::to_string(&body).map_err(redacted_internal_from)?,
    ))
}

fn validate_manifest(manifest: &BackupManifest) -> Result<(), AppError> {
    if manifest.version != MANIFEST_VERSION {
        return Err(invalid("Unsupported backup manifest version"));
    }
    if manifest.workspace_id.trim().is_empty()
        || manifest.workspace_name.trim().is_empty()
        || chrono::DateTime::parse_from_rfc3339(&manifest.created_at).is_err()
        || !is_sha256(&manifest.manifest_sha256)
    {
        return Err(invalid("Invalid backup manifest metadata"));
    }
    if manifest_hash(manifest)? != manifest.manifest_sha256 {
        return Err(invalid("Backup manifest hash mismatch"));
    }
    let mut paths = HashSet::new();
    let mut has_database = false;
    let mut physical_entries = 1; // manifest.json itself is also an archive entry.
    for entry in &manifest.entries {
        let directory = entry.entry_type == "directory" || entry.entry_type == "database_table";
        let relative = crypto::confined_relative_path(&entry.relative_path)?;
        if directory != entry.relative_path.ends_with('/') || !paths.insert(relative.clone()) {
            return Err(invalid("Invalid or duplicate backup manifest path"));
        }
        let path = relative
            .to_str()
            .ok_or_else(|| invalid("Invalid backup manifest path"))?;
        match entry.entry_type.as_str() {
            "file" => {
                if !is_sha256(&entry.sha256)
                    || !(path == "workspace.sqlite"
                        || path.starts_with("documents/")
                        || path.starts_with("exports/"))
                {
                    return Err(invalid("Invalid backup manifest file entry"));
                }
                has_database |= path == "workspace.sqlite";
                physical_entries += 1;
            }
            "directory" => {
                if !entry.sha256.is_empty()
                    || entry.bytes != 0
                    || !(path == "documents"
                        || path == "exports"
                        || path.starts_with("documents/")
                        || path.starts_with("exports/"))
                {
                    return Err(invalid("Invalid backup manifest directory entry"));
                }
                physical_entries += 1;
            }
            "database_table" => {
                // Version 1 uses bytes as a row count for this one virtual entry.
                if path != "rule_versions" || !entry.sha256.is_empty() || entry.bytes == 0 {
                    return Err(invalid("Invalid backup manifest database-table entry"));
                }
            }
            _ => return Err(invalid("Unsupported backup manifest entry type")),
        }
    }
    if !has_database {
        return Err(invalid("Backup manifest is missing workspace.sqlite"));
    }
    if physical_entries > crypto::MAX_TAR_ENTRIES {
        return Err(invalid("Backup archive contains too many entries"));
    }
    Ok(())
}

async fn verify_database(root: &Path, manifest: &BackupManifest) -> Result<(), AppError> {
    let database_path = root.join("workspace.sqlite");
    // Verification must neither migrate the snapshot nor follow its stored physical paths.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&database_path)
                .read_only(true)
                .immutable(true),
        )
        .await?;
    let result = async {
        let check: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await?;
        if check != "ok"
            || sqlx::query("PRAGMA foreign_key_check")
                .fetch_optional(&pool)
                .await?
                .is_some()
        {
            return Err(invalid("Backup database integrity check failed"));
        }
        let identities: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM workspaces")
            .fetch_all(&pool)
            .await?;
        if identities.len() != 1
            || identities[0]
                != (
                    manifest.workspace_id.clone(),
                    manifest.workspace_name.clone(),
                )
        {
            return Err(invalid(
                "Backup manifest/database workspace identity mismatch",
            ));
        }
        for entry in &manifest.entries {
            if entry.entry_type == "database_table" {
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rule_versions")
                    .fetch_one(&pool)
                    .await?;
                if count < 0 || count as u64 != entry.bytes {
                    return Err(invalid("Backup rule_versions row count mismatch"));
                }
            }
        }
        let documents: Vec<(String, String, String)> =
            sqlx::query_as("SELECT workspace_id, object_path, content_sha256 FROM documents")
                .fetch_all(&pool)
                .await?;
        for (workspace_id, object_path, sha256) in documents {
            if workspace_id != manifest.workspace_id || !is_sha256(&sha256) {
                return Err(invalid(
                    "Invalid retained evidence metadata in backup database",
                ));
            }
            crypto::confined_relative_path(&object_path)?;
            crate::documents::verify_retained_document_object(
                &root.join("documents"),
                &database_path.to_string_lossy(),
                &object_path,
                &sha256,
            )?;
        }
        Ok(())
    }
    .await;
    pool.close().await;
    result
}

pub(super) async fn staged_package(root: &Path) -> Result<BackupManifest, AppError> {
    let manifest_path = root.join("manifest.json");
    let metadata =
        fs::symlink_metadata(&manifest_path).map_err(|_| invalid("Backup manifest missing"))?;
    if metadata_is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(invalid("Backup manifest is not a regular file"));
    }
    let manifest: BackupManifest = serde_json::from_slice(&fs::read(manifest_path)?)
        .map_err(|_| invalid("Invalid backup manifest schema"))?;
    validate_manifest(&manifest)?;

    // Inventory equality prevents undeclared files from being copied to the restored workspace.
    let actual = collect_backup_entries(root)?;
    let mut declared: Vec<_> = manifest
        .entries
        .iter()
        .filter(|entry| entry.entry_type != "database_table")
        .cloned()
        .collect();
    declared.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if actual != declared {
        return Err(invalid(
            "Backup staged files do not match manifest paths, types, hashes or sizes",
        ));
    }
    // This reconciliation also detects evidence omitted from BOTH the archive and the manifest.
    verify_database(root, &manifest).await?;
    Ok(manifest)
}

pub(super) fn extract_artifact(
    path: &Path,
    passphrase: &str,
    destination: &Path,
) -> Result<(), AppError> {
    let metadata = fs::metadata(path)?;
    crypto::validate_encrypted_size(metadata.len())?;
    let encrypted = fs::read(path)?;
    crypto::validate_encrypted_size(encrypted.len() as u64)?;
    let tar_bytes = crypto::decrypt_bytes(passphrase, &encrypted)?;
    crypto::extract_tar_archive(&tar_bytes, destination)
}

pub(super) async fn artifact(
    path: &Path,
    passphrase: &str,
    expected: &BackupManifest,
) -> Result<(), AppError> {
    let verification_dir = tempfile::tempdir().map_err(redacted_storage_from)?;
    extract_artifact(path, passphrase, verification_dir.path())?;
    let manifest = staged_package(verification_dir.path()).await?;
    if manifest != *expected {
        return Err(invalid(
            "Backup manifest does not match the interrupted backup",
        ));
    }
    Ok(())
}
