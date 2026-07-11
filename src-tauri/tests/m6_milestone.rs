use oppenbokforing_desktop_lib::{
    accountant_package::{
        self, AccountantPackageExportCreateInput, AccountantPackageImportValidateInput,
    },
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    integrations,
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceIssueInput, InvoiceLineInput,
    },
    profiles::{self, BusinessProfileSaveInput, TaxProfileSaveInput, VatProfileSaveInput},
    settings::{self, WorkspaceSettingsSaveInput},
    sie::{self, SieExportCreateInput},
    state::load_golden_scenario,
    workspace::ensure_workspace_ready,
};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_workspace_with_invoice(
    dir: &tempfile::TempDir,
) -> (sqlx::SqlitePool, String, std::path::PathBuf) {
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
    .bind("M6 fixture workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    profiles::save_business_profile(
        &pool,
        &workspace_id,
        &BusinessProfileSaveInput {
            business_name: "M6 Test Firma".to_string(),
            owner_name: "Test Owner".to_string(),
            residency_country: Some("SE".to_string()),
            sni_code: None,
        },
    )
    .await
    .expect("business profile");

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "f_skatt".to_string(),
            expected_business_profit_minor: Some(500_000),
            expected_salary_income_minor: Some(0),
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

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Customer AB".to_string(),
            org_number: None,
            email: Some("customer@example.com".to_string()),
        },
    )
    .await
    .expect("customer");

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id.clone(),
            due_date: Some("2026-04-15".to_string()),
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

    invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "m6-invoice-issue".to_string(),
            issue_date: Some("2026-03-15".to_string()),
        },
    )
    .await
    .expect("issue");

    (pool, workspace_id, data_dir)
}

#[tokio::test]
async fn m6_workspace_settings_table_exists() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace_with_invoice(&dir).await;

    let settings = settings::workspace_settings_get(&pool, &workspace_id)
        .await
        .expect("settings");
    assert_eq!(settings.locale, "sv");
    assert!(!settings.updater_enabled);
}

#[tokio::test]
async fn m6_locale_persists_after_save() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace_with_invoice(&dir).await;

    let saved = settings::workspace_settings_save(
        &pool,
        &workspace_id,
        &WorkspaceSettingsSaveInput {
            locale: "sv".to_string(),
            updater_enabled: Some(false),
            default_export_directory: None,
            default_backup_directory: None,
            simple_mode: None,
        },
    )
    .await
    .expect("save sv");

    assert_eq!(saved.locale, "sv");

    let reloaded = settings::workspace_settings_get(&pool, &workspace_id)
        .await
        .expect("reload");
    assert_eq!(reloaded.locale, "sv");
}

#[tokio::test]
async fn m6_sie_export_ledger_fixture() {
    let dir = tempdir().expect("tempdir");
    let scenario = load_golden_scenario("sie-export-ledger");
    let expected = scenario.expected.as_object().expect("expected");
    let (pool, workspace_id, data_dir) = setup_workspace_with_invoice(&dir).await;

    let export = sie::sie_export_create(
        &pool,
        &workspace_id,
        &SieExportCreateInput {
            fiscal_year: 2026,
            idempotency_key: "sie-export-ledger".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("sie export");

    assert_eq!(export.fiscal_year, 2026);
    assert!(export.voucher_count >= 1);
    assert_eq!(
        expected["exportStoredLocally"].as_bool(),
        Some(!export.export_path.is_empty())
    );

    let sie_path = data_dir.join("exports").join(&export.export_path);
    let content = fs::read_to_string(&sie_path).expect("sie file");
    assert!(content.contains("#SIETYP 4"));
    assert!(content.contains("#FORMAT PC8"));
    assert!(content.contains("#VALUTA SEK"));
    assert!(content.contains("#FNAMN \"M6 Test Firma\""));
    assert!(content.contains("#VER "));
    assert!(content.contains("#TRANS ") && content.contains("{}"));

    let retry = sie::sie_export_create(
        &pool,
        &workspace_id,
        &SieExportCreateInput {
            fiscal_year: 2026,
            idempotency_key: "sie-export-ledger".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("idempotent retry");
    assert_eq!(retry.export_path, export.export_path);
    assert_eq!(
        expected["idempotentRetry"].as_bool(),
        Some(retry.export_path == export.export_path)
    );
}

#[tokio::test]
async fn m6_accountant_package_export_and_validate() {
    let dir = tempdir().expect("tempdir");
    let scenario = load_golden_scenario("sie-export-ledger");
    let expected = scenario.expected.as_object().expect("expected");
    let (pool, workspace_id, _) = setup_workspace_with_invoice(&dir).await;

    let package = accountant_package::accountant_package_export_create(
        &pool,
        &workspace_id,
        &AccountantPackageExportCreateInput {
            fiscal_year: 2026,
            idempotency_key: "accountant-package".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("accountant export");

    assert!(package.manifest.entries.iter().any(|e| e.entry_type == "sie"));
    assert_eq!(
        expected["accountantPackageIncludesSie"].as_bool(),
        Some(true)
    );

    let validation = accountant_package::accountant_package_import_validate(
        &pool,
        &workspace_id,
        &AccountantPackageImportValidateInput {
            package_path: package.package_path.clone(),
        },
    )
    .await
    .expect("validate");
    assert!(validation.valid);
    assert!(validation.manual_fallback_hint.contains("manual"));
}

#[tokio::test]
async fn m6_integrations_degrade_to_manual_fallback() {
    let scenario = load_golden_scenario("sie-export-ledger");
    let expected = scenario.expected.as_object().expect("expected");

    let status = integrations::status_response();
    assert!(!status.open_banking.available);
    assert!(!status.bankid.available);
    assert!(status.open_banking.manual_fallback_hint.contains("CSV")
        || status.open_banking.manual_fallback_hint.contains("SIE"));
    assert_eq!(
        expected["integrationManualFallback"].as_bool(),
        Some(!status.open_banking.available && !status.bankid.available)
    );
}

#[tokio::test]
async fn m6_settings_preserves_updater_when_omitted() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace_with_invoice(&dir).await;

    settings::workspace_settings_save(
        &pool,
        &workspace_id,
        &WorkspaceSettingsSaveInput {
            locale: "en".to_string(),
            updater_enabled: Some(true),
            default_export_directory: None,
            default_backup_directory: None,
            simple_mode: None,
        },
    )
    .await
    .expect("enable updater");

    let saved = settings::workspace_settings_save(
        &pool,
        &workspace_id,
        &WorkspaceSettingsSaveInput {
            locale: "sv".to_string(),
            updater_enabled: None,
            default_export_directory: None,
            default_backup_directory: None,
            simple_mode: None,
        },
    )
    .await
    .expect("change locale only");

    assert_eq!(saved.locale, "sv");
    assert!(saved.updater_enabled);
}

#[tokio::test]
async fn m6_sie_idempotency_rejects_different_fiscal_year() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace_with_invoice(&dir).await;

    sie::sie_export_create(
        &pool,
        &workspace_id,
        &SieExportCreateInput {
            fiscal_year: 2026,
            idempotency_key: "sie-year-mismatch".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("first export");

    let err = sie::sie_export_create(
        &pool,
        &workspace_id,
        &SieExportCreateInput {
            fiscal_year: 2025,
            idempotency_key: "sie-year-mismatch".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect_err("should reject fiscal year mismatch");

    assert_eq!(err.code, "validation_error");
}

#[tokio::test]
async fn m6_validate_rejects_parent_directory_in_package_path() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace_with_invoice(&dir).await;

    let package = accountant_package::accountant_package_export_create(
        &pool,
        &workspace_id,
        &AccountantPackageExportCreateInput {
            fiscal_year: 2026,
            idempotency_key: "package-traversal".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("export package");

    let traversal_path = format!("../{}", package.package_path);
    let err = accountant_package::accountant_package_import_validate(
        &pool,
        &workspace_id,
        &AccountantPackageImportValidateInput {
            package_path: traversal_path,
        },
    )
    .await
    .expect_err("parent directory segments should be rejected");

    assert_eq!(err.code, "validation_error");
}

#[tokio::test]
async fn m6_sie_export_regenerates_missing_file_on_retry() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, data_dir) = setup_workspace_with_invoice(&dir).await;

    let export = sie::sie_export_create(
        &pool,
        &workspace_id,
        &SieExportCreateInput {
            fiscal_year: 2026,
            idempotency_key: "sie-export-heal".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("sie export");

    let sie_path = data_dir.join("exports").join(&export.export_path);
    fs::remove_file(&sie_path).expect("delete sie file");

    let healed = sie::sie_export_create(
        &pool,
        &workspace_id,
        &SieExportCreateInput {
            fiscal_year: 2026,
            idempotency_key: "sie-export-heal".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("idempotent heal");

    assert_eq!(healed.export_path, export.export_path);
    assert!(sie_path.is_file());
}

#[tokio::test]
async fn m6_validate_rejects_missing_absolute_package_path() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace_with_invoice(&dir).await;

    let err = accountant_package::accountant_package_import_validate(
        &pool,
        &workspace_id,
        &AccountantPackageImportValidateInput {
            package_path: "/tmp/manifest.json".to_string(),
        },
    )
    .await
    .expect_err("missing absolute paths should be rejected");

    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("not found"));
}

#[tokio::test]
async fn m6_settings_rejects_invalid_locale() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace_with_invoice(&dir).await;

    let err = settings::workspace_settings_save(
        &pool,
        &workspace_id,
        &WorkspaceSettingsSaveInput {
            locale: "de".to_string(),
            updater_enabled: None,
            default_export_directory: None,
            default_backup_directory: None,
            simple_mode: None,
        },
    )
    .await
    .expect_err("invalid locale");

    assert_eq!(err.code, "validation_error");
}
