//! Real prior-schema upgrade and encrypted persistence of issuance provenance.
pub mod common;

use std::{borrow::Cow, collections::BTreeMap, path::PathBuf};

use oppenbokforing_desktop_lib::{
    backup::{self, BackupRestoreInput},
    counterparties::{self, CounterpartyCreateInput},
    db::{connect_workspace, open_existing_workspace},
    documents, invoicing,
    invoicing::{
        InvoiceCreateDraftInput, InvoiceIssueInput, InvoiceIssueValidationGetInput,
        InvoiceLineInput,
    },
    jobs,
    profiles::{self, BusinessProfileSaveInput, TaxProfileSaveInput, VatProfileSaveInput},
    workspace::ensure_workspace_ready,
};
use serde_json::Value;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use tempfile::{tempdir, TempDir};
use uuid::Uuid;

const GATE_MIGRATION: i64 = 202611040001;
const RULE: &str = "rv-2026-method-recognition-v2";
const PASSPHRASE: &str = "eligibility-persistence-test-only";

struct Fixture {
    dir: TempDir,
    database: PathBuf,
    workspace: String,
    customer: String,
    pool: SqlitePool,
}

impl Fixture {
    async fn new(prior_schema: bool) -> Self {
        let dir = tempdir().expect("independent fixture directory");
        let database = dir.path().join("workspace.sqlite");
        std::fs::create_dir_all(dir.path().join("documents")).unwrap();
        std::fs::create_dir_all(dir.path().join("exports")).unwrap();
        let pool = if prior_schema {
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
            // Use SQLx's real runner/bookkeeping, never apply then undo migration 21.
            let mut migrator = Migrator::new(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("migrations")
                    .as_path(),
            )
            .await
            .unwrap();
            migrator.migrations = Cow::Owned(
                migrator
                    .migrations
                    .iter()
                    .filter(|migration| migration.version < GATE_MIGRATION)
                    .cloned()
                    .collect(),
            );
            assert_eq!(migrator.iter().count(), 20);
            migrator.run(&pool).await.unwrap();
            pool
        } else {
            connect_workspace(&database).await.unwrap()
        };
        let workspace = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES(?1,'Persistence test',?2,?3,?4)")
            .bind(&workspace).bind(database.to_string_lossy().as_ref())
            .bind(dir.path().join("documents").to_string_lossy().as_ref())
            .bind(dir.path().join("exports").to_string_lossy().as_ref())
            .execute(&pool).await.unwrap();
        ensure_workspace_ready(&pool, &workspace).await.unwrap();
        profiles::save_business_profile(
            &pool,
            &workspace,
            &BusinessProfileSaveInput {
                business_name: "Persistence test firma".into(),
                owner_name: "Test owner".into(),
                residency_country: Some("SE".into()),
                sni_code: None,
            },
        )
        .await
        .unwrap();
        profiles::save_tax_profile(
            &pool,
            &workspace,
            &TaxProfileSaveInput {
                tax_status: "f_skatt".into(),
                expected_business_profit_minor: Some(1_000_000),
                expected_salary_income_minor: Some(0),
                active_rule_year: Some(2026),
            },
        )
        .await
        .unwrap();
        profiles::save_vat_profile(
            &pool,
            &workspace,
            &VatProfileSaveInput {
                vat_status: "registered".into(),
                reporting_period: "yearly".into(),
                accounting_method: "invoice_method".into(),
                voluntary_registration_date: None,
                vat_filing_deadline_regime: Some("annual_may_12".into()),
            },
        )
        .await
        .unwrap();
        let customer = counterparties::create_counterparty(
            &pool,
            &workspace,
            &CounterpartyCreateInput {
                kind: "customer".into(),
                name: "Persistence buyer".into(),
                email: None,
                org_number: None,
            },
        )
        .await
        .unwrap()
        .id;
        Self {
            dir,
            database,
            workspace,
            customer,
            pool,
        }
    }

    async fn draft_input(&self, key: &str) -> InvoiceIssueInput {
        let draft = invoicing::create_draft(
            &self.pool,
            &self.workspace,
            &InvoiceCreateDraftInput {
                counterparty_id: self.customer.clone(),
                due_date: Some("2027-01-30".into()),
                lines: vec![InvoiceLineInput {
                    description: "Completed ordinary domestic service".into(),
                    quantity: 1,
                    unit_price_minor: 100_000,
                    vat_rate: 0.25,
                    account_number: Some("3041".into()),
                }],
            },
        )
        .await
        .unwrap();
        InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: key.into(),
            issue_date: Some("2026-12-31".into()),
            eligibility_facts: None,
            review_fingerprint: None,
        }
    }

    async fn reopen(&mut self) {
        self.pool.close().await;
        self.pool = open_existing_workspace(&self.database).await.unwrap();
    }
}

// Compare every column, including IDs, timestamps, replay payload bytes and NULLs.
// All tables passed here have scalar (non-BLOB) columns.
async fn table_rows(pool: &SqlitePool, table: &str, predicate: &str) -> Vec<String> {
    let columns = sqlx::query(&format!("PRAGMA table_info(\"{table}\")"))
        .fetch_all(pool)
        .await
        .unwrap();
    assert!(!columns.is_empty(), "expected table {table}");
    let fields = columns
        .iter()
        .map(|column| {
            let name: String = column.get("name");
            format!("'{name}',\"{name}\"")
        })
        .collect::<Vec<_>>()
        .join(",");
    sqlx::query_scalar(&format!(
        "SELECT json_object({fields}) AS row_json FROM \"{table}\" {predicate} ORDER BY row_json"
    ))
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn history(pool: &SqlitePool) -> BTreeMap<String, Vec<String>> {
    let mut result = BTreeMap::new();
    for table in [
        "invoices",
        "invoice_lines",
        "invoice_sequences",
        "invoice_issue_snapshots",
        "invoice_snapshot_recovery_requirements",
        "vouchers",
        "journal_lines",
        "audit_events",
        "local_jobs",
    ] {
        result.insert(table.into(), table_rows(pool, table, "").await);
    }
    result
}

async fn migration_bookkeeping(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT json_object('version',version,'description',description,'installedOn',installed_on,'success',success,'checksum',hex(checksum),'executionTime',execution_time) FROM _sqlx_migrations WHERE version < 202611040001 ORDER BY version")
        .fetch_all(pool).await.unwrap()
}

async fn validation(pool: &SqlitePool, workspace: &str, invoice: &str) -> Option<Value> {
    invoicing::invoice_issue_validation_get(
        pool,
        workspace,
        &InvoiceIssueValidationGetInput {
            invoice_id: invoice.into(),
        },
    )
    .await
    .unwrap()
    .map(|record| serde_json::to_value(record).unwrap())
}

async fn assert_validation_immutable(pool: &SqlitePool, invoice: &str) {
    let before = table_rows(pool, "invoice_issue_validations", "").await;
    for statement in [
        "UPDATE invoice_issue_validations SET facts_json=facts_json WHERE invoice_id=?1",
        "DELETE FROM invoice_issue_validations WHERE invoice_id=?1",
    ] {
        let error = sqlx::query(statement)
            .bind(invoice)
            .execute(pool)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("invoice issue validation is immutable"),
            "{error}"
        );
        assert_eq!(
            table_rows(pool, "invoice_issue_validations", "").await,
            before
        );
    }
}

#[tokio::test]
async fn real_twenty_to_twenty_one_upgrade_preserves_history_without_backfilling_eligibility() {
    let mut fixture = Fixture::new(true).await;
    assert_eq!(migration_bookkeeping(&fixture.pool).await.len(), 20);
    let absent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name='invoice_issue_validations'",
    )
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(absent, 0);
    let input = fixture.draft_input("pre-gate-history").await;
    let historical = common::seed_historical_invoice(&fixture.pool, &fixture.workspace, &input)
        .await
        .unwrap();
    assert_eq!(historical.status, "issued");
    assert!(historical.voucher_id.is_some());
    let history_before = history(&fixture.pool).await;
    for table in [
        "invoices",
        "invoice_lines",
        "vouchers",
        "journal_lines",
        "audit_events",
        "local_jobs",
        "invoice_issue_snapshots",
    ] {
        assert!(!history_before[table].is_empty(), "seeded {table}");
    }
    let rules_before = table_rows(&fixture.pool, "tax_rules", "").await;
    assert!(!rules_before.is_empty());
    let versions_before: Vec<String> = sqlx::query_scalar("SELECT json_remove(json_object('id',id,'tax_year',tax_year,'effective_from',effective_from,'effective_to',effective_to,'source_url',source_url,'checksum',checksum,'created_at',created_at,'status',status),'$.status') FROM rule_versions ORDER BY id")
        .fetch_all(&fixture.pool).await.unwrap();
    let bookkeeping_before = migration_bookkeeping(&fixture.pool).await;

    fixture.reopen().await; // Current public open applies the pending migration normally.

    assert_eq!(history(&fixture.pool).await, history_before);
    assert_eq!(
        migration_bookkeeping(&fixture.pool).await,
        bookkeeping_before
    );
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success=1")
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(applied, 22);
    assert_eq!(
        table_rows(
            &fixture.pool,
            "tax_rules",
            &format!(
                "WHERE rule_version_id NOT IN ('{RULE}','rv-2026-domestic-service25-exact-v1')"
            )
        )
        .await,
        rules_before,
    );
    let versions_after: Vec<String> = sqlx::query_scalar("SELECT json_remove(json_object('id',id,'tax_year',tax_year,'effective_from',effective_from,'effective_to',effective_to,'source_url',source_url,'checksum',checksum,'created_at',created_at,'status',status),'$.status') FROM rule_versions WHERE id != ?1 AND id != 'rv-2026-domestic-service25-exact-v1' ORDER BY id")
        .bind(RULE).fetch_all(&fixture.pool).await.unwrap();
    assert_eq!(
        versions_after, versions_before,
        "old versions may retire, not change payloads"
    );
    let status: String =
        sqlx::query_scalar("SELECT status FROM rule_versions WHERE id=?1 AND tax_year=2026")
            .bind(RULE)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert_eq!(status, "active");
    let calendar: String = sqlx::query_scalar("SELECT value_json FROM tax_rules WHERE rule_version_id=?1 AND family='vat' AND key='filing_deadline_schedules'")
        .bind(RULE).fetch_one(&fixture.pool).await.unwrap();
    let calendar: Value = serde_json::from_str(&calendar).unwrap();
    assert_eq!(calendar["schedules"]["annual_may_12"]["2026"], "2027-05-12");
    assert!(table_rows(&fixture.pool, "invoice_issue_validations", "")
        .await
        .is_empty());
    assert_eq!(
        serde_json::to_value(
            invoicing::get_invoice(&fixture.pool, &fixture.workspace, &historical.id)
                .await
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&historical).unwrap(),
    );
    assert!(
        validation(&fixture.pool, &fixture.workspace, &historical.id)
            .await
            .is_none()
    );
    let replay = invoicing::issue_invoice(&fixture.pool, &fixture.workspace, &input)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(replay).unwrap(),
        serde_json::to_value(&historical).unwrap()
    );
    assert_eq!(
        history(&fixture.pool).await,
        history_before,
        "completed replay changes no history"
    );

    let mut input = fixture.draft_input("new-post-upgrade-issue").await;
    input.eligibility_facts = Some(common::facts("2026-12-31"));
    let issued = common::issue_reviewed(&fixture.pool, &fixture.workspace, &input)
        .await
        .unwrap();
    let record = validation(&fixture.pool, &fixture.workspace, &issued.id)
        .await
        .unwrap();
    assert_eq!(
        record["facts"],
        serde_json::to_value(common::facts("2026-12-31")).unwrap()
    );
    assert_eq!(record["assessment"]["ruleVersionId"], RULE);
    assert_eq!(record["assessment"]["dueOn"], "2027-05-12");
    assert!(
        validation(&fixture.pool, &fixture.workspace, &historical.id)
            .await
            .is_none()
    );
    fixture.pool.close().await;
}

#[tokio::test]
async fn immutable_eligibility_survives_reopen_and_encrypted_backup_restore() {
    let mut fixture = Fixture::new(false).await;
    let legacy_input = fixture.draft_input("backup-legacy-history").await;
    let legacy = common::seed_historical_invoice(&fixture.pool, &fixture.workspace, &legacy_input)
        .await
        .unwrap();
    let mut input = fixture.draft_input("backup-reviewed-issue").await;
    input.eligibility_facts = Some(common::facts("2026-12-31"));
    let issued = common::issue_reviewed(&fixture.pool, &fixture.workspace, &input)
        .await
        .unwrap();
    assert_eq!(issued.status, "issued");
    let record = validation(&fixture.pool, &fixture.workspace, &issued.id)
        .await
        .unwrap();
    assert_eq!(
        record["facts"],
        serde_json::to_value(common::facts("2026-12-31")).unwrap()
    );
    assert_eq!(record["assessment"]["ruleVersionId"], RULE);
    for field in ["evidenceDigest", "reviewFingerprint"] {
        assert_eq!(record["assessment"][field].as_str().unwrap().len(), 64);
    }
    assert!(!record["recordedAt"].as_str().unwrap().is_empty());
    let raw_record = table_rows(&fixture.pool, "invoice_issue_validations", "").await;
    assert_validation_immutable(&fixture.pool, &issued.id).await;

    // Retain real PDFs using the public worker, not made-up document metadata.
    assert_eq!(
        jobs::process_pending_invoice_pdf_jobs(&fixture.pool, &fixture.workspace)
            .await
            .unwrap(),
        2
    );
    let issued = invoicing::get_invoice(&fixture.pool, &fixture.workspace, &issued.id)
        .await
        .unwrap();
    let legacy = invoicing::get_invoice(&fixture.pool, &fixture.workspace, &legacy.id)
        .await
        .unwrap();
    let document_id = issued
        .pdf_document_id
        .as_deref()
        .expect("retained issued PDF");
    let document =
        documents::verify_retained_document(&fixture.pool, &fixture.workspace, document_id)
            .await
            .unwrap();
    let pdf = std::fs::read(
        fixture
            .dir
            .path()
            .join("documents")
            .join(&document.object_path),
    )
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    fixture.reopen().await;
    assert_eq!(
        validation(&fixture.pool, &fixture.workspace, &issued.id).await,
        Some(record.clone())
    );
    assert_eq!(
        table_rows(&fixture.pool, "invoice_issue_validations", "").await,
        raw_record
    );
    assert!(validation(&fixture.pool, &fixture.workspace, &legacy.id)
        .await
        .is_none());
    assert_eq!(
        serde_json::to_value(
            invoicing::get_invoice(&fixture.pool, &fixture.workspace, &issued.id)
                .await
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&issued).unwrap()
    );
    assert_validation_immutable(&fixture.pool, &issued.id).await;

    let backup_dir = tempdir().unwrap();
    let backup = backup::create_backup_package(
        &fixture.pool,
        &fixture.workspace,
        fixture.dir.path(),
        &fixture.database,
        backup_dir.path(),
        PASSPHRASE,
        None,
    )
    .await
    .unwrap();
    let encrypted = std::fs::read(&backup.backup_path).unwrap();
    assert!(!backup::backup_plaintext_is_sqlite(&encrypted));
    assert!(backup
        .manifest
        .entries
        .iter()
        .any(|entry| entry.relative_path == format!("documents/{}", document.object_path)));
    fixture.pool.close().await;
    let restore_dir = tempdir().unwrap();
    let restored = backup::restore_backup_package(
        &BackupRestoreInput {
            backup_path: backup.backup_path,
            confirm_overwrite: true,
            passphrase: PASSPHRASE.into(),
        },
        restore_dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(restored.workspace_id, fixture.workspace);
    let restored_pool = open_existing_workspace(std::path::Path::new(&restored.database_path))
        .await
        .unwrap();
    assert_eq!(
        validation(&restored_pool, &restored.workspace_id, &issued.id).await,
        Some(record)
    );
    assert_eq!(
        table_rows(&restored_pool, "invoice_issue_validations", "").await,
        raw_record
    );
    for invoice in [&issued, &legacy] {
        assert_eq!(
            serde_json::to_value(
                invoicing::get_invoice(&restored_pool, &restored.workspace_id, &invoice.id)
                    .await
                    .unwrap()
            )
            .unwrap(),
            serde_json::to_value(invoice).unwrap()
        );
    }
    assert!(
        validation(&restored_pool, &restored.workspace_id, &legacy.id)
            .await
            .is_none()
    );
    assert_validation_immutable(&restored_pool, &issued.id).await;
    let restored_document =
        documents::verify_retained_document(&restored_pool, &restored.workspace_id, document_id)
            .await
            .unwrap();
    assert_eq!(
        serde_json::to_value(&restored_document).unwrap(),
        serde_json::to_value(&document).unwrap()
    );
    let restored_root = std::path::Path::new(&restored.database_path)
        .parent()
        .unwrap();
    assert_eq!(
        std::fs::read(
            restored_root
                .join("documents")
                .join(restored_document.object_path)
        )
        .unwrap(),
        pdf
    );
    restored_pool.close().await;
}
