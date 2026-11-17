use oppenbokforing_desktop_lib::{
    compliance::{
        evaluate_scenario, run_profile_compliance_checks, ComplianceProfileCheckInput,
        ScenarioProfile,
    },
    db::connect_workspace,
    profiles::{
        save_onboarding_profiles, BusinessProfileSaveInput, OnboardingProfileSaveInput,
        TaxProfileSaveInput, VatProfileSaveInput,
    },
    state::load_compliance_golden_scenario,
};
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn registered_f_skatt_onboarding_without_salary_passes_profile_guidance() {
    let dir = tempdir().unwrap();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .unwrap();
    let workspace_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO workspaces (id, name, database_path, documents_path, exports_path) VALUES (?1, 'Synthetic F-skatt', ?2, ?3, ?4)",
    )
    .bind(&workspace_id)
    .bind(dir.path().join("workspace.sqlite").to_string_lossy().to_string())
    .bind(dir.path().join("documents").to_string_lossy().to_string())
    .bind(dir.path().join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .unwrap();

    let mut input = OnboardingProfileSaveInput {
        business: BusinessProfileSaveInput {
            business_name: "Synthetic F-skatt Studio".to_string(),
            owner_name: "Synthetic Owner".to_string(),
            residency_country: Some("SE".to_string()),
            sni_code: None,
        },
        tax: TaxProfileSaveInput {
            tax_status: "f_skatt".to_string(),
            expected_business_profit_minor: Some(18_000_000),
            expected_salary_income_minor: Some(0),
            active_rule_year: Some(2026),
        },
        vat: VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: Some("annual_may_12".to_string()),
        },
    };
    let saved = save_onboarding_profiles(&pool, &workspace_id, &input)
        .await
        .unwrap();
    assert!(saved.compliance.passed, "{}", saved.compliance.outcomes);
    assert_eq!(saved.compliance.scenario_ids, vec!["f-skatt-business"]);
    assert_eq!(saved.tax.expected_salary_income_minor, 0);
    let outcomes = &saved.compliance.outcomes["f-skatt-business"];
    let fixture = load_compliance_golden_scenario("f-skatt-business").unwrap();
    assert_eq!(outcomes, &fixture.expected);
    assert_eq!(outcomes["businessIncomeInLedger"], true);
    assert_eq!(outcomes["requiresFSkattGuidance"], true);
    assert_eq!(outcomes["invoiceMustMentionFSkatt"], true);
    assert!(outcomes.get("requiresFaSkattGuidance").is_none());
    assert!(outcomes.get("taxPlanningUsesSalaryAssumption").is_none());

    // FA-skatt retains its salary requirement at the save boundary.
    input.tax.tax_status = "fa_skatt".to_string();
    for salary in [None, Some(0), Some(-1)] {
        input.tax.expected_salary_income_minor = salary;
        let err = save_onboarding_profiles(&pool, &workspace_id, &input)
            .await
            .unwrap_err();
        assert_eq!(err.code, "validation_error");
    }
    input.tax.expected_salary_income_minor = Some(48_000_000);
    let fa_saved = save_onboarding_profiles(&pool, &workspace_id, &input)
        .await
        .unwrap();
    assert!(fa_saved.compliance.passed);
    assert_eq!(
        fa_saved.compliance.scenario_ids,
        vec!["fa-skatt-salary-and-business"]
    );
}

#[tokio::test]
async fn fa_skatt_profile_guidance_still_requires_positive_salary() {
    let dir = tempdir().unwrap();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .unwrap();
    for (salary, expected_pass) in [(None, false), (Some(0), false), (Some(48_000_000), true)] {
        let result = run_profile_compliance_checks(
            &pool,
            &ComplianceProfileCheckInput {
                tax_status: "fa_skatt".to_string(),
                vat_status: "registered".to_string(),
                expected_salary_income_minor: salary,
                expected_business_profit_minor: Some(18_000_000),
                rule_year: Some(2026),
            },
        )
        .await
        .unwrap();
        assert_eq!(result.passed, expected_pass);
        assert_eq!(result.scenario_ids, vec!["fa-skatt-salary-and-business"]);
        assert_eq!(
            result.outcomes["fa-skatt-salary-and-business"]["taxPlanningUsesSalaryAssumption"],
            expected_pass
        );
    }
}

#[tokio::test]
async fn f_skatt_guidance_does_not_replace_exemption_checks_or_other_postures() {
    let dir = tempdir().unwrap();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .unwrap();
    for (tax_status, vat_status, expected_ids, passed) in [
        ("f_skatt", "registered", vec!["f-skatt-business"], true),
        (
            "f_skatt",
            "exempt_low_turnover",
            vec!["f-skatt-business", "vat-exempt-below-threshold"],
            true,
        ),
        (
            "fa_skatt",
            "exempt_low_turnover",
            vec!["fa-skatt-salary-and-business", "vat-exempt-below-threshold"],
            false,
        ),
        (
            "planning",
            "registered",
            vec!["vat-exempt-below-threshold"],
            false,
        ),
        (
            "unknown",
            "registered",
            vec!["vat-exempt-below-threshold"],
            false,
        ),
    ] {
        let result = run_profile_compliance_checks(
            &pool,
            &ComplianceProfileCheckInput {
                tax_status: tax_status.to_string(),
                vat_status: vat_status.to_string(),
                expected_salary_income_minor: None,
                expected_business_profit_minor: Some(18_000_000),
                rule_year: Some(2026),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            result.scenario_ids, expected_ids,
            "{tax_status}/{vat_status}"
        );
        assert_eq!(result.passed, passed, "{tax_status}/{vat_status}");
        if vat_status == "exempt_low_turnover" {
            let exemption = &result.outcomes["vat-exempt-below-threshold"];
            assert_eq!(exemption["invoiceMustStateVatExemption"], true);
            assert_eq!(exemption["mustChargeVat"], false);
            assert_eq!(exemption["mustRegisterForVat"], false);
            assert!(exemption["thresholdMinor"].as_i64().unwrap() > 0);
        }
    }
}

#[tokio::test]
async fn f_skatt_scenario_does_not_approve_other_tax_postures_or_unknown_scenarios() {
    let dir = tempdir().unwrap();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .unwrap();
    for tax_status in [None, Some("planning"), Some("unknown"), Some("fa_skatt")] {
        let profile = ScenarioProfile {
            tax_status: tax_status.map(str::to_string),
            vat_status: Some("registered".to_string()),
            expected_salary_income_minor: Some(48_000_000),
            expected_business_profit_minor: Some(18_000_000),
            rule_year: Some(2026),
        };
        let result = evaluate_scenario(&pool, "f-skatt-business", &profile, &[])
            .await
            .unwrap();
        assert!(!result.passed);
        assert_eq!(result.outcomes["requiresFSkattGuidance"], false);
        let unsupported = evaluate_scenario(&pool, "unknown", &profile, &[])
            .await
            .unwrap();
        assert!(!unsupported.passed);
        assert_eq!(unsupported.outcomes["error"], "scenario_not_implemented");
    }
}
