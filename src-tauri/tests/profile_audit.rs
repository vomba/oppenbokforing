use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    profiles::{
        self, BusinessProfileSaveInput, OnboardingProfileSaveInput, TaxProfileSaveInput,
        VatProfileSaveInput,
    },
    workspace::ensure_workspace_ready,
};
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn profile_saves_record_audit_events() {
    let dir = tempdir().unwrap();
    let workspace_id = Uuid::new_v4().to_string();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .unwrap();

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("Audit test")
    .bind(dir.path().join("workspace.sqlite").to_string_lossy().to_string())
    .bind(dir.path().join("documents").to_string_lossy().to_string())
    .bind(dir.path().join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .unwrap();

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "fa_skatt".to_string(),
            expected_business_profit_minor: Some(1_000_000),
            expected_salary_income_minor: Some(2_000_000),
            active_rule_year: Some(2026),
        },
    )
    .await
    .unwrap();

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput { vat_status: "registered".to_string(),
        reporting_period: "quarterly".to_string(),
        accounting_method: "invoice_method".to_string(),
        voluntary_registration_date: None, vat_filing_deadline_regime: Some("quarterly_12".to_string()) },
    )
    .await
    .unwrap();

    let actions: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT action FROM audit_events
        WHERE workspace_id = ?1
        ORDER BY created_at ASC
        "#,
    )
    .bind(&workspace_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert!(actions.contains(&"tax_profile_save_current".to_string()));
    assert!(actions.contains(&"vat_profile_save_current".to_string()));
}

#[tokio::test]
async fn vat_frequency_guard_preserves_consistent_state_and_onboarding_audits_once_per_profile() {
    let dir = tempdir().unwrap();
    let workspace_id = Uuid::new_v4().to_string();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .unwrap();

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("Atomic onboarding test")
    .bind(dir.path().join("workspace.sqlite").to_string_lossy().to_string())
    .bind(dir.path().join("documents").to_string_lossy().to_string())
    .bind(dir.path().join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .unwrap();
    ensure_workspace_ready(&pool, &workspace_id).await.unwrap();
    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: Some("quarterly_12".to_string()),
        },
    )
    .await
    .unwrap();
    let fiscal_year_id: String =
        sqlx::query_scalar("SELECT id FROM fiscal_years WHERE workspace_id = ?1 LIMIT 1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let rule_version_id: String =
        sqlx::query_scalar("SELECT id FROM rule_versions WHERE tax_year = 2026 LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let fiscal_period_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO fiscal_periods (id, workspace_id, fiscal_year_id, period_key, starts_on, ends_on)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&fiscal_period_id)
    .bind(&workspace_id)
    .bind(&fiscal_year_id)
    .bind("2026-Q1")
    .bind("2026-01-01")
    .bind("2026-03-31")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO vat_returns (id, workspace_id, fiscal_period_id, status, rule_version_id) VALUES (?1, ?2, ?3, 'draft', ?4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&workspace_id)
    .bind(&fiscal_period_id)
    .bind(&rule_version_id)
    .execute(&pool)
    .await
    .unwrap();
    let audit_count_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let direct_frequency_change = profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "monthly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: Some("monthly_12".to_string()),
        },
    )
    .await;
    assert!(direct_frequency_change.is_err());
    let direct_reporting_period: String =
        sqlx::query_scalar("SELECT reporting_period FROM vat_profiles WHERE workspace_id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let direct_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(direct_reporting_period, "quarterly");
    assert_eq!(direct_audit_count, audit_count_before);

    let invalid = profiles::save_onboarding_profiles(
        &pool,
        &workspace_id,
        &OnboardingProfileSaveInput {
            business: BusinessProfileSaveInput {
                business_name: "Atomic Studio".to_string(),
                owner_name: "Anna Andersson".to_string(),
                residency_country: Some("SE".to_string()),
                sni_code: None,
            },
            tax: TaxProfileSaveInput {
                tax_status: "planning".to_string(),
                expected_business_profit_minor: Some(500_000),
                expected_salary_income_minor: Some(0),
                active_rule_year: Some(2026),
            },
            vat: VatProfileSaveInput {
                vat_status: "registered".to_string(),
                reporting_period: "monthly".to_string(),
                accounting_method: "invoice_method".to_string(),
                voluntary_registration_date: None,
                vat_filing_deadline_regime: Some("monthly_12".to_string()),
            },
        },
    )
    .await;

    assert!(invalid.is_err());
    let business_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sole_trader_profiles WHERE workspace_id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let tax_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tax_profiles WHERE workspace_id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let reporting_period: String =
        sqlx::query_scalar("SELECT reporting_period FROM vat_profiles WHERE workspace_id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(business_count, 0);
    assert_eq!(tax_count, 0);
    assert_eq!(audit_count, audit_count_before);
    assert_eq!(reporting_period, "quarterly");

    let saved = profiles::save_onboarding_profiles(
        &pool,
        &workspace_id,
        &OnboardingProfileSaveInput {
            business: BusinessProfileSaveInput {
                business_name: "Atomic Studio".to_string(),
                owner_name: "Anna Andersson".to_string(),
                residency_country: Some("SE".to_string()),
                sni_code: Some("62010".to_string()),
            },
            tax: TaxProfileSaveInput {
                tax_status: "planning".to_string(),
                expected_business_profit_minor: Some(500_000),
                expected_salary_income_minor: Some(1_200_000),
                active_rule_year: Some(2026),
            },
            vat: VatProfileSaveInput {
                vat_status: "voluntary_registered".to_string(),
                reporting_period: "quarterly".to_string(),
                accounting_method: "cash_method".to_string(),
                voluntary_registration_date: Some("2026-01-01".to_string()),
                vat_filing_deadline_regime: Some("quarterly_12".to_string()),
            },
        },
    )
    .await
    .unwrap();

    assert_eq!(saved.tax.tax_status, "planning");
    assert_eq!(saved.vat.vat_status, "voluntary_registered");
    assert_eq!(
        saved.vat.voluntary_registration_date.as_deref(),
        Some("2026-01-01")
    );
    assert_eq!(
        profiles::get_tax_profile(&pool, &workspace_id)
            .await
            .unwrap()
            .unwrap()
            .tax_status,
        "planning"
    );
    let stored_vat = profiles::get_vat_profile(&pool, &workspace_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_vat.vat_status, "voluntary_registered");
    assert_eq!(
        stored_vat.voluntary_registration_date.as_deref(),
        Some("2026-01-01")
    );
    let final_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(final_audit_count, audit_count_before + 3);
}
