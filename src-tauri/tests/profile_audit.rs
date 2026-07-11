use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
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
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
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
