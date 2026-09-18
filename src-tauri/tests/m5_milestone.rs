use oppenbokforing_desktop_lib::{
    audit::record_event,
    db::connect_workspace,
    expenses::{self, ExpensePostInput},
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    state::load_golden_scenario,
    vat::{self, VatReturnApproveInput, VatReturnDraftCreateInput},
    workspace::ensure_workspace_ready,
    year_end::{self, YearEndPackageApproveInput, YearEndPackageCreateInput},
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_workspace(
    dir: &tempfile::TempDir,
    tax_status: &str,
) -> (sqlx::SqlitePool, String) {
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
    .bind("M5 fixture workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: tax_status.to_string(),
            expected_business_profit_minor: Some(18_000_000),
            expected_salary_income_minor: Some(48_000_000),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("tax profile");

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput { vat_status: "registered".to_string(),
        reporting_period: "yearly".to_string(),
        accounting_method: "invoice_method".to_string(),
        voluntary_registration_date: None, vat_filing_deadline_regime: Some("annual_may_12".to_string()) },
    )
    .await
    .expect("vat profile");

    (pool, workspace_id)
}

async fn approve_vat_year(pool: &sqlx::SqlitePool, workspace_id: &str, fiscal_year: i32) {
    let draft = vat::vat_return_draft_create(
        pool,
        workspace_id,
        &VatReturnDraftCreateInput {
            period_key: fiscal_year.to_string(),
            idempotency_key: format!("m5-vat-year-{fiscal_year}"),
        },
    )
    .await
    .expect("vat draft for year-end tests");

    vat::vat_return_approve(
        pool,
        workspace_id,
        &VatReturnApproveInput {
            vat_return_id: draft.id,
            idempotency_key: format!("m5-vat-approve-{fiscal_year}"),
        },
    )
    .await
    .expect("vat approve for year-end tests");
}

async fn setup_workspace_with_vat_filed(
    dir: &tempfile::TempDir,
    tax_status: &str,
    fiscal_year: i32,
) -> (sqlx::SqlitePool, String) {
    let (pool, workspace_id) = setup_workspace(dir, tax_status).await;
    approve_vat_year(&pool, &workspace_id, fiscal_year).await;
    (pool, workspace_id)
}

#[tokio::test]
async fn m5_year_end_schema_tables_exist() {
    let dir = tempdir().expect("tempdir");
    let (pool, _workspace_id) = setup_workspace(&dir, "fa_skatt").await;

    let tables: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name FROM sqlite_master
        WHERE type = 'table' AND name IN ('year_end_packages', 'ne_fields')
        ORDER BY name
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("sqlite_master");

    assert_eq!(tables, vec!["ne_fields", "year_end_packages"]);
}

#[tokio::test]
async fn m5_year_end_k1_ne_fixture() {
    let dir = tempdir().expect("tempdir");
    let scenario = load_golden_scenario("year-end-k1-ne").expect("golden scenario");
    let expected = scenario.expected.as_object().expect("expected");
    let profile = scenario.profile.as_object().expect("profile");
    let tax_status = profile["taxStatus"].as_str().unwrap_or("fa_skatt");

    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, tax_status, 2026).await;

    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-k1-ne".to_string(),
        },
    )
    .await
    .expect("year-end package should be created for year-end-k1-ne fixture");

    assert_eq!(package.status, "draft");
    assert!(!package.fiscal_year_locked);

    assert_eq!(
        expected["simplifiedAnnualAccountsAllowed"].as_bool(),
        Some(package.k1_allowed)
    );
    assert_eq!(expected["neDraftRequired"].as_bool(), Some(package.ne_draft_present));
    assert_eq!(
        expected["annualAccountsStoredLocally"].as_bool(),
        Some(package.stored_locally)
    );
    assert_eq!(
        expected["exportPackageRequired"].as_bool(),
        Some(package.export_path.is_some())
    );

    let approved = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id,
            idempotency_key: "year-end-k1-ne-approve".to_string(),
        },
    )
    .await
    .expect("year-end package should be approved for year-end-k1-ne fixture");

    assert_eq!(approved.status, "approved");
    assert_eq!(
        expected["fiscalYearLockAfterApproval"].as_bool(),
        Some(approved.fiscal_year_locked)
    );

    let retried = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: approved.id.clone(),
            idempotency_key: "year-end-k1-ne-approve".to_string(),
        },
    )
    .await
    .expect("retry should return the approved package");

    assert_eq!(retried.id, approved.id);
    assert_eq!(retried.status, "approved");
}

#[tokio::test]
async fn m5_year_end_approval_rejects_stale_ledger_snapshot() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "fa_skatt").await;
    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "exempt_low_turnover".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: None,
        },
    )
    .await
    .expect("exempt VAT profile");

    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-stale-snapshot".to_string(),
        },
    )
    .await
    .expect("create package");

    expenses::expense_post(
        &pool,
        &workspace_id,
        &ExpensePostInput {
            amount_minor_ex_vat: 10_000,
            vat_rate: 0.0,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: None,
            no_document_reason: Some("Year-end snapshot regression fixture".to_string()),
            staged_transaction_id: None,
            idempotency_key: "year-end-stale-snapshot-expense".to_string(),
            date: Some("2026-12-31".to_string()),
        },
    )
    .await
    .expect("post in-year expense after reviewing draft");

    let err = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id.clone(),
            idempotency_key: "year-end-stale-snapshot-approve".to_string(),
        },
    )
    .await
    .expect_err("stale year-end package must not approve");

    assert_eq!(err.code, "validation_error");
    assert_eq!(
        err.message,
        "Ledger changed since this year-end package was generated. Regenerate and review it before approving."
    );

    let package_status: String = sqlx::query_scalar(
        "SELECT status FROM year_end_packages WHERE id = ?1",
    )
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("package status");
    assert_eq!(package_status, "draft");

    let fiscal_year_status: String = sqlx::query_scalar(
        r#"
        SELECT status FROM fiscal_years
        WHERE workspace_id = ?1 AND starts_on = '2026-01-01'
        LIMIT 1
        "#,
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("fiscal year status");
    assert_eq!(fiscal_year_status, "open");
}

#[tokio::test]
async fn m5_year_end_regenerates_stale_draft_before_successful_approval() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "fa_skatt").await;
    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "exempt_low_turnover".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: None,
        },
    )
    .await
    .expect("exempt VAT profile");
    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-regenerate-stale".to_string(),
        },
    )
    .await
    .expect("create package");

    expenses::expense_post(
        &pool,
        &workspace_id,
        &ExpensePostInput {
            amount_minor_ex_vat: 10_000,
            vat_rate: 0.0,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: None,
            no_document_reason: Some("Regenerate stale year-end fixture".to_string()),
            staged_transaction_id: None,
            idempotency_key: "year-end-regenerate-stale-expense".to_string(),
            date: Some("2026-12-31".to_string()),
        },
    )
    .await
    .expect("post in-year expense after reviewing draft");

    year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id.clone(),
            idempotency_key: "year-end-regenerate-stale-approve".to_string(),
        },
    )
    .await
    .expect_err("stale package must require regeneration");

    let regenerated = year_end::year_end_package_regenerate(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageRegenerateInput {
            package_id: package.id.clone(),
            idempotency_key: "year-end-regenerate-stale-refresh".to_string(),
        },
    )
    .await
    .expect("regenerate stale draft");

    assert_eq!(regenerated.id, package.id);
    assert_eq!(regenerated.status, "draft");
    assert_eq!(
        regenerated
            .ne_fields
            .iter()
            .find(|field| field.field_code == "B14")
            .map(|field| field.amount_minor),
        Some(-10_000),
    );

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1 AND action = 'year_end_package_regenerated' AND resource_id = ?2",
    )
    .bind(&workspace_id)
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("regeneration audit event");
    assert_eq!(audit_count, 1);

    let approved = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id,
            idempotency_key: "year-end-regenerate-stale-approve-after-review".to_string(),
        },
    )
    .await
    .expect("regenerated draft should require a new approval and then approve");
    assert_eq!(approved.status, "approved");
    assert!(approved.fiscal_year_locked);
}

#[tokio::test]
async fn m5_year_end_regeneration_rejects_approved_package() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "fa_skatt").await;
    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "exempt_low_turnover".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: None,
        },
    )
    .await
    .expect("exempt VAT profile");
    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-regenerate-approved".to_string(),
        },
    )
    .await
    .expect("create package");
    let approved = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id,
            idempotency_key: "year-end-regenerate-approved-approve".to_string(),
        },
    )
    .await
    .expect("approve package");

    let err = year_end::year_end_package_regenerate(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageRegenerateInput {
            package_id: approved.id.clone(),
            idempotency_key: "year-end-regenerate-approved-retry".to_string(),
        },
    )
    .await
    .expect_err("approved packages must not be regenerated");

    assert_eq!(err.code, "validation_error");
    assert_eq!(err.message, "Only draft year-end packages can be regenerated");
    let retried = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: approved.id,
            idempotency_key: "year-end-regenerate-approved-approve".to_string(),
        },
    )
    .await
    .expect("approved package retry remains idempotent");
    assert_eq!(retried.status, "approved");
}

#[tokio::test]
async fn m5_year_end_regeneration_recovers_pre_upgrade_missing_fingerprint() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;
    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-regenerate-missing-fingerprint".to_string(),
        },
    )
    .await
    .expect("create package");
    // A pre-upgrade creation event is an append-only fixture, not an audit-history rewrite.
    record_event(
        &pool,
        &workspace_id,
        "year_end_package_created",
        "year_end_package",
        Some(&package.id),
        "{}",
    )
    .await
    .expect("append legacy package event without a fingerprint");

    let regenerated = year_end::year_end_package_regenerate(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageRegenerateInput {
            package_id: package.id.clone(),
            idempotency_key: "year-end-regenerate-missing-fingerprint-refresh".to_string(),
        },
    )
    .await
    .expect("pre-upgrade draft should become regenerable");
    assert_eq!(regenerated.status, "draft");

    let approved = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id,
            idempotency_key: "year-end-regenerate-missing-fingerprint-approve".to_string(),
        },
    )
    .await
    .expect("regenerated pre-upgrade draft should approve");
    assert_eq!(approved.status, "approved");
}

#[tokio::test]
async fn m5_year_end_duplicate_fiscal_year_returns_existing() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;

    let first = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-first".to_string(),
        },
    )
    .await
    .expect("first create");

    let second = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-second".to_string(),
        },
    )
    .await
    .expect("second create should return existing package");

    assert_eq!(first.id, second.id);
    assert_eq!(first.ne_fields, second.ne_fields);
}

#[tokio::test]
async fn m5_year_end_find_by_fiscal_year() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;

    assert!(
        year_end::year_end_package_find_by_fiscal_year(
            &pool,
            &workspace_id,
            &year_end::YearEndPackageFindInput { fiscal_year: 2026 },
        )
        .await
        .expect("lookup")
        .is_none()
    );

    let created = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-find".to_string(),
        },
    )
    .await
    .expect("create");

    let found = year_end::year_end_package_find_by_fiscal_year(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageFindInput { fiscal_year: 2026 },
    )
    .await
    .expect("lookup after create")
    .expect("package should exist");

    assert_eq!(found.id, created.id);
}

#[tokio::test]
async fn m5_year_end_reexport_preserves_ne_fields() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;

    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-reexport".to_string(),
        },
    )
    .await
    .expect("create package");

    let approved = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id.clone(),
            idempotency_key: "year-end-reexport-approve".to_string(),
        },
    )
    .await
    .expect("approve package");

    let before = approved.ne_fields.clone();
    let exported = year_end::year_end_package_export(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageExportInput {
            package_id: approved.id.clone(),
            idempotency_key: "export-once".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("export package");

    assert!(exported.export_path.is_some());
    assert_eq!(exported.ne_fields, before);
}


#[tokio::test]
async fn m5_year_end_recovers_truncated_draft_artifact_with_verified_publication() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;
    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-recover-truncated-draft".to_string(),
        },
    )
    .await
    .expect("create package");
    let original_export_path: String = sqlx::query_scalar(
        "SELECT export_path FROM year_end_packages WHERE id = ?1",
    )
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("original export path");
    fs::write(
        dir.path()
            .join(&workspace_id)
            .join("exports")
            .join(&original_export_path),
        b"",
    )
    .expect("simulate interrupted export publication");

    let recovered = year_end::year_end_package_get(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageGetInput {
            package_id: package.id.clone(),
        },
    )
    .await
    .expect("recover draft artifacts");

    let row = sqlx::query(
        "SELECT export_path, export_bytes, export_sha256 FROM year_end_packages WHERE id = ?1",
    )
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("recovered export metadata");
    let recovered_export_path: String = row.get("export_path");
    let recovered_export_bytes: i64 = row.get("export_bytes");
    let recovered_export_sha256: String = row.get("export_sha256");
    let recovered_export = dir
        .path()
        .join(&workspace_id)
        .join("exports")
        .join(&recovered_export_path);
    assert_eq!(recovered_export_sha256, format!("{:x}", Sha256::digest(&fs::read(&recovered_export).expect("recovered export digest bytes"))));
    assert_eq!(recovered.status, "draft");
    assert_eq!(
        i64::try_from(fs::metadata(&recovered_export).expect("recovered export metadata").len())
            .expect("export length fits i64"),
        recovered_export_bytes
    );
    assert_eq!(recovered_export_sha256.len(), 64);
    serde_json::from_slice::<serde_json::Value>(
        &fs::read(recovered_export).expect("recovered export bytes"),
    )
    .expect("recovered export is complete JSON");
}

#[tokio::test]
async fn m5_year_end_regeneration_publishes_only_verified_artifacts() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;
    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-regeneration-integrity".to_string(),
        },
    )
    .await
    .expect("create package");
    let previous_export_path: String = sqlx::query_scalar(
        "SELECT export_path FROM year_end_packages WHERE id = ?1",
    )
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("original export path");

    let regenerated = year_end::year_end_package_regenerate(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageRegenerateInput {
            package_id: package.id.clone(),
            idempotency_key: "year-end-regeneration-integrity-refresh".to_string(),
        },
    )
    .await
    .expect("regenerate package");
    let row = sqlx::query(
        r#"
        SELECT annual_accounts_path, annual_accounts_bytes, annual_accounts_sha256,
               ne_draft_path, ne_draft_bytes, ne_draft_sha256,
               export_path, export_bytes, export_sha256
        FROM year_end_packages WHERE id = ?1
        "#,
    )
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("regenerated artifact metadata");
    let documents = dir.path().join(&workspace_id).join("documents");
    let exports = dir.path().join(&workspace_id).join("exports");

    for (base, path_column, bytes_column, sha_column) in [
        (&documents, "annual_accounts_path", "annual_accounts_bytes", "annual_accounts_sha256"),
        (&documents, "ne_draft_path", "ne_draft_bytes", "ne_draft_sha256"),
        (&exports, "export_path", "export_bytes", "export_sha256"),
    ] {
        let rel_path: String = row.get(path_column);
        let expected_bytes: i64 = row.get(bytes_column);
        let expected_sha256: String = row.get(sha_column);
        let bytes = fs::read(base.join(rel_path)).expect("published artifact bytes");
        assert_eq!(i64::try_from(bytes.len()).expect("artifact length fits i64"), expected_bytes);
        assert_eq!(expected_sha256.len(), 64);
        assert_eq!(expected_sha256, format!("{:x}", Sha256::digest(&bytes)));
        serde_json::from_slice::<serde_json::Value>(&bytes).expect("published artifact is complete JSON");
    }

    let regenerated_export_path: String = row.get("export_path");
    assert_eq!(regenerated.status, "draft");
    assert_ne!(regenerated_export_path, previous_export_path);
}

#[tokio::test]
async fn m5_year_end_rejects_export_of_tampered_approved_artifact() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;
    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-approved-tamper".to_string(),
        },
    )
    .await
    .expect("create package");
    let approved = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id,
            idempotency_key: "year-end-approved-tamper-approve".to_string(),
        },
    )
    .await
    .expect("approve package");
    let export_path: String =
        sqlx::query_scalar("SELECT export_path FROM year_end_packages WHERE id = ?1")
            .bind(&approved.id)
            .fetch_one(&pool)
            .await
            .expect("approved export path");
    fs::write(
        dir.path()
            .join(&workspace_id)
            .join("exports")
            .join(&export_path),
        b"",
    )
    .expect("tamper with approved export");

    let err = year_end::year_end_package_export(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageExportInput {
            package_id: approved.id.clone(),
            idempotency_key: "year-end-approved-tamper-export".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect_err("tampered approved artifact must not export");

    let status: String = sqlx::query_scalar("SELECT status FROM year_end_packages WHERE id = ?1")
        .bind(&approved.id)
        .fetch_one(&pool)
        .await
        .expect("package remains stored");
    let stored_export_path: String =
        sqlx::query_scalar("SELECT export_path FROM year_end_packages WHERE id = ?1")
            .bind(&approved.id)
            .fetch_one(&pool)
            .await
            .expect("approved export reference remains stored");
    assert_eq!(err.code, "storage_error");
    assert_eq!(status, "approved");
    assert_eq!(stored_export_path, export_path);
}
#[tokio::test]
async fn m5_year_end_rejects_pending_vat_return() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "fa_skatt").await;

    vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026".to_string(),
            idempotency_key: "year-end-block-vat".to_string(),
        },
    )
    .await
    .expect("vat draft");

    let err = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-with-draft-vat".to_string(),
        },
    )
    .await
    .expect_err("year-end should block while VAT return is draft");

    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("Draft VAT return"));

    let package_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM year_end_packages WHERE workspace_id = ?1",
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("package count");
    assert_eq!(package_count, 0);

    let fiscal_year_status: String = sqlx::query_scalar(
        r#"
        SELECT status FROM fiscal_years
        WHERE workspace_id = ?1 AND starts_on = '2026-01-01'
        LIMIT 1
        "#,
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("fiscal year status");
    assert_eq!(fiscal_year_status, "open");
}

#[tokio::test]
async fn m5_year_end_rejects_missing_vat_return() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "fa_skatt").await;

    let err = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-no-vat".to_string(),
        },
    )
    .await
    .expect_err("year-end should require approved VAT returns");

    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("Approved VAT return required"));
}

#[tokio::test]
async fn m5_year_end_regenerates_missing_artifacts() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;

    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-artifacts".to_string(),
        },
    )
    .await
    .expect("create package");

    sqlx::query(
        r#"
        UPDATE year_end_packages
        SET annual_accounts_path = NULL, ne_draft_path = NULL, export_path = NULL
        WHERE id = ?1
        "#,
    )
    .bind(&package.id)
    .execute(&pool)
    .await
    .expect("clear artifact paths");

    let healed = year_end::year_end_package_find_by_fiscal_year(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageFindInput { fiscal_year: 2026 },
    )
    .await
    .expect("lookup")
    .expect("package should exist");

    assert!(healed.stored_locally);
    assert!(healed.export_path.is_some());
}

#[tokio::test]
async fn m5_year_end_regenerates_missing_export_file() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;

    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-export-heal".to_string(),
        },
    )
    .await
    .expect("create package");

    let original_export_path = package.export_path.clone().expect("export path");
    let original_export_file = dir
        .path()
        .join(&workspace_id)
        .join("exports")
        .join(&original_export_path);
    fs::remove_file(&original_export_file).expect("delete export file");

    let healed = year_end::year_end_package_get(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageGetInput {
            package_id: package.id.clone(),
        },
    )
    .await
    .expect("heal missing export");

    let healed_export_path = healed.export_path.clone().expect("healed export path");
    let healed_export_file = dir
        .path()
        .join(&workspace_id)
        .join("exports")
        .join(&healed_export_path);
    let (expected_bytes, expected_sha256): (i64, String) = sqlx::query_as(
        "SELECT export_bytes, export_sha256 FROM year_end_packages WHERE id = ?1",
    )
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("healed export integrity metadata");
    let healed_export_bytes = fs::read(&healed_export_file).expect("healed export bytes");

    assert!(healed_export_file.is_file());
    assert_eq!(
        i64::try_from(healed_export_bytes.len()).expect("export length fits i64"),
        expected_bytes
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&healed_export_bytes)),
        expected_sha256
    );
    serde_json::from_slice::<serde_json::Value>(&healed_export_bytes)
        .expect("healed export is complete JSON");
}

#[tokio::test]
async fn m5_year_end_approve_rejects_missing_vat_return() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "fa_skatt").await;

    sqlx::query(
        r#"
        INSERT INTO year_end_packages (id, workspace_id, fiscal_year_id, status, rule_version_id)
        SELECT ?1, ?2, fy.id, 'draft', rv.id
        FROM fiscal_years fy, rule_versions rv
        WHERE fy.workspace_id = ?2 AND fy.starts_on = '2026-01-01' AND rv.status = 'active'
        LIMIT 1
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&workspace_id)
    .execute(&pool)
    .await
    .expect("draft package");

    let package_id: String = sqlx::query_scalar(
        "SELECT id FROM year_end_packages WHERE workspace_id = ?1 LIMIT 1",
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("package id");

    let err = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageApproveInput {
            package_id,
            idempotency_key: "approve-no-vat".to_string(),
        },
    )
    .await
    .expect_err("approve should require approved VAT returns");

    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("Approved VAT return required"));
}

#[tokio::test]
async fn m5_year_end_adopts_semantically_verified_legacy_approved_artifacts() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;
    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-legacy-adoption".to_string(),
        },
    )
    .await
    .expect("create package");
    let approved = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id.clone(),
            idempotency_key: "year-end-legacy-adoption-approve".to_string(),
        },
    )
    .await
    .expect("approve package");

    sqlx::query(
        r#"
        UPDATE year_end_packages
        SET annual_accounts_bytes = NULL, annual_accounts_sha256 = NULL,
            ne_draft_bytes = NULL, ne_draft_sha256 = NULL,
            export_bytes = NULL, export_sha256 = NULL
        WHERE id = ?1
        "#,
    )
    .bind(&approved.id)
    .execute(&pool)
    .await
    .expect("simulate pre-integrity upgrade");

    let adopted = year_end::year_end_package_get(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageGetInput {
            package_id: approved.id.clone(),
        },
    )
    .await
    .expect("semantically verified legacy package should adopt integrity metadata");
    assert_eq!(adopted.status, "approved");
    assert!(adopted.stored_locally);

    let metadata: (i64, String, i64, String, i64, String) = sqlx::query_as(
        r#"
        SELECT annual_accounts_bytes, annual_accounts_sha256,
               ne_draft_bytes, ne_draft_sha256,
               export_bytes, export_sha256
        FROM year_end_packages
        WHERE id = ?1
        "#,
    )
    .bind(&approved.id)
    .fetch_one(&pool)
    .await
    .expect("adopted integrity metadata");
    assert!(metadata.0 > 0 && metadata.2 > 0 && metadata.4 > 0);
    assert!(metadata.1.len() == 64 && metadata.3.len() == 64 && metadata.5.len() == 64);

    let audit_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM audit_events
        WHERE workspace_id = ?1
          AND action = 'year_end_package_legacy_artifacts_adopted'
          AND resource_id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&approved.id)
    .fetch_one(&pool)
    .await
    .expect("legacy adoption audit event");
    assert_eq!(audit_count, 1);

    year_end::year_end_package_export(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageExportInput {
            package_id: approved.id,
            idempotency_key: "year-end-legacy-adoption-export".to_string(),
            export_directory: None,
        },
    )
    .await
    .expect("adopted approved package should export");
}

#[tokio::test]
async fn m5_year_end_blocks_tampered_legacy_approved_artifacts() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace_with_vat_filed(&dir, "fa_skatt", 2026).await;
    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "year-end-legacy-tamper".to_string(),
        },
    )
    .await
    .expect("create package");
    let approved = year_end::year_end_package_approve(
        &pool,
        &workspace_id,
        &YearEndPackageApproveInput {
            package_id: package.id,
            idempotency_key: "year-end-legacy-tamper-approve".to_string(),
        },
    )
    .await
    .expect("approve package");
    let annual_path: String =
        sqlx::query_scalar("SELECT annual_accounts_path FROM year_end_packages WHERE id = ?1")
            .bind(&approved.id)
            .fetch_one(&pool)
            .await
            .expect("annual accounts path");

    sqlx::query(
        r#"
        UPDATE year_end_packages
        SET annual_accounts_bytes = NULL, annual_accounts_sha256 = NULL,
            ne_draft_bytes = NULL, ne_draft_sha256 = NULL,
            export_bytes = NULL, export_sha256 = NULL
        WHERE id = ?1
        "#,
    )
    .bind(&approved.id)
    .execute(&pool)
    .await
    .expect("simulate pre-integrity upgrade");
    fs::write(
        dir.path()
            .join(&workspace_id)
            .join("documents")
            .join(&annual_path),
        b"{}",
    )
    .expect("tamper with legacy annual accounts");

    let err = year_end::year_end_package_get(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageGetInput {
            package_id: approved.id.clone(),
        },
    )
    .await
    .expect_err("tampered legacy package must not adopt integrity metadata");

    let row = sqlx::query(
        r#"
        SELECT status, annual_accounts_path, annual_accounts_bytes
        FROM year_end_packages
        WHERE id = ?1
        "#,
    )
    .bind(&approved.id)
    .fetch_one(&pool)
    .await
    .expect("immutable approved package");
    assert_eq!(err.code, "storage_error");
    assert_eq!(
        err.message,
        "Approved year-end package artifacts failed integrity validation"
    );
    assert_eq!(row.get::<String, _>("status"), "approved");
    assert_eq!(row.get::<String, _>("annual_accounts_path"), annual_path);
    assert!(row.get::<Option<i64>, _>("annual_accounts_bytes").is_none());
}
