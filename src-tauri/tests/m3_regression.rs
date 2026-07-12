use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    imports::{self, CsvImportCreateInput},
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceCreditInput, InvoiceIssueInput, InvoiceLineInput,
    },
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    reconciliation::{self, ReconciliationMatchCreateInput},
    workspace::ensure_workspace_ready,
};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_workspace() -> (tempfile::TempDir, String, sqlx::SqlitePool) {
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
    .bind("M3 regression workspace")
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

    (dir, workspace_id, pool)
}

async fn issue_standard_invoice(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    issue_key: &str,
) -> oppenbokforing_desktop_lib::invoicing::InvoiceSummary {
    let customer = counterparties::create_counterparty(
        pool,
        workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Regression customer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");

    let draft = invoicing::create_draft(
        pool,
        workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Service".to_string(),
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
        pool,
        workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: issue_key.to_string(),
            issue_date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect("issued")
}

async fn stage_csv_payment(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    data_dir: &std::path::Path,
    description: &str,
    import_key: &str,
) -> String {
    let csv_path = data_dir.join(format!("{import_key}.csv"));
    fs::write(
        &csv_path,
        format!(
            "date,description,amount_minor\n2026-02-15,{description},1250000\n"
        ),
    )
    .expect("csv");

    let import = imports::csv_import_create(
        pool,
        workspace_id,
        &CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: import_key.to_string(),
        },
    )
    .await
    .expect("import");

    import.first_staged_transaction_id
}

#[tokio::test]
async fn reconciliation_rejects_credited_invoice() {
    let (dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-credit-regression").await;

    invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id.clone(),
            idempotency_key: "credit-regression".to_string(),
            reason: Some("Returned".to_string()),
        },
    )
    .await
    .expect("credited");

    let staged_id = stage_csv_payment(
        &pool,
        &workspace_id,
        dir.path().join(&workspace_id).as_path(),
        "Payment after credit",
        "csv-credit-regression",
    )
    .await;

    let error = reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &ReconciliationMatchCreateInput {
            staged_transaction_id: staged_id,
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id),
            idempotency_key: "match-credit-regression".to_string(),
        },
    )
    .await
    .expect_err("credited invoice must not accept payment");

    assert_eq!(error.code, "validation_error");
    assert!(error.message.contains("Only issued invoices"));
}

#[tokio::test]
async fn reconciliation_rejects_duplicate_invoice_payment() {
    let (dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-dup-regression").await;
    let data_dir = dir.path().join(&workspace_id);

    let first_staged = stage_csv_payment(
        &pool,
        &workspace_id,
        &data_dir,
        "First payment",
        "csv-dup-1",
    )
    .await;

    reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &ReconciliationMatchCreateInput {
            staged_transaction_id: first_staged,
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id.clone()),
            idempotency_key: "match-dup-1".to_string(),
        },
    )
    .await
    .expect("first match");

    let second_staged = stage_csv_payment(
        &pool,
        &workspace_id,
        &data_dir,
        "Second payment",
        "csv-dup-2",
    )
    .await;

    let error = reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &ReconciliationMatchCreateInput {
            staged_transaction_id: second_staged,
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id),
            idempotency_key: "match-dup-2".to_string(),
        },
    )
    .await
    .expect_err("duplicate payment must be rejected");

    assert_eq!(error.code, "validation_error");
    assert!(error.message.contains("already has a payment"));
}

#[tokio::test]
async fn csv_import_persists_staged_rows() {
    let (dir, workspace_id, pool) = setup_workspace().await;
    let data_dir = dir.path().join(&workspace_id);
    let csv_path = data_dir.join("persist.csv");
    fs::write(
        &csv_path,
        "date,description,amount_minor\n2026-02-15,Bank row,50000\n",
    )
    .expect("csv");

    let summary = imports::csv_import_create(
        &pool,
        &workspace_id,
        &CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-persist".to_string(),
        },
    )
    .await
    .expect("import");

    let staged_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM staged_transactions
        WHERE workspace_id = ?1 AND csv_import_id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&summary.id)
    .fetch_one(&pool)
    .await
    .expect("count");

    assert_eq!(staged_count, summary.staged_count);
    assert_eq!(staged_count, 1);
}

#[tokio::test]
async fn csv_import_idempotent_replay_skips_audit() {
    let (dir, workspace_id, pool) = setup_workspace().await;
    let data_dir = dir.path().join(&workspace_id);
    let csv_path = data_dir.join("audit.csv");
    fs::write(
        &csv_path,
        "date,description,amount_minor\n2026-02-15,Bank row,50000\n",
    )
    .expect("csv");

    let input = CsvImportCreateInput {
        source_path: csv_path.to_string_lossy().to_string(),
        idempotency_key: "csv-audit-replay".to_string(),
    };

    let first = imports::csv_import_create(&pool, &workspace_id, &input)
        .await
        .expect("first import");
    let replay = imports::csv_import_create(&pool, &workspace_id, &input)
        .await
        .expect("replay import");

    assert_eq!(first.id, replay.id);

    let audit_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM audit_events
        WHERE workspace_id = ?1 AND action = 'csv_import_create'
        "#,
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("audit count");

    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn csv_import_idempotency_rejects_different_file() {
    let (dir, workspace_id, pool) = setup_workspace().await;
    let data_dir = dir.path().join(&workspace_id);
    let csv_a = data_dir.join("a.csv");
    let csv_b = data_dir.join("b.csv");
    fs::write(
        &csv_a,
        "date,description,amount_minor\n2026-02-15,Bank row,50000\n",
    )
    .expect("csv a");
    fs::write(
        &csv_b,
        "date,description,amount_minor\n2026-02-16,Other row,60000\n",
    )
    .expect("csv b");

    imports::csv_import_create(
        &pool,
        &workspace_id,
        &CsvImportCreateInput {
            source_path: csv_a.to_string_lossy().to_string(),
            idempotency_key: "csv-same-key".to_string(),
        },
    )
    .await
    .expect("first import");

    let error = imports::csv_import_create(
        &pool,
        &workspace_id,
        &CsvImportCreateInput {
            source_path: csv_b.to_string_lossy().to_string(),
            idempotency_key: "csv-same-key".to_string(),
        },
    )
    .await
    .expect_err("different file must not reuse key");

    assert_eq!(error.code, "validation_error");
    assert!(error.message.contains("different CSV file"));
}

#[tokio::test]
async fn reconciliation_rejects_already_matched_staged_row() {
    let (dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-staged-guard").await;
    let data_dir = dir.path().join(&workspace_id);

    let staged_id = stage_csv_payment(
        &pool,
        &workspace_id,
        &data_dir,
        "First payment",
        "csv-staged-guard",
    )
    .await;

    reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &ReconciliationMatchCreateInput {
            staged_transaction_id: staged_id.clone(),
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id.clone()),
            idempotency_key: "match-staged-guard-1".to_string(),
        },
    )
    .await
    .expect("first match");

    let error = reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &ReconciliationMatchCreateInput {
            staged_transaction_id: staged_id,
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id.clone()),
            idempotency_key: "match-staged-guard-2".to_string(),
        },
    )
    .await
    .expect_err("already matched staged row must be rejected");

    assert_eq!(error.code, "validation_error");
    assert!(error.message.contains("not available for matching"));
}

#[tokio::test]
async fn invoice_payment_record_links_bank_statement_pdf() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-for-pdf-payment").await;

    let statement = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"%PDF-1.4 bank statement",
        "bank-july.pdf",
        "application/pdf",
    )
    .await
    .expect("bank statement");

    let result = reconciliation::invoice_payment_record(
        &pool,
        &workspace_id,
        &reconciliation::InvoicePaymentRecordInput {
            invoice_id: issued.id.clone(),
            document_id: statement.id.clone(),
            payment_date: Some("2026-03-01".to_string()),
            idempotency_key: "pdf-payment-1".to_string(),
        },
    )
    .await
    .expect("record payment");

    assert!(result.voucher_id.is_some());

    let voucher_document: Option<String> = sqlx::query_scalar(
        r#"
        SELECT document_id FROM vouchers
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(&workspace_id)
    .bind(result.voucher_id.as_ref().expect("voucher id"))
    .fetch_one(&pool)
    .await
    .expect("voucher row");

    assert_eq!(voucher_document.as_deref(), Some(statement.id.as_str()));

    let refreshed = invoicing::get_invoice(&pool, &workspace_id, &issued.id)
        .await
        .expect("invoice");
    assert!(refreshed.payment_voucher_id.is_some());
}
