use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

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

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/golden-scenarios")
}

pub fn load_golden_scenario(id: &str) -> GoldenScenario {
    let path = fixtures_dir().join(format!("{id}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read fixture {id}: {error}"));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("invalid fixture {id}: {error}"))
}
