use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    documents,
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceIssueInput, InvoiceLineInput,
    },
    profiles::{self, TaxProfileSaveInput},
    settings,
    workspace::ensure_workspace_ready,
};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn new_workspace_defaults_locale_to_sv() {
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
    .bind("M8 locale workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let settings = settings::workspace_settings_get(&pool, &workspace_id)
        .await
        .expect("settings");

    assert_eq!(settings.locale, "sv", "new workspaces must default to Swedish locale");
    assert!(settings.simple_mode, "new workspaces must default to simple mode");
    assert!(!settings.dashboard_tour_completed, "tour should run on first dashboard visit");
}

#[tokio::test]
async fn simple_mode_save_persists() {
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
    .bind("M8 simple mode workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let saved = settings::workspace_settings_save(
        &pool,
        &workspace_id,
        &settings::WorkspaceSettingsSaveInput {
            locale: "sv".to_string(),
            updater_enabled: None,
            default_export_directory: None,
            default_backup_directory: None,
            simple_mode: Some(false),
        },
    )
    .await
    .expect("save");

    assert!(!saved.simple_mode);
}

#[tokio::test]
async fn dashboard_tour_mark_complete_persists() {
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
    .bind("M8 tour workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let updated = settings::dashboard_tour_mark_complete(&pool, &workspace_id)
        .await
        .expect("tour complete");

    assert!(updated.dashboard_tour_completed);
}

#[tokio::test]
async fn count_open_invoices_excludes_reconciled_payments() {
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
    .bind("M8 open invoice workspace")
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
            tax_status: "f_skatt".to_string(),
            expected_business_profit_minor: Some(500_000),
            expected_salary_income_minor: Some(0),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("tax profile");

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Open invoice customer".to_string(),
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
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Service".to_string(),
                quantity: 1,
                unit_price_minor: 10_000_00,
                vat_rate: 0.0,
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
            invoice_id: draft.id,
            idempotency_key: "issue-open-count".to_string(),
            issue_date: Some("2026-01-15".to_string()),
        },
    )
    .await
    .expect("issued");

    let open_before = invoicing::count_open_invoices(&pool, &workspace_id)
        .await
        .expect("open count");
    assert_eq!(open_before, 1);

    let staged_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO staged_transactions (
          id, workspace_id, csv_import_id, transaction_date, description, amount_minor, status
        ) VALUES (?1, ?2, NULL, '2026-01-20', 'Payment', ?3, 'matched')
        "#,
    )
    .bind(&staged_id)
    .bind(&workspace_id)
    .bind(issued.total_inc_vat_minor)
    .execute(&pool)
    .await
    .expect("staged row");

    sqlx::query(
        r#"
        INSERT INTO reconciliation_matches (id, workspace_id, staged_transaction_id, match_kind, invoice_id, voucher_id)
        VALUES (?1, ?2, ?3, 'invoice_payment', ?4, NULL)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&workspace_id)
    .bind(&staged_id)
    .bind(&issued.id)
    .execute(&pool)
    .await
    .expect("payment match");

    let open_after = invoicing::count_open_invoices(&pool, &workspace_id)
        .await
        .expect("open count");
    assert_eq!(open_after, 0);
}

#[tokio::test]
async fn document_reveal_rejects_path_traversal() {
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
    .bind("M8 reveal workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let document_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO documents (
          id, workspace_id, object_path, content_sha256, mime_type, original_filename, retention_years
        ) VALUES (?1, ?2, ?3, ?4, 'application/pdf', 'evil.pdf', 7)
        "#,
    )
    .bind(&document_id)
    .bind(&workspace_id)
    .bind("../../../etc/passwd")
    .bind("deadbeef")
    .execute(&pool)
    .await
    .expect("poisoned document");

    let error = documents::document_reveal(&pool, &workspace_id, &document_id)
        .await
        .expect_err("traversal must be rejected");
    assert_eq!(error.code, "validation_error");
}

#[cfg(unix)]
#[tokio::test]
async fn document_reveal_rejects_symlink_object_path() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let data_dir = dir.path().join(&workspace_id);
    let documents_dir = data_dir.join("documents");
    fs::create_dir_all(&documents_dir).expect("documents");
    fs::create_dir_all(data_dir.join("exports")).expect("exports");
    let outside_file = dir.path().join("outside-secret.pdf");
    fs::write(&outside_file, b"secret").expect("outside file");
    let symlink_path = documents_dir.join("link.pdf");
    symlink(&outside_file, &symlink_path).expect("symlink");

    let database_path = data_dir.join("workspace.sqlite");
    let pool = connect_workspace(&database_path).await.expect("connect");

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("M8 reveal symlink workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(documents_dir.to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let document_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO documents (
          id, workspace_id, object_path, content_sha256, mime_type, original_filename, retention_years
        ) VALUES (?1, ?2, ?3, ?4, 'application/pdf', 'link.pdf', 7)
        "#,
    )
    .bind(&document_id)
    .bind(&workspace_id)
    .bind("link.pdf")
    .bind("cafebabe")
    .execute(&pool)
    .await
    .expect("symlink document");

    let error = documents::document_reveal(&pool, &workspace_id, &document_id)
        .await
        .expect_err("symlink must be rejected");
    assert_eq!(error.code, "validation_error");
}
