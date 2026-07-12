use std::{fs, path::{Path, PathBuf}};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{AppError, redacted_storage_from};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentWorkspaceEntry {
    pub id: String,
    pub name: String,
    pub database_path: String,
    pub last_opened_at: String,
}

fn recent_workspaces_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("recent_workspaces.json")
}

pub fn list_recent_workspaces(app_data_dir: &Path) -> Result<Vec<RecentWorkspaceEntry>, AppError> {
    let path = recent_workspaces_path(app_data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    let entries: Vec<RecentWorkspaceEntry> = serde_json::from_str(&raw)
        .map_err(redacted_storage_from)?;
    Ok(entries)
}

pub fn record_recent_workspace(
    app_data_dir: &Path,
    id: &str,
    name: &str,
    database_path: &str,
) -> Result<Vec<RecentWorkspaceEntry>, AppError> {
    fs::create_dir_all(app_data_dir)?;
    let mut entries = list_recent_workspaces(app_data_dir)?;
    entries.retain(|entry| entry.database_path != database_path);
    entries.insert(
        0,
        RecentWorkspaceEntry {
            id: id.to_string(),
            name: name.to_string(),
            database_path: database_path.to_string(),
            last_opened_at: Utc::now().to_rfc3339(),
        },
    );
    entries.truncate(10);
    let path = recent_workspaces_path(app_data_dir);
    fs::write(
        &path,
        serde_json::to_string_pretty(&entries)
            .map_err(redacted_storage_from)?,
    )?;
    Ok(entries)
}
