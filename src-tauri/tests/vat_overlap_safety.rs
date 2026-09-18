use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    vat::{self, VatReturnApproveInput, VatReturnDraftCreateInput, VatReturnTraceInput},
    workspace::{ensure_workspace_ready, fiscal_year_id_for_year},
};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

fn vat_profile(reporting_period: &str, deadline_regime: &str) -> VatProfileSaveInput {
    VatProfileSaveInput {
        vat_status: "registered".to_string(),
        reporting_period: reporting_period.to_string(),
        accounting_method: "invoice_method".to_string(),
        voluntary_registration_date: None,
        vat_filing_deadline_regime: Some(deadline_regime.to_string()),
    }
}

async fn setup_workspace() -> (tempfile::TempDir, sqlx::SqlitePool, String) {
    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let data_dir = dir.path().join(&workspace_id);
    fs::create_dir_all(data_dir.join("documents")).expect("documents directory");
    fs::create_dir_all(data_dir.join("exports")).expect("exports directory");
    let database_path = data_dir.join("workspace.sqlite");
    let pool = connect_workspace(&database_path).await.expect("connect workspace");

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("VAT overlap safety")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap workspace");
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

    profiles::save_vat_profile(&pool, &workspace_id, &vat_profile("quarterly", "quarterly_12"))
        .await
        .expect("quarterly VAT profile");

    (dir, pool, workspace_id)
}

#[tokio::test]
async fn reporting_frequency_change_is_rejected_after_vat_return_work_but_allowed_before_it() {
    let (_dir, pool, workspace_id) = setup_workspace().await;

    let monthly_profile = profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &vat_profile("monthly", "monthly_12"),
    )
    .await
    .expect("reporting frequency may change before VAT returns exist");
    assert_eq!(monthly_profile.reporting_period, "monthly");

    profiles::save_vat_profile(&pool, &workspace_id, &vat_profile("quarterly", "quarterly_12"))
        .await
        .expect("reset to quarterly before VAT return work");
    let quarterly_return = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "quarterly-q1".to_string(),
        },
    )
    .await
    .expect("quarterly draft");

    let err = profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &vat_profile("monthly", "monthly_12"),
    )
    .await
    .expect_err("VAT return work freezes reporting frequency");

    assert_eq!(err.code, "validation_error");
    assert_eq!(err.message, "VAT filing frequency cannot change after VAT return work exists");
    assert_eq!(
        profiles::get_vat_profile(&pool, &workspace_id)
            .await
            .expect("VAT profile")
            .expect("saved VAT profile")
            .reporting_period,
        "quarterly"
    );

    vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: quarterly_return.id,
            idempotency_key: "approve-quarterly-q1".to_string(),
        },
    )
    .await
    .expect("approve quarterly return");

    let err = profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &vat_profile("monthly", "monthly_12"),
    )
    .await
    .expect_err("approved VAT return also freezes reporting frequency");
    assert_eq!(err.message, "VAT filing frequency cannot change after VAT return work exists");
}

#[tokio::test]
async fn serial_profile_frequency_save_then_return_uses_the_committed_frequency() {
    let (_dir, pool, workspace_id) = setup_workspace().await;

    profiles::save_vat_profile(&pool, &workspace_id, &vat_profile("monthly", "monthly_12"))
        .await
        .expect("monthly frequency save");

    let monthly_return = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-M01".to_string(),
            idempotency_key: "serial-monthly-return".to_string(),
        },
    )
    .await
    .expect("monthly return must use the committed monthly profile");

    assert_eq!(monthly_return.period_key, "2026-M01");
    assert_eq!(
        profiles::get_vat_profile(&pool, &workspace_id)
            .await
            .expect("VAT profile")
            .expect("saved VAT profile")
            .reporting_period,
        "monthly"
    );
}

#[tokio::test]
async fn vat_return_creation_and_approval_reject_overlapping_periods() {
    let (_dir, pool, workspace_id) = setup_workspace().await;
    let quarterly_return = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "quarterly-q1".to_string(),
        },
    )
    .await
    .expect("quarterly draft");

    sqlx::query(
        "UPDATE vat_profiles SET reporting_period = 'monthly', vat_filing_deadline_regime = 'monthly_12' WHERE workspace_id = ?1",
    )
    .bind(&workspace_id)
    .execute(&pool)
    .await
    .expect("simulate a legacy reporting-frequency change");

    let err = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-M01".to_string(),
            idempotency_key: "monthly-m1".to_string(),
        },
    )
    .await
    .expect_err("overlapping monthly draft is rejected");
    assert_eq!(err.code, "validation_error");
    assert_eq!(err.message, "VAT return overlaps an existing VAT return");
    let non_overlapping_month = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-M04".to_string(),
            idempotency_key: "monthly-m4".to_string(),
        },
    )
    .await
    .expect("non-overlapping monthly draft");
    assert_eq!(non_overlapping_month.period_key, "2026-M04");
    let fiscal_year_id = format!("fy-{workspace_id}-2026");
    let monthly_period_id = format!("fp-{workspace_id}-2026-M01-manual");
    let monthly_return_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO fiscal_periods (
          id, workspace_id, fiscal_year_id, period_key, starts_on, ends_on, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open')
        "#,
    )
    .bind(&monthly_period_id)
    .bind(&workspace_id)
    .bind(&fiscal_year_id)
    .bind("2026-M01-manual")
    .bind("2026-01-01")
    .bind("2026-01-31")
    .execute(&pool)
    .await
    .expect("legacy overlapping fiscal period");
    sqlx::query(
        r#"
        INSERT INTO vat_returns (id, workspace_id, fiscal_period_id, status, rule_version_id)
        VALUES (?1, ?2, ?3, 'draft', ?4)
        "#,
    )
    .bind(&monthly_return_id)
    .bind(&workspace_id)
    .bind(&monthly_period_id)
    .bind(&quarterly_return.rule_version_id)
    .execute(&pool)
    .await
    .expect("legacy overlapping VAT draft");

    let err = vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: monthly_return_id,
            idempotency_key: "approve-monthly-m1".to_string(),
        },
    )
    .await
    .expect_err("overlapping VAT draft cannot be approved");
    assert_eq!(err.code, "validation_error");
    assert_eq!(err.message, "VAT return overlaps an existing VAT return");
}

#[tokio::test]
async fn historical_vat_return_trace_uses_the_period_fiscal_year_rule_version() {
    let (_dir, pool, workspace_id) = setup_workspace().await;

    fiscal_year_id_for_year(&pool, &workspace_id, 2025)
        .await
        .expect("bootstrap historical fiscal year");
    let vat_return = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2025-Q1".to_string(),
            idempotency_key: "historical-quarterly-return".to_string(),
        },
    )
    .await
    .expect("historical VAT return");
    assert_eq!(vat_return.rule_version_id, "rv-2025-year-end");

    let trace = vat::vat_return_trace(
        &pool,
        &workspace_id,
        &VatReturnTraceInput {
            vat_return_id: vat_return.id,
        },
    )
    .await
    .expect("historical VAT return trace");

    assert_eq!(trace.rule_version_id, "rv-2025-year-end");
    assert_eq!(trace.tax_year, 2025);
}

#[tokio::test]
async fn historical_vat_return_requires_an_active_rule_for_its_fiscal_year() {
    let (_dir, pool, workspace_id) = setup_workspace().await;

    fiscal_year_id_for_year(&pool, &workspace_id, 2024)
        .await
        .expect("bootstrap historical fiscal year");
    let err = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2024-Q1".to_string(),
            idempotency_key: "missing-historical-rule".to_string(),
        },
    )
    .await
    .expect_err("historical VAT return without its own rule must fail closed");

    assert_eq!(err.code, "validation_error");
    assert_eq!(
        err.message,
        "No active rule version for VAT return fiscal year"
    );
    let period_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fiscal_periods WHERE workspace_id = ?1 AND period_key = '2024-Q1')",
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("check rollback");
    assert!(!period_exists);
}

#[tokio::test]
async fn vat_draft_rejects_closed_year_without_creating_period_return_boxes_or_job() {
    let (_dir, pool, workspace_id) = setup_workspace().await;
    let fiscal_year_id = format!("fy-{workspace_id}-2026");

    sqlx::query("UPDATE fiscal_years SET status = 'closed' WHERE id = ?1")
        .bind(&fiscal_year_id)
        .execute(&pool)
        .await
        .expect("close fiscal year");

    let error = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "closed-year-vat-draft".to_string(),
        },
    )
    .await
    .expect_err("closed fiscal year must reject VAT draft creation");
    assert_eq!(error.code, "locked_period");

    let (period_count, return_count, box_count, job_count): (i64, i64, i64, i64) =
        sqlx::query_as(
            r#"
            SELECT
              (SELECT COUNT(*) FROM fiscal_periods WHERE fiscal_year_id = ?1),
              (SELECT COUNT(*) FROM vat_returns WHERE workspace_id = ?2),
              (SELECT COUNT(*) FROM vat_return_boxes),
              (SELECT COUNT(*) FROM local_jobs
               WHERE workspace_id = ?2 AND job_type = 'vat_return_draft_create')
            "#,
        )
        .bind(&fiscal_year_id)
        .bind(&workspace_id)
        .fetch_one(&pool)
        .await
        .expect("VAT draft state remains unchanged");
    assert_eq!((period_count, return_count, box_count, job_count), (0, 0, 0, 0));
}
