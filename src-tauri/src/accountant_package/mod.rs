use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::SqlitePool;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use sha2::{Digest, Sha256};

use crate::{
    audit::record_event,
    error::AppError,
    sie::{self, SieExportCreateInput},
    workspace::{
        ensure_path_within_root, reject_path_traversal, resolve_workspace_exports_dir,
        safe_join_under,
    },
    year_end::{self, YearEndPackageFindInput},
};

const MANIFEST_VERSION: u32 = 1;
const JOB_ACCOUNTANT_EXPORT: &str = "accountant_package_export_create";
const MAX_VALIDATE_HASH_BYTES: u64 = 52_428_800;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountantPackageEntry {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
    pub entry_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountantPackageManifest {
    pub version: u32,
    pub workspace_id: String,
    pub workspace_name: String,
    pub fiscal_year: i32,
    pub created_at: String,
    pub entries: Vec<AccountantPackageEntry>,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountantPackageExportSummary {
    pub package_path: String,
    pub manifest: AccountantPackageManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountantPackageValidateSummary {
    pub valid: bool,
    pub workspace_id: String,
    pub workspace_name: String,
    pub fiscal_year: i32,
    pub entry_count: usize,
    pub manual_fallback_hint: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountantPackageExportCreateInput {
    pub fiscal_year: i32,
    pub idempotency_key: String,
    pub export_directory: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountantPackageImportValidateInput {
    pub package_path: String,
}

fn normalize_idempotency_key(key: &str) -> Result<String, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(
            "Idempotency key is required",
            "idempotencyKey",
        ));
    }
    Ok(trimmed.to_string())
}

fn hash_file(path: &Path, max_bytes: Option<u64>) -> Result<(String, u64), AppError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut bytes = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes += read as u64;
        if let Some(limit) = max_bytes {
            if bytes > limit {
                return Err(AppError::validation(
                    "Package entry exceeds size limit",
                    "packagePath",
                ));
            }
        }
        hasher.update(&buffer[..read]);
    }
    Ok((format!("{:x}", hasher.finalize()), bytes))
}

fn hash_string(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn resolve_package_path(
    package_path: &str,
    exports_path: &str,
    database_path: &str,
) -> Result<PathBuf, AppError> {
    reject_path_traversal(package_path, "packagePath")?;
    let path = PathBuf::from(package_path);
    if path.is_absolute() {
        if path.is_file() {
            return Ok(path);
        }
        return Err(AppError::validation(
            "Package manifest file not found",
            "packagePath",
        ));
    }
    let export_root = resolve_workspace_exports_dir(exports_path, database_path)?;
    let joined = safe_join_under(&export_root, package_path, "packagePath")?;
    if joined.exists() {
        ensure_path_within_root(&joined, &export_root, "packagePath")?;
    }
    Ok(joined)
}

fn idempotency_fiscal_year_mismatch() -> AppError {
    AppError::validation(
        "Idempotency key was already used for a different fiscal year",
        "idempotencyKey",
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdempotentAccountantExportPayload {
    fiscal_year: i32,
    package_path: String,
    manifest: AccountantPackageManifest,
}

async fn check_accountant_export_idempotency(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotentAccountantExportPayload>, AppError> {
    let payload: Option<String> = sqlx::query_scalar(
        r#"
        SELECT payload_json FROM local_jobs
        WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_ACCOUNTANT_EXPORT)
    .bind(idempotency_key)
    .fetch_optional(pool)
    .await?;

    let Some(json) = payload else {
        return Ok(None);
    };

    let parsed: IdempotentAccountantExportPayload =
        serde_json::from_str(&json).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Some(parsed))
}

async fn write_accountant_package_files(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &AccountantPackageExportCreateInput,
    idempotency_key: &str,
    package_root: &Path,
    rel_package_path: &str,
) -> Result<AccountantPackageExportSummary, AppError> {
    let sie_summary = sie::sie_export_create(
        pool,
        workspace_id,
        &SieExportCreateInput {
            fiscal_year: input.fiscal_year,
            idempotency_key: format!("{idempotency_key}-sie"),
            export_directory: None,
        },
    )
    .await?;

    let (workspace_name, exports_path, database_path): (String, String, String) =
        sqlx::query_as(
            r#"
            SELECT name, exports_path, database_path FROM workspaces WHERE id = ?1 LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .fetch_one(pool)
        .await?;

    let export_root = resolve_workspace_exports_dir(&exports_path, &database_path)?;
    fs::create_dir_all(package_root).map_err(AppError::from)?;

    let mut entries = Vec::new();

    let sie_src = export_root.join(&sie_summary.export_path);
    let sie_dest = package_root.join("ledger.sie");
    fs::copy(&sie_src, &sie_dest).map_err(AppError::from)?;
    let (sie_hash, sie_bytes) = hash_file(&sie_dest, None)?;
    entries.push(AccountantPackageEntry {
        relative_path: "ledger.sie".to_string(),
        sha256: sie_hash,
        bytes: sie_bytes,
        entry_type: "sie".to_string(),
    });

    if let Some(year_end) = year_end::year_end_package_find_by_fiscal_year(
        pool,
        workspace_id,
        &YearEndPackageFindInput {
            fiscal_year: input.fiscal_year,
        },
    )
    .await?
    {
        if let Some(export_path) = year_end.export_path {
            let src = export_root.join(&export_path);
            if src.exists() {
                let dest = package_root.join("year-end-package.json");
                fs::copy(&src, &dest).map_err(AppError::from)?;
                let (hash, bytes) = hash_file(&dest, None)?;
                entries.push(AccountantPackageEntry {
                    relative_path: "year-end-package.json".to_string(),
                    sha256: hash,
                    bytes,
                    entry_type: "year_end".to_string(),
                });
            }
        }
    }

    let evidence_index = serde_json::json!({
        "workspaceId": workspace_id,
        "fiscalYear": input.fiscal_year,
        "generatedAt": Utc::now().to_rfc3339(),
        "sieExportPath": sie_summary.export_path,
        "entries": entries,
    });
    let evidence_path = package_root.join("evidence-index.json");
    fs::write(
        &evidence_path,
        serde_json::to_string_pretty(&evidence_index)
            .map_err(|e| AppError::internal(e.to_string()))?,
    )
    .map_err(AppError::from)?;
    let (evidence_hash, evidence_bytes) = hash_file(&evidence_path, None)?;
    entries.push(AccountantPackageEntry {
        relative_path: "evidence-index.json".to_string(),
        sha256: evidence_hash,
        bytes: evidence_bytes,
        entry_type: "evidence_index".to_string(),
    });

    let manifest_body = serde_json::json!({
        "version": MANIFEST_VERSION,
        "workspaceId": workspace_id,
        "workspaceName": workspace_name,
        "fiscalYear": input.fiscal_year,
        "createdAt": Utc::now().to_rfc3339(),
        "entries": entries,
    });
    let manifest_sha256 = hash_string(&manifest_body.to_string());
    let manifest = AccountantPackageManifest {
        version: MANIFEST_VERSION,
        workspace_id: workspace_id.to_string(),
        workspace_name: workspace_name.clone(),
        fiscal_year: input.fiscal_year,
        created_at: manifest_body["createdAt"].as_str().unwrap_or_default().to_string(),
        entries: entries.clone(),
        manifest_sha256: manifest_sha256.clone(),
    };

    let manifest_path = package_root.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| AppError::internal(e.to_string()))?,
    )
    .map_err(AppError::from)?;

    Ok(AccountantPackageExportSummary {
        package_path: rel_package_path.to_string(),
        manifest,
    })
}

fn package_suffix_from_key(idempotency_key: &str) -> String {
    let digest = Sha256::digest(idempotency_key.as_bytes());
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_package_summary(
    package_path: &str,
    manifest_path: &Path,
) -> Result<AccountantPackageExportSummary, AppError> {
    let manifest_text = fs::read_to_string(manifest_path).map_err(AppError::from)?;
    let manifest: AccountantPackageManifest =
        serde_json::from_str(&manifest_text).map_err(|_| {
            AppError::validation("Invalid accountant package manifest", "packagePath")
        })?;
    Ok(AccountantPackageExportSummary {
        package_path: package_path.to_string(),
        manifest,
    })
}

async fn update_accountant_export_payload(
    pool: &SqlitePool,
    workspace_id: &str,
    idempotency_key: &str,
    fiscal_year: i32,
    summary: &AccountantPackageExportSummary,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(&IdempotentAccountantExportPayload {
        fiscal_year,
        package_path: summary.package_path.clone(),
        manifest: summary.manifest.clone(),
    })
    .map_err(|e| AppError::internal(e.to_string()))?;

    sqlx::query(
        r#"
        UPDATE local_jobs
        SET payload_json = ?1
        WHERE workspace_id = ?2 AND job_type = ?3 AND idempotency_key = ?4
        "#,
    )
    .bind(&payload)
    .bind(workspace_id)
    .bind(JOB_ACCOUNTANT_EXPORT)
    .bind(idempotency_key)
    .execute(pool)
    .await?;

    Ok(())
}

async fn ensure_accountant_package_files(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &AccountantPackageExportCreateInput,
    idempotency_key: &str,
    cached: &IdempotentAccountantExportPayload,
) -> Result<AccountantPackageExportSummary, AppError> {
    let (exports_path, database_path): (String, String) = sqlx::query_as(
        r#"
        SELECT exports_path, database_path FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let manifest_path =
        resolve_package_path(&cached.package_path, &exports_path, &database_path)?;
    if manifest_path.is_file() {
        let summary = read_package_summary(&cached.package_path, &manifest_path)?;
        return publish_accountant_summary(
            pool,
            workspace_id,
            &exports_path,
            &database_path,
            summary,
            input.export_directory.as_deref(),
        )
        .await;
    }

    let package_root = manifest_path
        .parent()
        .ok_or_else(|| AppError::validation("Invalid package path", "packagePath"))?;

    let summary = write_accountant_package_files(
        pool,
        workspace_id,
        input,
        idempotency_key,
        package_root,
        &cached.package_path,
    )
    .await?;

    update_accountant_export_payload(
        pool,
        workspace_id,
        idempotency_key,
        cached.fiscal_year,
        &summary,
    )
    .await?;

    publish_accountant_summary(
        pool,
        workspace_id,
        &exports_path,
        &database_path,
        summary,
        input.export_directory.as_deref(),
    )
    .await
}

fn package_directory_relative(manifest_rel: &str) -> String {
    Path::new(manifest_rel)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| manifest_rel.to_string())
}

async fn publish_accountant_summary(
    pool: &SqlitePool,
    workspace_id: &str,
    exports_path: &str,
    database_path: &str,
    summary: AccountantPackageExportSummary,
    export_directory: Option<&str>,
) -> Result<AccountantPackageExportSummary, AppError> {
    if crate::paths::resolve_export_directory(pool, workspace_id, export_directory)
        .await?
        .is_none()
    {
        return Ok(summary);
    }

    let rel_dir = package_directory_relative(&summary.package_path);
    let published_dir = crate::paths::publish_export_directory(
        pool,
        workspace_id,
        exports_path,
        database_path,
        &rel_dir,
        export_directory,
    )
    .await?;
    let manifest_name = Path::new(&summary.package_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("manifest.json");
    Ok(AccountantPackageExportSummary {
        package_path: Path::new(&published_dir)
            .join(manifest_name)
            .to_string_lossy()
            .replace('\\', "/"),
        manifest: summary.manifest,
    })
}

pub async fn accountant_package_export_create(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &AccountantPackageExportCreateInput,
) -> Result<AccountantPackageExportSummary, AppError> {
    let idempotency_key = normalize_idempotency_key(&input.idempotency_key)?;

    if let Some(cached) =
        check_accountant_export_idempotency(pool, workspace_id, &idempotency_key).await?
    {
        if cached.fiscal_year != input.fiscal_year {
            return Err(idempotency_fiscal_year_mismatch());
        }
        return ensure_accountant_package_files(
            pool,
            workspace_id,
            input,
            &idempotency_key,
            &cached,
        )
        .await;
    }

    let (exports_path, database_path): (String, String) = sqlx::query_as(
        r#"
        SELECT exports_path, database_path FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let export_root = resolve_workspace_exports_dir(&exports_path, &database_path)?;
    let package_dir = export_root.join("accountant-packages");
    fs::create_dir_all(&package_dir).map_err(AppError::from)?;

    let package_suffix = package_suffix_from_key(&idempotency_key);
    let package_root = package_dir.join(format!("{}-{}", input.fiscal_year, package_suffix));
    let rel_package = format!(
        "accountant-packages/{}-{}/manifest.json",
        input.fiscal_year, package_suffix
    );

    let summary = write_accountant_package_files(
        pool,
        workspace_id,
        input,
        &idempotency_key,
        &package_root,
        &rel_package,
    )
    .await?;

    let payload = serde_json::to_string(&IdempotentAccountantExportPayload {
        fiscal_year: input.fiscal_year,
        package_path: summary.package_path.clone(),
        manifest: summary.manifest.clone(),
    })
    .map_err(|e| AppError::internal(e.to_string()))?;

    match sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(workspace_id)
    .bind(JOB_ACCOUNTANT_EXPORT)
    .bind(&payload)
    .bind(&idempotency_key)
    .execute(pool)
    .await
    {
        Ok(_) => {}
        Err(error) if crate::error::is_sqlite_unique_violation(&error) => {
            if let Some(cached) =
                check_accountant_export_idempotency(pool, workspace_id, &idempotency_key).await?
            {
                if cached.fiscal_year != input.fiscal_year {
                    return Err(idempotency_fiscal_year_mismatch());
                }
                return ensure_accountant_package_files(
                    pool,
                    workspace_id,
                    input,
                    &idempotency_key,
                    &cached,
                )
                .await;
            }
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    }

    record_event(
        pool,
        workspace_id,
        "accountant_package_export_create",
        "accountant_package",
        Some(&idempotency_key),
        &serde_json::json!({
            "packagePath": summary.package_path,
            "fiscalYear": input.fiscal_year,
        })
        .to_string(),
    )
    .await?;

    publish_accountant_summary(
        pool,
        workspace_id,
        &exports_path,
        &database_path,
        summary,
        input.export_directory.as_deref(),
    )
    .await
}

pub async fn accountant_package_import_validate(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &AccountantPackageImportValidateInput,
) -> Result<AccountantPackageValidateSummary, AppError> {
    let (exports_path, database_path): (String, String) = sqlx::query_as(
        r#"
        SELECT exports_path, database_path FROM workspaces WHERE id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;

    let manifest_path =
        resolve_package_path(&input.package_path, &exports_path, &database_path)?;
    if !manifest_path.exists() {
        return Ok(AccountantPackageValidateSummary {
            valid: false,
            workspace_id: workspace_id.to_string(),
            workspace_name: String::new(),
            fiscal_year: 0,
            entry_count: 0,
            manual_fallback_hint: "Package manifest not found. Use SIE export or workspace backup for manual handoff.".to_string(),
        });
    }

    let manifest_text = fs::read_to_string(&manifest_path).map_err(AppError::from)?;
    let manifest: AccountantPackageManifest =
        serde_json::from_str(&manifest_text).map_err(|_| {
            AppError::validation("Invalid accountant package manifest", "packagePath")
        })?;

    if manifest.version != MANIFEST_VERSION {
        return Ok(AccountantPackageValidateSummary {
            valid: false,
            workspace_id: manifest.workspace_id,
            workspace_name: manifest.workspace_name,
            fiscal_year: manifest.fiscal_year,
            entry_count: manifest.entries.len(),
            manual_fallback_hint: "Unsupported package version. Export a new accountant package from Settings.".to_string(),
        });
    }

    let package_root = manifest_path
        .parent()
        .ok_or_else(|| AppError::validation("Invalid package path", "packagePath"))?;

    for entry in &manifest.entries {
        let file_path = safe_join_under(package_root, &entry.relative_path, "packagePath")?;
        if file_path.exists() {
            ensure_path_within_root(&file_path, package_root, "packagePath")?;
        }
        if !file_path.exists() {
            return Ok(AccountantPackageValidateSummary {
                valid: false,
                workspace_id: manifest.workspace_id.clone(),
                workspace_name: manifest.workspace_name.clone(),
                fiscal_year: manifest.fiscal_year,
                entry_count: manifest.entries.len(),
                manual_fallback_hint: format!(
                    "Missing entry {}. Re-export the accountant package or attach ledger.sie manually.",
                    entry.relative_path
                ),
            });
        }
        let (hash, _) = hash_file(&file_path, Some(MAX_VALIDATE_HASH_BYTES))?;
        if hash != entry.sha256 {
            return Ok(AccountantPackageValidateSummary {
                valid: false,
                workspace_id: manifest.workspace_id.clone(),
                workspace_name: manifest.workspace_name.clone(),
                fiscal_year: manifest.fiscal_year,
                entry_count: manifest.entries.len(),
                manual_fallback_hint: "Checksum mismatch. Re-export the package; import does not modify the live ledger.".to_string(),
            });
        }
    }

    let computed = hash_string(&serde_json::json!({
        "version": manifest.version,
        "workspaceId": manifest.workspace_id,
        "workspaceName": manifest.workspace_name,
        "fiscalYear": manifest.fiscal_year,
        "createdAt": manifest.created_at,
        "entries": manifest.entries,
    })
    .to_string());

    let valid = computed == manifest.manifest_sha256
        && manifest.workspace_id == workspace_id;
    let workspace_mismatch = manifest.workspace_id != workspace_id;

    record_event(
        pool,
        workspace_id,
        "accountant_package_import_validate",
        "accountant_package",
        Some(&manifest.workspace_id),
        &serde_json::json!({ "valid": valid, "packagePath": input.package_path }).to_string(),
    )
    .await?;

    Ok(AccountantPackageValidateSummary {
        valid,
        workspace_id: manifest.workspace_id,
        workspace_name: manifest.workspace_name,
        fiscal_year: manifest.fiscal_year,
        entry_count: manifest.entries.len(),
        manual_fallback_hint: if valid {
            "Package validated. Review files manually; live ledger import is not performed in v1.".to_string()
        } else if workspace_mismatch {
            "Package belongs to a different workspace. Export a package from this workspace or open the source workspace.".to_string()
        } else {
            "Manifest checksum mismatch. Use SIE export for manual accountant handoff.".to_string()
        },
    })
}
