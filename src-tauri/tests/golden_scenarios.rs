use oppenbokforing_desktop_lib::{
    backup::{self, backup_plaintext_is_sqlite, BackupRestoreInput},
    compliance::{evaluate_scenario, ScenarioProfile, ScenarioTransaction},
    db::connect_workspace,
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    rules::{get_active_rule_version, ACTIVE_RULE_VERSION_ID},
    state::{fixtures_dir, load_golden_scenario},
};
use sqlx::Row;
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

fn list_fixture_ids() -> Vec<String> {
    fs::read_dir(fixtures_dir())
        .expect("fixtures directory missing")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            name.strip_suffix(".json")
                .filter(|id| *id != "schema")
                .map(str::to_string)
        })
        .collect()
}

#[test]
fn all_golden_fixtures_parse() {
    for id in list_fixture_ids() {
        let scenario = load_golden_scenario(&id);
        assert_eq!(scenario.id, id);
        assert!(!scenario.title.is_empty());
        assert!(scenario.expected.is_object());
        assert!(!scenario.sources.is_empty());
    }
}

#[tokio::test]
async fn m1_golden_scenarios_pass_compliance_engine() {
    let dir = tempdir().expect("tempdir");
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .expect("connect");

    for scenario_id in ["fa-skatt-salary-and-business", "vat-exempt-below-threshold", "vat-exempt-threshold-breach"] {
        let scenario = load_golden_scenario(scenario_id);
        let profile: ScenarioProfile =
            serde_json::from_value(scenario.profile).expect("profile");
        let transactions: Vec<ScenarioTransaction> = scenario
            .transactions
            .iter()
            .map(|value| serde_json::from_value(value.clone()))
            .collect::<Result<_, _>>()
            .expect("transactions");

        let result = evaluate_scenario(&pool, scenario_id, &profile, &transactions)
            .await
            .expect("evaluate");

        assert!(result.passed, "scenario {scenario_id} failed: {}", result.outcomes);

        for (key, expected) in scenario.expected.as_object().expect("expected object") {
            if result.outcomes.get(key).is_some() {
                assert_eq!(
                    &result.outcomes[key], expected,
                    "mismatch for {scenario_id}.{key}"
                );
            }
        }
    }
}

#[tokio::test]
async fn m1_backup_restore_preserves_rules_fixture() {
    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let data_dir = dir.path().join(&workspace_id);
    fs::create_dir_all(data_dir.join("documents")).expect("documents");
    fs::create_dir_all(data_dir.join("exports")).expect("exports");
    let database_path = data_dir.join("workspace.sqlite");
    let pool = connect_workspace(&database_path).await.expect("connect");

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("Backup fixture workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "fa_skatt".to_string(),
            expected_business_profit_minor: Some(18_000_000),
            expected_salary_income_minor: Some(42_000_000),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("tax profile");

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("vat profile");

    let backup = backup::create_backup_package(
        &pool,
        &workspace_id,
        &data_dir,
        &database_path,
        &data_dir.join("exports"),
        "fixture-passphrase-12",
        None,
    )
    .await
    .expect("backup");

    let scenario = load_golden_scenario("backup-restore-preserves-rules");
    let expected = scenario.expected.as_object().expect("expected");

    let backup_bytes = fs::read(&backup.backup_path).expect("read backup file");
    assert!(!backup_plaintext_is_sqlite(&backup_bytes));
    assert_eq!(
        expected["backupEncryptedAtRest"].as_bool(),
        Some(!backup_plaintext_is_sqlite(&backup_bytes))
    );

    assert_eq!(
        expected["backupIncludesDatabase"].as_bool(),
        Some(backup.manifest.entries.iter().any(|entry| entry.relative_path == "workspace.sqlite"))
    );
    assert_eq!(
        expected["backupIncludesDocuments"].as_bool(),
        Some(backup.manifest.entries.iter().any(|entry| entry.relative_path.starts_with("documents/")))
    );
    assert_eq!(
        expected["backupIncludesRuleVersions"].as_bool(),
        Some(backup.manifest.entries.iter().any(|entry| entry.relative_path == "rule_versions/"))
    );
    assert!(!backup.manifest.manifest_sha256.is_empty());

    let restore_root = dir.path().join("restored-workspaces");
    fs::create_dir_all(&restore_root).expect("restore root");

    let without_confirm = backup::restore_backup_package(
        &BackupRestoreInput {
            backup_path: backup.backup_path.clone(),
            confirm_overwrite: false,
            passphrase: "fixture-passphrase-12".to_string(),
        },
        &restore_root,
    )
    .await;
    assert!(without_confirm.is_err());
    assert_eq!(
        expected["restoreRequiresExplicitConfirmation"].as_bool(),
        Some(true)
    );

    let restored = backup::restore_backup_package(
        &BackupRestoreInput {
            backup_path: backup.backup_path,
            confirm_overwrite: true,
            passphrase: "fixture-passphrase-12".to_string(),
        },
        &restore_root,
    )
    .await
    .expect("restore");

    let restore_target = std::path::PathBuf::from(&restored.database_path);
    let restored_pool = connect_workspace(&restore_target).await.expect("restored pool");
    let preserved = backup::profiles_preserved_after_restore(&restored_pool, &restored.workspace_id)
        .await
        .expect("profile check");
    assert_eq!(expected["profilesPreservedAfterRestore"].as_bool(), Some(preserved));
}

#[tokio::test]
async fn m1_workspace_offline_reopen_fixture() {
    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let data_dir = dir.path().join(&workspace_id);
    fs::create_dir_all(data_dir.join("documents")).expect("documents");
    fs::create_dir_all(data_dir.join("exports")).expect("exports");
    let database_path = data_dir.join("workspace.sqlite");

    let pool = connect_workspace(&database_path).await.expect("connect");
    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("Offline reopen workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    let rule_before = get_active_rule_version(&pool)
        .await
        .expect("rule")
        .expect("active rule");
    assert_eq!(rule_before.id, ACTIVE_RULE_VERSION_ID);

    drop(pool);

    let reopened = connect_workspace(&database_path).await.expect("reopen");
    let row = sqlx::query(
        r#"
        SELECT id FROM workspaces WHERE database_path = ?1 LIMIT 1
        "#,
    )
    .bind(database_path.to_string_lossy().to_string())
    .fetch_optional(&reopened)
    .await
    .expect("workspace lookup")
    .expect("workspace persisted");

    let rule_after = get_active_rule_version(&reopened)
        .await
        .expect("rule after reopen")
        .expect("active rule after reopen");

    let scenario = load_golden_scenario("workspace-offline-reopen");
    let expected = scenario.expected.as_object().expect("expected");

    assert_eq!(expected["workspacePersisted"].as_bool(), Some(true));
    assert_eq!(row.get::<String, _>("id"), workspace_id);
    assert_eq!(expected["reopenWithoutNetwork"].as_bool(), Some(true));
    assert_eq!(expected["ruleVersionPreserved"].as_bool(), Some(rule_after.id == rule_before.id));
}

#[tokio::test]
async fn m2_credit_invoice_fixture() {
    use oppenbokforing_desktop_lib::{
        counterparties::{self, CounterpartyCreateInput},
        invoicing::{
            self, InvoiceCreateDraftInput, InvoiceCreditInput, InvoiceIssueInput, InvoiceLineInput,
        },
        ledger::{has_reversal_for_invoice, net_output_vat_minor, net_revenue_minor},
        profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
        workspace::ensure_workspace_ready,
    };

    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .expect("connect");

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("M2 credit invoice workspace")
    .bind(dir.path().join("workspace.sqlite").to_string_lossy().to_string())
    .bind(dir.path().join("documents").to_string_lossy().to_string())
    .bind(dir.path().join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace");

    ensure_workspace_ready(&pool, &workspace_id).await.expect("bootstrap");

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "f_skatt".to_string(),
            expected_business_profit_minor: Some(1_000_000),
            expected_salary_income_minor: Some(0),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("tax");

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("vat");

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Fixture Customer AB".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: Some("2026-02-28".to_string()),
            lines: vec![InvoiceLineInput {
                description: "Consulting".to_string(),
                quantity: 1,
                unit_price_minor: 1_000_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

    let issued = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "issue-2026-0001".to_string(),
            issue_date: Some("2026-01-15".to_string()),
        },
    )
    .await
    .expect("issue");

    assert_eq!(issued.invoice_number.as_deref(), Some("2026-0001"));
    assert_eq!(issued.status, "issued");

    let scenario = load_golden_scenario("credit-invoice-reversal");
    let expected = scenario.expected.as_object().expect("expected");

    let _credited = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id.clone(),
            idempotency_key: "credit-2026-0001".to_string(),
            reason: Some("Fixture correction".to_string()),
        },
    )
    .await
    .expect("credit");

    let immutable = invoicing::original_invoice_immutable(
        &pool,
        &workspace_id,
        &issued.id,
        1_000_000,
        250_000,
        "2026-0001",
    )
    .await
    .expect("immutable");

    let reversal = has_reversal_for_invoice(&pool, &workspace_id, &issued.id)
        .await
        .expect("reversal");
    let revenue = net_revenue_minor(&pool, &workspace_id).await.expect("revenue");
    let vat = net_output_vat_minor(&pool, &workspace_id).await.expect("vat");

    assert_eq!(expected["originalInvoiceImmutable"].as_bool(), Some(immutable));
    assert_eq!(expected["creditNoteRequired"].as_bool(), Some(true));
    assert_eq!(expected["reversalVoucherCreated"].as_bool(), Some(reversal));
    assert_eq!(expected["netRevenueMinor"].as_i64(), Some(revenue));
    assert_eq!(expected["netVatMinor"].as_i64(), Some(vat));

    let cached = invoicing::check_issue_idempotency(&pool, &workspace_id, "issue-2026-0001")
        .await
        .expect("check")
        .expect("cached issue");
    assert_eq!(cached.invoice_number.as_deref(), Some("2026-0001"));

    let cached_credit = invoicing::check_credit_idempotency(&pool, &workspace_id, "credit-2026-0001")
        .await
        .expect("credit check")
        .expect("cached credit");
    assert_eq!(cached_credit.invoice_kind, "credit_note");
}
