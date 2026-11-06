//! Restore genuine revision-47369a1 native rows, not newly generated issuance.
//! This fixture is historical engineering evidence, not cash-case authorization.
use std::{borrow::Cow, collections::BTreeMap, path::PathBuf};

use argon2::password_hash::Encoding;
use oppenbokforing_desktop_lib::db::open_existing_workspace;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use tempfile::{tempdir, TempDir};

pub const OLD_RULE: &str = "rv-2026-domestic-service25-exact-v1";
pub const NEW_RULE: &str = "rv-2026-method-recognition-v2";
pub const FINGERPRINT: &str = "f9b6933df5ff7e7f643c8e72f3eeef843918c9db80d9701dbb05ea44f773da5d";
pub const PDF_HASH: &str = "46ead1a55e6d3081faace1f6f12259aefc4de79207832361cd183129ecd8c292";
const ARCHIVE: &str =
    include_str!("../../../fixtures/historical-regressions/invoice-v1-native-2026.json");
const SCHEMA_21: i64 = 202611040001;
const SCHEMA_22: i64 = 202611040002;

pub type History = BTreeMap<String, Vec<Value>>;

pub struct NativeFixture {
    pub dir: TempDir,
    pub database: PathBuf,
    pub workspace: String,
    pub invoice: String,
    pub document: String,
    pub archive: Value,
    pub pdf: Vec<u8>,
    pub pool: SqlitePool,
    pub old_rules: History,
    pub migrations_21: Vec<Value>,
    pub history_21: History,
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn sorted(mut rows: Vec<Value>) -> Vec<Value> {
    rows.sort_by_key(Value::to_string);
    rows
}

// Compare every column, preserving raw JSON TEXT as strings (never parse/reformat it).
// Only trusted fixture/table identifiers are interpolated; all row values are bound.
pub async fn rows(pool: &SqlitePool, table: &str, predicate: &str) -> Vec<Value> {
    let columns = sqlx::query(&format!("PRAGMA table_info(\"{table}\")"))
        .fetch_all(pool)
        .await
        .unwrap();
    assert!(!columns.is_empty(), "expected table {table}");
    let fields = columns
        .iter()
        .map(|column| {
            let name: String = column.get("name");
            // SQLx migration checksums are the only BLOBs compared here.
            let value = if column.get::<String, _>("type") == "BLOB" {
                format!("hex(\"{name}\")")
            } else {
                format!("\"{name}\"")
            };
            format!("'{name}',{value}")
        })
        .collect::<Vec<_>>()
        .join(",");
    let raw: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT json_object({fields}) FROM \"{table}\" {predicate}"
    ))
    .fetch_all(pool)
    .await
    .unwrap();
    sorted(
        raw.iter()
            .map(|raw| serde_json::from_str(raw).unwrap())
            .collect(),
    )
}

pub async fn history(pool: &SqlitePool, archive: &Value) -> History {
    let mut result = BTreeMap::new();
    for table in archive["tables"].as_object().unwrap().keys() {
        result.insert(table.clone(), rows(pool, table, "").await);
    }
    result.insert(
        "invoice_snapshot_recovery_requirements".into(),
        rows(pool, "invoice_snapshot_recovery_requirements", "").await,
    );
    result
}

pub async fn old_rules(pool: &SqlitePool) -> History {
    let predicate = "WHERE rule_version_id IN ('rv-2025-year-end','rv-2026-active','rv-2026-domestic-service25-exact-v1')";
    BTreeMap::from([
        ("tax_rules".into(), rows(pool, "tax_rules", predicate).await),
        (
            "rule_versions".into(),
            rows(pool, "rule_versions", "WHERE id IN ('rv-2025-year-end','rv-2026-active','rv-2026-domestic-service25-exact-v1')").await,
        ),
    ])
}

async fn insert_row(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, table: &str, row: &Value) {
    let row = row.as_object().unwrap();
    let names = row
        .keys()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(",");
    let placeholders = vec!["?"; row.len()].join(",");
    let sql = format!("INSERT INTO \"{table}\" ({names}) VALUES ({placeholders})");
    let mut query = sqlx::query(&sql);
    for value in row.values() {
        query = match value {
            Value::Null => query.bind(Option::<String>::None),
            Value::String(value) => query.bind(value),
            Value::Number(value) => query.bind(value.as_i64().expect("archived SQL INTEGER")),
            _ => panic!("archive SQL values must be scalar, got {value}"),
        };
    }
    query
        .execute(&mut **tx)
        .await
        .unwrap_or_else(|error| panic!("restore {table}: {error}"));
}

impl NativeFixture {
    pub async fn schema_21() -> Self {
        assert_eq!(
            sha256(ARCHIVE.as_bytes()),
            "f8803465317a8b13c93cfcd88151b32fbd0aade5df25d350a1d7d3d7db8b12df"
        );
        let archive: Value = serde_json::from_str(ARCHIVE).unwrap();
        assert_eq!(archive["originalContextFingerprint"], FINGERPRINT);
        assert_eq!(
            archive["sourceRevision"],
            "47369a1586518451e70a537c70011f4159fbbee9"
        );
        let dir = tempdir().unwrap();
        let database = dir.path().join("workspace.sqlite");
        std::fs::create_dir_all(dir.path().join("documents/objects")).unwrap();
        std::fs::create_dir_all(dir.path().join("exports")).unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&database)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        // Run a real prior schema with SQLx bookkeeping. Never apply latest then undo it.
        let mut migrator = Migrator::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("migrations")
                .as_path(),
        )
        .await
        .unwrap();
        migrator.migrations = Cow::Owned(
            migrator
                .iter()
                .filter(|migration| migration.version <= SCHEMA_21)
                .cloned()
                .collect(),
        );
        assert_eq!(migrator.iter().count(), 21);
        migrator.run(&pool).await.unwrap();
        assert_eq!(rows(&pool, "_sqlx_migrations", "").await.len(), 21);
        let cash_tables: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('cash_business_origins','cash_invoice_issues','cash_receipt_recognitions')").fetch_one(&pool).await.unwrap();
        assert_eq!(
            cash_tables, 0,
            "fixture must genuinely start before cash schema"
        );

        // Migration 21 already seeds the genuine archived rule payloads. Assert exact
        // original TEXT, rather than replacing old rules with a current approximation.
        let payloads: Vec<String> = sqlx::query_scalar("SELECT json_object('rule_version_id',rule_version_id,'family',family,'key',key,'value_json',value_json) FROM tax_rules").fetch_all(&pool).await.unwrap();
        assert_eq!(
            sorted(
                payloads
                    .iter()
                    .map(|raw| serde_json::from_str(raw).unwrap())
                    .collect()
            ),
            sorted(archive["originalRulePayloads"].as_array().unwrap().clone())
        );
        let old_rules = old_rules(&pool).await;
        let migrations_21 = rows(&pool, "_sqlx_migrations", "").await;
        let workspace = archive["tables"]["workspaces"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let invoice = archive["tables"]["invoices"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let document = archive["tables"]["documents"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut expected = archive["tables"].as_object().unwrap().clone();
        // Map physical workspace locations only. Do not rewrite absolute paths inside
        // archived audit/replay/context strings, or read anything from original /tmp UAT.
        let workspace_row = &mut expected.get_mut("workspaces").unwrap()[0];
        workspace_row["database_path"] = database.to_string_lossy().as_ref().into();
        workspace_row["documents_path"] = dir
            .path()
            .join("documents")
            .to_string_lossy()
            .as_ref()
            .into();
        workspace_row["exports_path"] =
            dir.path().join("exports").to_string_lossy().as_ref().into();
        let mut tx = pool.begin().await.unwrap();
        // FK order, retaining all guards. Draft states exist only within this fixture
        // transaction; before upgrade every row must match the archive's final state.
        for table in [
            "workspaces",
            "sole_trader_profiles",
            "tax_profiles",
            "vat_profiles",
            "fiscal_years",
            "accounts",
            "invoice_sequences",
            "counterparties",
            "documents",
            "local_jobs",
            "vouchers",
            "invoices",
            "invoice_lines",
            "journal_lines",
            "invoice_issue_snapshots",
            "invoice_issue_validations",
            "audit_events",
            "workspace_settings",
            "csv_imports",
            "staged_transactions",
            "reconciliation_matches",
            "credit_notes",
        ] {
            for original in expected[table].as_array().unwrap() {
                let mut row = original.clone();
                if table == "invoices" || table == "vouchers" {
                    row["status"] = "draft".into();
                }
                insert_row(&mut tx, table, &row).await;
            }
        }
        for table in ["vouchers", "invoices"] {
            for row in expected[table].as_array().unwrap() {
                sqlx::query(&format!("UPDATE {table} SET status=?1 WHERE id=?2"))
                    .bind(row["status"].as_str().unwrap())
                    .bind(row["id"].as_str().unwrap())
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
        }
        tx.commit().await.unwrap();
        let history_21 = history(&pool, &archive).await;
        for (table, expected_rows) in &expected {
            assert_eq!(
                history_21[table],
                sorted(expected_rows.as_array().unwrap().clone()),
                "raw archive restore: {table}"
            );
        }
        assert!(sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty());
        let encoded = archive["documentBytesBase64"][&document].as_str().unwrap();
        // Reuse Argon2's existing standard-base64 decoder; it expects no '=' padding.
        let mut decoded = vec![0; encoded.len()];
        let pdf = Encoding::B64
            .decode(encoded.trim_end_matches('='), &mut decoded)
            .unwrap()
            .to_vec();
        assert_eq!(pdf.len(), 2328);
        assert_eq!(sha256(&pdf), PDF_HASH);
        std::fs::write(
            dir.path().join("documents").join(
                archive["tables"]["documents"][0]["object_path"]
                    .as_str()
                    .unwrap(),
            ),
            &pdf,
        )
        .unwrap();
        Self {
            dir,
            database,
            workspace,
            invoice,
            document,
            archive,
            pdf,
            pool,
            old_rules,
            migrations_21,
            history_21,
        }
    }

    pub async fn upgrade(&mut self) {
        self.pool.close().await;
        self.pool = open_existing_workspace(&self.database)
            .await
            .expect("public schema21 -> successor upgrade");
        self.assert_schema_22(&self.pool).await;
        assert_eq!(
            history(&self.pool, &self.archive).await,
            self.history_21,
            "upgrade must not rewrite native history or append accounting/audit/jobs"
        );
    }

    pub async fn assert_schema_22(&self, pool: &SqlitePool) {
        let migrations = rows(pool, "_sqlx_migrations", "").await;
        assert_eq!(
            migrations.len(),
            22,
            "public open must install migration 22"
        );
        assert_eq!(
            rows(pool, "_sqlx_migrations", "WHERE version <= 202611040001").await,
            self.migrations_21,
            "original SQLx migration checksums/states remain exact"
        );
        let success: i64 =
            sqlx::query_scalar("SELECT success FROM _sqlx_migrations WHERE version=?1")
                .bind(SCHEMA_22)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(success, 1);
        let active: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM rule_versions WHERE tax_year=2026 AND status='active'",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(active, vec![NEW_RULE]);
        let mut expected = self.old_rules.clone();
        for row in expected.get_mut("rule_versions").unwrap() {
            if row["id"] == OLD_RULE {
                assert_eq!(row["status"], "active");
                row["status"] = "retired".into();
            }
        }
        expected.insert(
            "rule_versions".into(),
            sorted(expected["rule_versions"].clone()),
        );
        assert_eq!(
            old_rules(pool).await,
            expected,
            "every old payload, ID and timestamp unchanged except v1 retirement"
        );
        for table in [
            "cash_business_origins",
            "cash_invoice_issues",
            "cash_receipt_recognitions",
        ] {
            assert!(
                rows(pool, table, "").await.is_empty(),
                "no fabricated native cash eligibility: {table}"
            );
        }
        assert!(sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(pool)
            .await
            .unwrap()
            .is_empty());
    }
}
