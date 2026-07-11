use std::path::{Component, Path, PathBuf};

use sqlx::SqlitePool;

use crate::{error::AppError, settings};

pub fn validate_user_directory(path: &str, field: &str) -> Result<PathBuf, AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Directory path is required", field));
    }
    if trimmed.contains("..") {
        return Err(AppError::validation("Directory path is invalid", field));
    }
    let parsed = PathBuf::from(trimmed);
    if !parsed.is_absolute() {
        return Err(AppError::validation(
            "Directory must be an absolute path",
            field,
        ));
    }
    if parsed
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::validation("Directory path is invalid", field));
    }
    Ok(parsed)
}

pub fn validate_backup_file_path(path: &str) -> Result<PathBuf, AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Backup file path is required", "backupPath"));
    }
    if trimmed.contains("..") {
        return Err(AppError::validation("Backup path is invalid", "backupPath"));
    }
    let parsed = PathBuf::from(trimmed);
    if !parsed.is_absolute() {
        return Err(AppError::validation(
            "Backup path must be an absolute path",
            "backupPath",
        ));
    }
    let extension = parsed
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if extension != crate::backup::BACKUP_FILE_EXTENSION {
        return Err(AppError::validation(
            "Backup file must use the .skatbackup extension",
            "backupPath",
        ));
    }
    if let Some(parent) = parsed.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::from)?;
    }
    Ok(parsed)
}

pub async fn resolve_export_directory(
    pool: &SqlitePool,
    workspace_id: &str,
    override_dir: Option<&str>,
) -> Result<Option<PathBuf>, AppError> {
    if let Some(dir) = override_dir.filter(|value| !value.trim().is_empty()) {
        return Ok(Some(validate_user_directory(dir, "exportDirectory")?));
    }

    let settings = settings::workspace_settings_get(pool, workspace_id).await?;
    if let Some(dir) = settings
        .default_export_directory
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(Some(validate_user_directory(dir, "defaultExportDirectory")?));
    }

    Ok(None)
}

pub async fn resolve_backup_destination(
    pool: &SqlitePool,
    workspace_id: &str,
    destination_path: Option<&str>,
) -> Result<Option<PathBuf>, AppError> {
    if let Some(dir) = destination_path.filter(|value| !value.trim().is_empty()) {
        return Ok(Some(validate_user_directory(dir, "destinationPath")?));
    }

    let settings = settings::workspace_settings_get(pool, workspace_id).await?;
    if let Some(dir) = settings
        .default_backup_directory
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(Some(validate_user_directory(dir, "defaultBackupDirectory")?));
    }

    Ok(None)
}

pub fn copy_file_to_directory(
    source: &Path,
    dest_dir: &Path,
    filename: &str,
) -> Result<PathBuf, AppError> {
    if !source.exists() {
        return Err(AppError::storage("Export source file is missing"));
    }
    std::fs::create_dir_all(dest_dir).map_err(AppError::from)?;
    let destination = dest_dir.join(filename);
    std::fs::copy(source, &destination).map_err(AppError::from)?;
    Ok(destination)
}

pub fn copy_directory_to_directory(source: &Path, dest_dir: &Path) -> Result<PathBuf, AppError> {
    if !source.is_dir() {
        return Err(AppError::storage("Export source directory is missing"));
    }
    std::fs::create_dir_all(dest_dir).map_err(AppError::from)?;
    let destination = dest_dir.join(
        source
            .file_name()
            .ok_or_else(|| AppError::storage("Invalid export source directory"))?,
    );
    if let (Ok(source_path), Ok(dest_parent)) = (source.canonicalize(), dest_dir.canonicalize()) {
        let destination_path = dest_parent.join(
            source
                .file_name()
                .ok_or_else(|| AppError::storage("Invalid export source directory"))?,
        );
        if let Ok(destination_path) = destination_path.canonicalize() {
            if source_path == destination_path {
                return Ok(source_path);
            }
        }
    }
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(AppError::from)?;
    }
    copy_dir_all(source, &destination)?;
    Ok(destination)
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub async fn publish_export_artifact(
    pool: &SqlitePool,
    workspace_id: &str,
    exports_path: &str,
    database_path: &str,
    workspace_relative_path: &str,
    export_directory_override: Option<&str>,
    filename: &str,
) -> Result<String, AppError> {
    let workspace_root =
        crate::workspace::resolve_workspace_exports_dir(exports_path, database_path)?;
    let source = workspace_root.join(workspace_relative_path);

    if let Some(dest_root) =
        resolve_export_directory(pool, workspace_id, export_directory_override).await?
    {
        let published = copy_file_to_directory(&source, &dest_root, filename)?;
        return Ok(published.to_string_lossy().to_string());
    }

    Ok(workspace_relative_path.replace('\\', "/"))
}

pub async fn publish_export_directory(
    pool: &SqlitePool,
    workspace_id: &str,
    exports_path: &str,
    database_path: &str,
    workspace_relative_path: &str,
    export_directory_override: Option<&str>,
) -> Result<String, AppError> {
    let workspace_root =
        crate::workspace::resolve_workspace_exports_dir(exports_path, database_path)?;
    let source = workspace_root.join(workspace_relative_path);

    if let Some(dest_root) =
        resolve_export_directory(pool, workspace_id, export_directory_override).await?
    {
        let published = copy_directory_to_directory(&source, &dest_root)?;
        return Ok(published.to_string_lossy().to_string());
    }

    Ok(workspace_relative_path.replace('\\', "/"))
}
