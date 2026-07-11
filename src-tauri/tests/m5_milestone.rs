use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    state::load_golden_scenario,
    vat::{self, VatReturnApproveInput, VatReturnDraftCreateInput},
    workspace::ensure_workspace_ready,
    year_end::{self, YearEndPackageApproveInput, YearEndPackageCreateInput},
};
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
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
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
    let scenario = load_golden_scenario("year-end-k1-ne");
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

    let export_path = package.export_path.clone().expect("export path");
    let export_file = dir
        .path()
        .join(&workspace_id)
        .join("exports")
        .join(&export_path);
    fs::remove_file(&export_file).expect("delete export file");

    let healed = year_end::year_end_package_get(
        &pool,
        &workspace_id,
        &year_end::YearEndPackageGetInput {
            package_id: package.id.clone(),
        },
    )
    .await
    .expect("heal missing export");

    assert!(healed.export_path.is_some());
    assert!(export_file.is_file());
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
