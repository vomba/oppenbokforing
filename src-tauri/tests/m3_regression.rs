use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    imports::{self, CsvImportCreateInput},
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceCreditInput, InvoiceIssueInput, InvoiceLineInput,
    },
    profiles::{self, BusinessProfileSaveInput, TaxProfileSaveInput, VatProfileSaveInput},
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


    profiles::save_business_profile(
        &pool,
        &workspace_id,
        &BusinessProfileSaveInput {
            business_name: "M3 Regression Firma".to_string(),
            owner_name: "Regression Owner".to_string(),
            residency_country: Some("SE".to_string()),
            sni_code: Some("62010".to_string()),
        },
    )
    .await
    .expect("business");
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
        &VatProfileSaveInput { vat_status: "registered".to_string(),
        reporting_period: "quarterly".to_string(),
        accounting_method: "invoice_method".to_string(),
        voluntary_registration_date: None, vat_filing_deadline_regime: Some("quarterly_12".to_string()) },
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
            issue_date: Some("2026-02-10".to_string()),
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

    let voucher: (Option<String>, Option<String>) = sqlx::query_as(
        r#"
        SELECT document_id, accounting_date FROM vouchers
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(&workspace_id)
    .bind(result.voucher_id.as_ref().expect("voucher id"))
    .fetch_one(&pool)
    .await
    .expect("voucher row");

    assert_eq!(voucher.0.as_deref(), Some(statement.id.as_str()));
    assert_eq!(voucher.1.as_deref(), Some("2026-03-01"));

    let refreshed = invoicing::get_invoice(&pool, &workspace_id, &issued.id)
        .await
        .expect("invoice");
    assert!(refreshed.payment_voucher_id.is_some());
}

#[tokio::test]
async fn invoice_payment_record_rejects_omitted_and_invalid_payment_dates() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-for-payment-date-validation").await;
    let statement = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"%PDF-1.4 bank statement",
        "bank-july.pdf",
        "application/pdf",
    )
    .await
    .expect("bank statement");

    for payment_date in [None, Some("2026-13-01".to_string())] {
        let error = reconciliation::invoice_payment_record(
            &pool,
            &workspace_id,
            &reconciliation::InvoicePaymentRecordInput {
                invoice_id: issued.id.clone(),
                document_id: statement.id.clone(),
                payment_date,
                idempotency_key: Uuid::new_v4().to_string(),
            },
        )
        .await
        .expect_err("payment date must be explicit ISO date");

        assert_eq!(error.code, "validation_error");
        assert_eq!(
            error.details.as_ref().and_then(|details| details[0].field.as_deref()),
            Some("paymentDate")
        );
    }
}

#[tokio::test]
async fn invoice_payment_record_rejects_payment_before_issued_invoice_without_persistence() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-before-cross-year-payment").await;
    let statement = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"%PDF-1.4 bank statement",
        "bank-prior-year.pdf",
        "application/pdf",
    )
    .await
    .expect("bank statement");
    let idempotency_key = "payment-before-issued-invoice";

    let error = reconciliation::invoice_payment_record(
        &pool,
        &workspace_id,
        &reconciliation::InvoicePaymentRecordInput {
            invoice_id: issued.id.clone(),
            document_id: statement.id,
            payment_date: Some("2025-12-31".to_string()),
            idempotency_key: idempotency_key.to_string(),
        },
    )
    .await
    .expect_err("payment before the issued invoice must be rejected");

    assert_eq!(error.code, "validation_error");
    assert_eq!(
        error.details.as_ref().and_then(|details| details[0].field.as_deref()),
        Some("paymentDate")
    );

    let (staged_count, voucher_count, match_count, idempotency_count): (i64, i64, i64, i64) =
        sqlx::query_as(
            r#"
            SELECT
              (SELECT COUNT(*) FROM staged_transactions WHERE workspace_id = ?1),
              (SELECT COUNT(*) FROM vouchers WHERE workspace_id = ?1 AND source_type = 'reconciliation' AND source_id = ?2),
              (SELECT COUNT(*) FROM reconciliation_matches WHERE workspace_id = ?1 AND invoice_id = ?2),
              (SELECT COUNT(*) FROM local_jobs WHERE workspace_id = ?1 AND job_type = 'invoice_payment_record' AND idempotency_key = ?3)
            "#,
        )
        .bind(&workspace_id)
        .bind(&issued.id)
        .bind(idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("persistence counts");

    assert_eq!((staged_count, voucher_count, match_count, idempotency_count), (0, 0, 0, 0));
}

#[tokio::test]
async fn invoice_payment_record_rejects_idempotency_replay_with_changed_payment_date() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-for-payment-date-idempotency").await;
    let statement = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"%PDF-1.4 bank statement",
        "bank-july.pdf",
        "application/pdf",
    )
    .await
    .expect("bank statement");
    let idempotency_key = "payment-date-idempotency";

    reconciliation::invoice_payment_record(
        &pool,
        &workspace_id,
        &reconciliation::InvoicePaymentRecordInput {
            invoice_id: issued.id.clone(),
            document_id: statement.id.clone(),
            payment_date: Some("2026-03-01".to_string()),
            idempotency_key: idempotency_key.to_string(),
        },
    )
    .await
    .expect("initial payment");

    let error = reconciliation::invoice_payment_record(
        &pool,
        &workspace_id,
        &reconciliation::InvoicePaymentRecordInput {
            invoice_id: issued.id,
            document_id: statement.id,
            payment_date: Some("2026-03-02".to_string()),
            idempotency_key: idempotency_key.to_string(),
        },
    )
    .await
    .expect_err("replayed key with a changed payment date must be rejected");

    assert_eq!(error.code, "validation_error");
    assert_eq!(
        error.details.as_ref().and_then(|details| details[0].field.as_deref()),
        Some("idempotencyKey")
    );
}

#[tokio::test]
async fn invoice_payment_record_accepts_case_insensitive_pdf_mime() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-for-pdf-mime-case").await;

    let statement = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"%PDF-1.4 bank statement",
        "bank-july.pdf",
        "application/PDF",
    )
    .await
    .expect("bank statement");

    reconciliation::invoice_payment_record(
        &pool,
        &workspace_id,
        &reconciliation::InvoicePaymentRecordInput {
            invoice_id: issued.id.clone(),
            document_id: statement.id.clone(),
            payment_date: Some("2026-03-01".to_string()),
            idempotency_key: "pdf-payment-case-1".to_string(),
        },
    )
    .await
    .expect("case-insensitive PDF mime should be accepted");
}

#[tokio::test]
async fn document_store_rejects_mime_mismatch() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let bytes = b"%PDF-1.4 mime mismatch";

    let error = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        bytes,
        "statement.pdf",
        "image/png",
    )
    .await
    .expect_err("mime mismatch");

    assert_eq!(error.code, "validation_error");
}

#[tokio::test]
async fn document_dedupe_keeps_content_addressed_id() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let bytes = b"%PDF-1.4 dedupe mime refresh";

    let first = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        bytes,
        "statement.pdf",
        "application/octet-stream",
    )
    .await
    .expect("octet-stream PDF should sniff to application/pdf");
    assert_eq!(first.mime_type, "application/pdf");

    let second = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        bytes,
        "statement-renamed.pdf",
        "application/pdf",
    )
    .await
    .expect("second import");

    assert_eq!(second.id, first.id);
    assert_eq!(second.mime_type, "application/pdf");
}

#[tokio::test]
async fn invoice_payment_record_rejects_non_pdf_document() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-for-non-pdf-payment").await;

    let receipt = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"\x89PNG\r\n\x1a\nnot a pdf",
        "receipt.png",
        "image/png",
    )
    .await
    .expect("png document");

    let error = reconciliation::invoice_payment_record(
        &pool,
        &workspace_id,
        &reconciliation::InvoicePaymentRecordInput {
            invoice_id: issued.id.clone(),
            document_id: receipt.id.clone(),
            payment_date: Some("2026-03-01".to_string()),
            idempotency_key: "png-payment-1".to_string(),
        },
    )
    .await
    .expect_err("non-pdf evidence must be rejected");

    assert_eq!(error.code, "validation_error");
    assert!(
        error.message.to_lowercase().contains("pdf"),
        "expected PDF requirement message, got: {}",
        error.message
    );
}

#[tokio::test]
async fn invoice_payment_record_rejects_missing_or_tampered_statement_without_persistence() {
    for remove_object in [false, true] {
        let (_dir, workspace_id, pool) = setup_workspace().await;
        let issue_key = Uuid::new_v4().to_string();
        let issued = issue_standard_invoice(&pool, &workspace_id, &issue_key).await;
        let statement = oppenbokforing_desktop_lib::documents::store_document_bytes(
            &pool,
            &workspace_id,
            b"%PDF-1.4 retained bank statement",
            "bank-statement.pdf",
            "application/pdf",
        )
        .await
        .expect("bank statement");
        let documents_path: String =
            sqlx::query_scalar("SELECT documents_path FROM workspaces WHERE id = ?1")
                .bind(&workspace_id)
                .fetch_one(&pool)
                .await
                .expect("documents path");
        let object_path = std::path::Path::new(&documents_path).join(&statement.object_path);
        if remove_object {
            fs::remove_file(&object_path).expect("remove evidence");
        } else {
            fs::write(&object_path, b"%PDF-1.4 tampered bank statement")
                .expect("tamper evidence");
        }
        let idempotency_key = Uuid::new_v4().to_string();

        let error = reconciliation::invoice_payment_record(
            &pool,
            &workspace_id,
            &reconciliation::InvoicePaymentRecordInput {
                invoice_id: issued.id.clone(),
                document_id: statement.id,
                payment_date: Some("2026-03-01".to_string()),
                idempotency_key: idempotency_key.clone(),
            },
        )
        .await
        .expect_err("missing or tampered statement must prevent voucher posting");

        assert_eq!(error.code, "storage_error");
        assert_eq!(error.message, "Retained document integrity check failed");
        let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
              (SELECT COUNT(*) FROM staged_transactions WHERE workspace_id = ?1),
              (SELECT COUNT(*) FROM vouchers WHERE workspace_id = ?1 AND source_type = 'reconciliation' AND source_id = ?2),
              (SELECT COUNT(*) FROM reconciliation_matches WHERE workspace_id = ?1 AND invoice_id = ?2),
              (SELECT COUNT(*) FROM local_jobs WHERE workspace_id = ?1 AND job_type = 'invoice_payment_record' AND idempotency_key = ?3),
              (SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1 AND action = 'invoice_payment_record')
            "#,
        )
        .bind(&workspace_id)
        .bind(&issued.id)
        .bind(&idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("persistence counts");
        assert_eq!(counts, (0, 0, 0, 0, 0));
    }
}

#[tokio::test]
async fn expense_post_rejects_missing_or_tampered_evidence_without_persistence() {
    for remove_object in [false, true] {
        let (_dir, workspace_id, pool) = setup_workspace().await;
        let receipt = oppenbokforing_desktop_lib::documents::store_document_bytes(
            &pool,
            &workspace_id,
            b"%PDF-1.4 retained expense receipt",
            "expense-receipt.pdf",
            "application/pdf",
        )
        .await
        .expect("expense receipt");
        let documents_path: String =
            sqlx::query_scalar("SELECT documents_path FROM workspaces WHERE id = ?1")
                .bind(&workspace_id)
                .fetch_one(&pool)
                .await
                .expect("documents path");
        let object_path = std::path::Path::new(&documents_path).join(&receipt.object_path);
        if remove_object {
            fs::remove_file(&object_path).expect("remove evidence");
        } else {
            fs::write(&object_path, b"%PDF-1.4 tampered expense receipt")
                .expect("tamper evidence");
        }
        let idempotency_key = Uuid::new_v4().to_string();

        let error = oppenbokforing_desktop_lib::expenses::expense_post(
            &pool,
            &workspace_id,
            &oppenbokforing_desktop_lib::expenses::ExpensePostInput {
                amount_minor_ex_vat: 10_000,
                vat_rate: 0.25,
                expense_account_number: "5610".to_string(),
                payment_account_number: "1930".to_string(),
                document_id: Some(receipt.id),
                no_document_reason: None,
                staged_transaction_id: None,
                idempotency_key: idempotency_key.clone(),
                date: Some("2026-03-01".to_string()),
            },
        )
        .await
        .expect_err("missing or tampered receipt must prevent voucher posting");

        assert_eq!(error.code, "storage_error");
        assert_eq!(error.message, "Retained document integrity check failed");
        let counts: (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
              (SELECT COUNT(*) FROM vouchers WHERE workspace_id = ?1 AND source_type = 'expense'),
              (SELECT COUNT(*) FROM local_jobs WHERE workspace_id = ?1 AND job_type = 'expense_post' AND idempotency_key = ?2),
              (SELECT COUNT(*) FROM audit_events WHERE workspace_id = ?1 AND action = 'expense_post')
            "#,
        )
        .bind(&workspace_id)
        .bind(&idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("persistence counts");
        assert_eq!(counts, (0, 0, 0));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn expense_post_rejects_documents_root_symlink_outside_workspace() {
    let (dir, workspace_id, pool) = setup_workspace().await;
    let receipt = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"%PDF-1.4 retained expense receipt",
        "expense-receipt.pdf",
        "application/pdf",
    )
    .await
    .expect("expense receipt");
    let documents_path: String =
        sqlx::query_scalar("SELECT documents_path FROM workspaces WHERE id = ?1")
            .bind(&workspace_id)
            .fetch_one(&pool)
            .await
            .expect("documents path");
    let retained_documents = dir.path().join("retained-documents");
    fs::rename(&documents_path, &retained_documents).expect("move retained documents");
    let external_documents = dir.path().join("external-documents");
    fs::create_dir_all(external_documents.join("objects")).expect("external objects");
    fs::copy(
        retained_documents.join(&receipt.object_path),
        external_documents.join(&receipt.object_path),
    )
    .expect("copy retained evidence");
    std::os::unix::fs::symlink(&external_documents, &documents_path)
        .expect("replace configured documents root with symlink");

    let error = oppenbokforing_desktop_lib::expenses::expense_post(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::expenses::ExpensePostInput {
            amount_minor_ex_vat: 10_000,
            vat_rate: 0.25,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: Some(receipt.id),
            no_document_reason: None,
            staged_transaction_id: None,
            idempotency_key: "symlinked-documents-root".to_string(),
            date: Some("2026-03-01".to_string()),
        },
    )
    .await
    .expect_err("documents root symlink must not be trusted");

    assert_eq!(error.code, "storage_error");
    assert_eq!(error.message, "Retained document integrity check failed");
    let voucher_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vouchers WHERE workspace_id = ?1 AND source_type = 'expense'",
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("voucher count");
    assert_eq!(voucher_count, 0);
}

#[tokio::test]
async fn posted_voucher_immutability_triggers_reject_mutation() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let issued = issue_standard_invoice(&pool, &workspace_id, "issue-for-immutability").await;
    let voucher_id = issued
        .voucher_id
        .as_ref()
        .expect("issued invoice should have a posted voucher");

    let update_err = sqlx::query("UPDATE vouchers SET status = 'draft' WHERE id = ?1")
        .bind(voucher_id)
        .execute(&pool)
        .await
        .expect_err("posted voucher update must abort");
    assert!(
        update_err.to_string().to_lowercase().contains("posted"),
        "expected posted-voucher abort, got: {update_err}"
    );

    let delete_err = sqlx::query("DELETE FROM vouchers WHERE id = ?1")
        .bind(voucher_id)
        .execute(&pool)
        .await
        .expect_err("posted voucher delete must abort");
    assert!(
        delete_err.to_string().to_lowercase().contains("posted"),
        "expected posted-voucher abort, got: {delete_err}"
    );

    let line_id: String = sqlx::query_scalar(
        "SELECT id FROM journal_lines WHERE voucher_id = ?1 LIMIT 1",
    )
    .bind(voucher_id)
    .fetch_one(&pool)
    .await
    .expect("journal line");

    let line_update_err =
        sqlx::query("UPDATE journal_lines SET debit_minor = debit_minor + 1 WHERE id = ?1")
            .bind(&line_id)
            .execute(&pool)
            .await
            .expect_err("posted journal line update must abort");
    assert!(
        line_update_err.to_string().to_lowercase().contains("posted")
            || line_update_err.to_string().to_lowercase().contains("journal"),
        "expected journal immutability abort, got: {line_update_err}"
    );
}
