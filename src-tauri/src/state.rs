use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::path::PathBuf;
use tokio::sync::Mutex;

use crate::error::{redacted_internal_from, AppError};

#[derive(Clone, Debug)]
pub struct WorkspaceContext {
    pub id: String,
    pub name: String,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub pool: SqlitePool,
}

pub struct AppState {
    pub current_workspace: Mutex<Option<WorkspaceContext>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current_workspace: Mutex::new(None),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GoldenScenario {
    pub id: String,
    pub title: String,
    pub profile: Value,
    pub transactions: Vec<Value>,
    pub expected: Value,
    pub sources: Vec<String>,
}

/// Scenario IDs accepted by the live `compliance_check_run` command.
/// Keep in sync with arms implemented in `compliance::evaluate_scenario`.
pub const ALLOWED_GOLDEN_SCENARIO_IDS: &[&str] = &[
    "fa-skatt-salary-and-business",
    "vat-exempt-below-threshold",
    "vat-exempt-threshold-breach",
];

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/golden-scenarios")
}

fn embedded_golden_scenario_json(id: &str) -> Option<&'static str> {
    match id {
        "fa-skatt-salary-and-business" => Some(include_str!(
            "../../fixtures/golden-scenarios/fa-skatt-salary-and-business.json"
        )),
        "vat-exempt-below-threshold" => Some(include_str!(
            "../../fixtures/golden-scenarios/vat-exempt-below-threshold.json"
        )),
        "vat-exempt-threshold-breach" => Some(include_str!(
            "../../fixtures/golden-scenarios/vat-exempt-threshold-breach.json"
        )),
        _ => None,
    }
}

pub fn is_allowed_golden_scenario_id(id: &str) -> bool {
    ALLOWED_GOLDEN_SCENARIO_IDS.contains(&id)
}

fn validate_scenario_id(id: &str) -> Result<&str, AppError> {
    let trimmed = id.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return Err(AppError::validation(
            "Invalid compliance scenario id",
            "scenarioId",
        ));
    }
    Ok(trimmed)
}

fn parse_golden_scenario(raw: &str) -> Result<GoldenScenario, AppError> {
    serde_json::from_str(raw).map_err(redacted_internal_from)
}

fn load_golden_scenario_from_disk(id: &str) -> Result<GoldenScenario, AppError> {
    let fixtures = fixtures_dir();
    let path = fixtures.join(format!("{id}.json"));
    let fixtures_canon = fixtures.canonicalize().map_err(|_| {
        AppError::validation("Golden scenario fixtures are unavailable", "scenarioId")
    })?;
    let path_canon = path.canonicalize().map_err(|_| {
        AppError::validation("Unknown compliance scenario id", "scenarioId")
    })?;
    if !path_canon.starts_with(&fixtures_canon) {
        return Err(AppError::validation(
            "Invalid compliance scenario id",
            "scenarioId",
        ));
    }
    let raw = std::fs::read_to_string(&path_canon).map_err(redacted_internal_from)?;
    parse_golden_scenario(&raw)
}

/// Load a golden scenario by id.
/// Prefer compile-time embeds for allowlisted compliance scenarios (packaged builds);
/// fall back to the fixtures directory for milestone/integration tests in a source checkout.
pub fn load_golden_scenario(id: &str) -> Result<GoldenScenario, AppError> {
    let id = validate_scenario_id(id)?;
    if let Some(raw) = embedded_golden_scenario_json(id) {
        return parse_golden_scenario(raw);
    }
    load_golden_scenario_from_disk(id)
}

/// Require an allowlisted scenario id before loading (used by the live Tauri command).
pub fn load_compliance_golden_scenario(id: &str) -> Result<GoldenScenario, AppError> {
    let id = validate_scenario_id(id)?;
    if !is_allowed_golden_scenario_id(id) {
        return Err(AppError::validation(
            "Unknown compliance scenario id",
            "scenarioId",
        ));
    }
    load_golden_scenario(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_golden_scenario_rejects_traversal() {
        let err = load_golden_scenario("../secrets").expect_err("traversal");
        assert_eq!(err.code, "validation_error");
    }

    #[test]
    fn load_compliance_rejects_unknown() {
        let err = load_compliance_golden_scenario("document-import-retention").expect_err("unknown");
        assert_eq!(err.code, "validation_error");
    }

    #[test]
    fn load_golden_scenario_embeds_fa_skatt() {
        let scenario = load_golden_scenario("fa-skatt-salary-and-business").expect("load");
        assert_eq!(scenario.id, "fa-skatt-salary-and-business");
    }

    #[test]
    fn load_golden_scenario_disk_fallback_for_tests() {
        let scenario = load_golden_scenario("document-import-retention").expect("disk");
        assert_eq!(scenario.id, "document-import-retention");
    }
}
