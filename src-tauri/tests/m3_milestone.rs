use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    state::load_golden_scenario,
    workspace::ensure_workspace_ready,
};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn m3_document_import_retention_fixture() {
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
    .bind("M3 fixture workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let scenario = load_golden_scenario("document-import-retention");
    let expected = scenario.expected.as_object().expect("expected object");

    let source_path = data_dir.join("receipt.png");
    fs::write(&source_path, b"receipt-bytes").expect("write");

    let imported = oppenbokforing_desktop_lib::documents::document_import(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::documents::DocumentImportInput {
            source_path: source_path.to_string_lossy().to_string(),
            filename: "receipt-2026-01-05.png".to_string(),
            mime_type: "image/png".to_string(),
            idempotency_key: "doc-import-1".to_string(),
        },
    )
    .await
    .expect("imported");

    assert_eq!(expected["documentStored"].as_bool(), Some(true));
    assert!(!imported.content_sha256.is_empty());
    assert_eq!(expected["contentHashPresent"].as_bool(), Some(true));
    assert_eq!(imported.retention_years, 7);
    assert_eq!(expected["retentionYears"].as_i64(), Some(7));
}

#[tokio::test]
async fn m3_document_import_idempotency_race_returns_winner() {
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
    .bind("M3 idempotency race workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let source_path = data_dir.join("race-receipt.png");
    fs::write(&source_path, b"race-receipt-bytes").expect("write");
    let input = oppenbokforing_desktop_lib::documents::DocumentImportInput {
        source_path: source_path.to_string_lossy().to_string(),
        filename: "race-receipt.png".to_string(),
        mime_type: "image/png".to_string(),
        idempotency_key: "doc-import-race".to_string(),
    };

    let (first, second) = tokio::join!(
        oppenbokforing_desktop_lib::documents::document_import(&pool, &workspace_id, &input),
        oppenbokforing_desktop_lib::documents::document_import(&pool, &workspace_id, &input),
    );

    let first = first.expect("first import");
    let second = second.expect("second import");
    assert_eq!(first.id, second.id);
    assert_eq!(first.content_sha256, second.content_sha256);

    let other_path = data_dir.join("other-receipt.png");
    fs::write(&other_path, b"other-receipt-bytes").expect("write other");
    let conflict = oppenbokforing_desktop_lib::documents::document_import(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::documents::DocumentImportInput {
            source_path: other_path.to_string_lossy().to_string(),
            filename: "other-receipt.png".to_string(),
            mime_type: "image/png".to_string(),
            idempotency_key: "doc-import-race".to_string(),
        },
    )
    .await;
    assert!(conflict.is_err());
}

#[tokio::test]
async fn m3_document_import_rejects_large_files() {
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
    .bind("M3 large document workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    // Create a file slightly above the 10 MiB guard.
    let source_path = data_dir.join("too-large.bin");
    let large_bytes = vec![0u8; 10 * 1024 * 1024 + 1];
    fs::write(&source_path, &large_bytes).expect("write large");

    let err = oppenbokforing_desktop_lib::documents::document_import(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::documents::DocumentImportInput {
            source_path: source_path.to_string_lossy().to_string(),
            filename: "too-large.bin".to_string(),
            mime_type: "application/octet-stream".to_string(),
            idempotency_key: "doc-import-too-large".to_string(),
        },
    )
    .await
    .expect_err("should reject large document");

    assert_eq!(err.code, "validation_error");
}

#[tokio::test]
async fn m3_manual_expense_input_vat_fixture() {
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
    .bind("M3 expense fixture workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let scenario = load_golden_scenario("manual-expense-input-vat");
    let expected = scenario.expected.as_object().expect("expected object");

    let receipt_path = data_dir.join("receipt.pdf");
    fs::write(&receipt_path, b"pdf-bytes").expect("write receipt");
    let document = oppenbokforing_desktop_lib::documents::document_import(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::documents::DocumentImportInput {
            source_path: receipt_path.to_string_lossy().to_string(),
            filename: "receipt-coffee.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            idempotency_key: "doc-import-2".to_string(),
        },
    )
    .await
    .expect("imported");

    let posted = oppenbokforing_desktop_lib::expenses::expense_post(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::expenses::ExpensePostInput {
            amount_minor_ex_vat: 10_000,
            vat_rate: 0.25,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: Some(document.id.clone()),
            no_document_reason: None,
            staged_transaction_id: None,
            idempotency_key: "expense-1".to_string(),
            date: Some("2026-01-05".to_string()),
        },
    )
    .await
    .expect("posted expense");

    assert_eq!(expected["voucherPosted"].as_bool(), Some(true));
    assert!(posted.voucher_id.is_some());
    assert_eq!(posted.debit_expense_minor, expected["debitExpenseMinor"].as_i64().unwrap());
    assert_eq!(posted.debit_input_vat_minor, expected["debitInputVatMinor"].as_i64().unwrap());
    assert_eq!(posted.credit_payment_minor, expected["creditBankMinor"].as_i64().unwrap());
}

#[tokio::test]
async fn m3_expense_requires_real_document_or_reason() {
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
    .bind("M3 expense document guard workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    // Empty/whitespace document id without reason must be rejected.
    let err = oppenbokforing_desktop_lib::expenses::expense_post(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::expenses::ExpensePostInput {
            amount_minor_ex_vat: 10_000,
            vat_rate: 0.25,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: Some("   ".to_string()),
            no_document_reason: None,
            staged_transaction_id: None,
            idempotency_key: "expense-empty-doc".to_string(),
            date: Some("2026-01-05".to_string()),
        },
    )
    .await
    .expect_err("should reject whitespace document id");

    assert_eq!(err.code, "validation_error");

    // Unknown document id should also be rejected.
    let err_unknown = oppenbokforing_desktop_lib::expenses::expense_post(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::expenses::ExpensePostInput {
            amount_minor_ex_vat: 10_000,
            vat_rate: 0.25,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: Some("non-existent-doc-id".to_string()),
            no_document_reason: None,
            staged_transaction_id: None,
            idempotency_key: "expense-unknown-doc".to_string(),
            date: Some("2026-01-05".to_string()),
        },
    )
    .await
    .expect_err("should reject unknown document id");

    assert_eq!(err_unknown.code, "validation_error");
}


#[tokio::test]
async fn m3_csv_import_match_invoice_payment_fixture() {
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
    .bind("M3 csv fixture workspace")
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

    let customer = oppenbokforing_desktop_lib::counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::counterparties::CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Invoice payer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");

    let draft = oppenbokforing_desktop_lib::invoicing::create_draft(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::invoicing::InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![oppenbokforing_desktop_lib::invoicing::InvoiceLineInput {
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

    let issued = oppenbokforing_desktop_lib::invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::invoicing::InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "issue-for-csv".to_string(),
            issue_date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect("issued");

    let csv_path = data_dir.join("bank.csv");
    fs::write(
        &csv_path,
        "date,description,amount_minor\n2026-02-15,Invoice payment 2026-0001,1250000\n",
    )
    .expect("csv");

    let import = oppenbokforing_desktop_lib::imports::csv_import_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::imports::CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-1".to_string(),
        },
    )
    .await
    .expect("import");

    assert_eq!(import.staged_count, 1);

    let match_result = oppenbokforing_desktop_lib::reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::reconciliation::ReconciliationMatchCreateInput {
            staged_transaction_id: import.first_staged_transaction_id.clone(),
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id),
            idempotency_key: "match-1".to_string(),
        },
    )
    .await
    .expect("matched");

    assert!(match_result.voucher_id.is_some());
}

#[tokio::test]
async fn m3_csv_import_is_idempotent_for_same_file() {
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
    .bind("M3 csv idempotency workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let csv_path = data_dir.join("bank.csv");
    fs::write(
        &csv_path,
        "date,description,amount_minor\n2026-02-15,Invoice payment 2026-0001,1250000\n",
    )
    .expect("csv");

    let first = oppenbokforing_desktop_lib::imports::csv_import_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::imports::CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-dup".to_string(),
        },
    )
    .await
    .expect("first import");

    let second = oppenbokforing_desktop_lib::imports::csv_import_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::imports::CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-dup".to_string(),
        },
    )
    .await
    .expect("second import");

    assert_eq!(first.id, second.id);
    assert_eq!(first.staged_count, second.staged_count);
    assert_eq!(
        first.first_staged_transaction_id,
        second.first_staged_transaction_id
    );
}

#[tokio::test]
async fn m3_csv_import_allows_comma_in_description() {
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
    .bind("M3 csv comma workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    let csv_path = data_dir.join("bank-commas.csv");
    fs::write(
        &csv_path,
        "date,description,amount_minor\n2026-03-01,\"Coffee, fika with client\",-5000\n",
    )
    .expect("csv");

    let import = oppenbokforing_desktop_lib::imports::csv_import_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::imports::CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-commas-1".to_string(),
        },
    )
    .await
    .expect("import");

    assert_eq!(import.staged_count, 1);
}

#[tokio::test]
async fn m3_reconciliation_rejects_mismatched_or_duplicate_payments() {
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
    .bind("M3 reconciliation guard workspace")
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

    let customer = oppenbokforing_desktop_lib::counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::counterparties::CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Invoice payer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");

    let draft = oppenbokforing_desktop_lib::invoicing::create_draft(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::invoicing::InvoiceCreateDraftInput {
            counterparty_id: customer.id.clone(),
            due_date: None,
            lines: vec![oppenbokforing_desktop_lib::invoicing::InvoiceLineInput {
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

    // Attempt to pay a non-issued invoice should fail.
    let unmatched_csv_path = data_dir.join("unmatched.csv");
    fs::write(
        &unmatched_csv_path,
        "date,description,amount_minor\n2026-02-10,Invoice payment 2026-0001,1250000\n",
    )
    .expect("csv");

    let import_unissued = oppenbokforing_desktop_lib::imports::csv_import_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::imports::CsvImportCreateInput {
            source_path: unmatched_csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-unissued".to_string(),
        },
    )
    .await
    .expect("import");

    let err = oppenbokforing_desktop_lib::reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::reconciliation::ReconciliationMatchCreateInput {
            staged_transaction_id: import_unissued.first_staged_transaction_id.clone(),
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(draft.id.clone()),
            idempotency_key: "match-unissued".to_string(),
        },
    )
    .await
    .expect_err("should reject non-issued invoice payment");

    assert_eq!(err.code, "validation_error");

    // Issue the invoice and pay it once with matching amount.
    let issued = oppenbokforing_desktop_lib::invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::invoicing::InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "issue-for-guards".to_string(),
            issue_date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect("issued");

    let csv_path = data_dir.join("bank-guards.csv");
    fs::write(
        &csv_path,
        "date,description,amount_minor\n2026-02-15,Invoice payment 2026-0001,1250000\n",
    )
    .expect("csv");

    let import = oppenbokforing_desktop_lib::imports::csv_import_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::imports::CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-guards".to_string(),
        },
    )
    .await
    .expect("import");

    let first_match = oppenbokforing_desktop_lib::reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::reconciliation::ReconciliationMatchCreateInput {
            staged_transaction_id: import.first_staged_transaction_id.clone(),
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id.clone()),
            idempotency_key: "match-guards-1".to_string(),
        },
    )
    .await
    .expect("first match");

    assert!(first_match.voucher_id.is_some());

    // A second staged line for the same amount should be rejected as a duplicate payment.
    let staged_second_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO staged_transactions (
          id, workspace_id, csv_import_id, transaction_date, description, amount_minor, status
        ) VALUES (?1, ?2, ?3, '2026-02-16', 'Duplicate payment 2026-0001', 1250000, 'staged')
        "#,
    )
    .bind(&staged_second_id)
    .bind(&workspace_id)
    .bind(&import.id)
    .execute(&pool)
    .await
    .expect("second staged row");

    let dup_err = oppenbokforing_desktop_lib::reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::reconciliation::ReconciliationMatchCreateInput {
            staged_transaction_id: staged_second_id.clone(),
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id.clone()),
            idempotency_key: "match-guards-2".to_string(),
        },
    )
    .await
    .expect_err("should reject duplicate payment");

    assert_eq!(dup_err.code, "validation_error");

    // A mismatched payment amount should also be rejected.
    let staged_mismatch_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO staged_transactions (
          id, workspace_id, csv_import_id, transaction_date, description, amount_minor, status
        ) VALUES (?1, ?2, ?3, '2026-02-17', 'Mismatched payment 2026-0001', 1000000, 'staged')
        "#,
    )
    .bind(&staged_mismatch_id)
    .bind(&workspace_id)
    .bind(&import.id)
    .execute(&pool)
    .await
    .expect("mismatched staged row");

    let mismatch_err = oppenbokforing_desktop_lib::reconciliation::reconciliation_match_create(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::reconciliation::ReconciliationMatchCreateInput {
            staged_transaction_id: staged_mismatch_id,
            match_kind: "invoice_payment".to_string(),
            invoice_id: Some(issued.id),
            idempotency_key: "match-guards-3".to_string(),
        },
    )
    .await
    .expect_err("should reject mismatched payment amount");

    assert_eq!(mismatch_err.code, "validation_error");
}


