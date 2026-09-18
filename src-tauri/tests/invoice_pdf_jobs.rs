use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    documents,
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceIssueInput, InvoiceLineInput,
        LegacyIssuedInvoiceSnapshotRecoveryInput,
    },
    jobs,
    profiles::{self, BusinessProfileSaveInput, TaxProfileSaveInput, VatProfileSaveInput},
    workspace::ensure_workspace_ready,
};
use sqlx::Row;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_invoice_workspace() -> (tempfile::TempDir, String, sqlx::SqlitePool) {
    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let data_dir = dir.path().join(&workspace_id);
    std::fs::create_dir_all(data_dir.join("documents")).expect("documents");
    std::fs::create_dir_all(data_dir.join("exports")).expect("exports");
    let database_path = data_dir.join("workspace.sqlite");
    let pool = connect_workspace(&database_path).await.expect("connect");

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("PDF job workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace");

    ensure_workspace_ready(&pool, &workspace_id)
        .await
        .expect("bootstrap");

    profiles::save_business_profile(
        &pool,
        &workspace_id,
        &BusinessProfileSaveInput {
            business_name: "PDF Test Firma".to_string(),
            owner_name: "Owner".to_string(),
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

#[tokio::test]
async fn render_and_store_invoice_pdf_bytes() {
    let (_dir, workspace_id, pool) = setup_invoice_workspace().await;
    let invoice = invoicing::InvoiceSummary {
        id: "inv-1".to_string(),
        counterparty_id: "cp-1".to_string(),
        counterparty_name: "Customer".to_string(),
        status: "issued".to_string(),
        invoice_kind: "standard".to_string(),
        invoice_number: Some("2026-0001".to_string()),
        source_invoice_id: None,
        issue_date: Some("2026-01-15".to_string()),
        due_date: Some("2026-03-01".to_string()),
        total_ex_vat_minor: 10_000_00,
        total_vat_minor: 2_500_00,
        total_inc_vat_minor: 12_500_00,
        pdf_job_id: None,
        pdf_document_id: None,
        voucher_id: None,
        payment_voucher_id: None,
        lines: vec![],
    };

    let bytes = oppenbokforing_desktop_lib::invoicing::pdf::render_invoice_pdf(
        &invoice,
        &oppenbokforing_desktop_lib::invoicing::pdf::InvoicePdfContext {
            business_name: "PDF Test Firma".to_string(),
            owner_name: "Owner".to_string(),
            tax_status: "f_skatt".to_string(),
            vat_status: "registered".to_string(),
            rule_version_id: "rv-2026-active".to_string(),
            tax_year: 2026,
            source_url: "https://example.test/rules".to_string(),
        },
    )
    .expect("render pdf");
    assert!(bytes.starts_with(b"%PDF"));

    let document = oppenbokforing_desktop_lib::documents::store_document_bytes(
        &pool,
        &workspace_id,
        &bytes,
        "invoice-2026-0001.pdf",
        "application/pdf",
    )
    .await
    .expect("store pdf");

    assert_eq!(document.mime_type, "application/pdf");
}

#[tokio::test]
async fn issued_invoice_pdf_job_archives_document() {
    let (_dir, workspace_id, pool) = setup_invoice_workspace().await;

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "PDF Customer".to_string(),
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
            due_date: Some("2026-03-01".to_string()),
            lines: vec![InvoiceLineInput {
                description: "Consulting".to_string(),
                quantity: 1,
                unit_price_minor: 10_000_00,
                vat_rate: 0.25,
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
            invoice_id: draft.id.clone(),
            idempotency_key: "pdf-issue-1".to_string(),
            issue_date: Some("2026-01-15".to_string()),
        },
    )
    .await
    .expect("issue");

    assert!(issued.pdf_job_id.is_some());

    let processed = jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
        .await
        .expect("process jobs");
    assert_eq!(processed, 1);

    let invoice = invoicing::get_invoice(&pool, &workspace_id, &issued.id)
        .await
        .expect("invoice");
    assert!(invoice.pdf_document_id.is_some());

    let document_id = invoice.pdf_document_id.clone().expect("document id");
    let row = sqlx::query(
        r#"
        SELECT object_path, mime_type, original_filename
        FROM documents
        WHERE workspace_id = ?1 AND id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&document_id)
    .fetch_one(&pool)
    .await
    .expect("document");

    let object_path: String = row.get("object_path");
    let mime_type: String = row.get("mime_type");
    assert_eq!(mime_type, "application/pdf");
    assert!(object_path.starts_with("objects/"));

    let documents_path: String = sqlx::query_scalar(
        r#"
        SELECT documents_path FROM workspaces WHERE id = ?1
        "#,
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("documents path");

    let pdf_path = std::path::Path::new(&documents_path).join(&object_path);
    let bytes = std::fs::read(&pdf_path).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF"));

    let replay = jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
        .await
        .expect("replay");
    assert_eq!(replay, 0);

    let invoice_after = invoicing::get_invoice(&pool, &workspace_id, &issued.id)
        .await
        .expect("invoice after");
    assert_eq!(invoice_after.pdf_document_id, invoice.pdf_document_id);
    std::fs::write(&pdf_path, b"%PDF-1.4 tampered invoice").expect("tamper invoice PDF");
    let error = jobs::invoice_pdf_status(&pool, &workspace_id, &invoice_after)
        .await
        .expect_err("tampered invoice PDF must not be reported as valid");
    assert_eq!(error.code, "storage_error");
    assert_eq!(error.message, "Retained document integrity check failed");
}

#[tokio::test]
async fn invoice_pdf_job_uses_issued_snapshot_after_profile_removal() {
    let (dir, workspace_id, pool) = setup_invoice_workspace().await;
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Retry Customer".to_string(),
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
            due_date: Some("2026-03-01".to_string()),
            lines: vec![InvoiceLineInput {
                description: "Retry line".to_string(),
                quantity: 1,
                unit_price_minor: 1_000_00,
                vat_rate: 0.25,
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
            invoice_id: draft.id.clone(),
            idempotency_key: "pdf-retry-issue".to_string(),
            issue_date: Some("2026-01-15".to_string()),
        },
    )
    .await
    .expect("issue");

    let snapshot = sqlx::query(
        r#"
        SELECT business_name, owner_name, tax_status, vat_status, rule_version_id, tax_year, source_url
        FROM invoice_issue_snapshots
        WHERE workspace_id = ?1 AND invoice_id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&issued.id)
    .fetch_one(&pool)
    .await
    .expect("issued snapshot");
    assert_eq!(snapshot.get::<String, _>("business_name"), "PDF Test Firma");
    assert_eq!(snapshot.get::<String, _>("owner_name"), "Owner");
    assert_eq!(snapshot.get::<String, _>("tax_status"), "f_skatt");
    assert_eq!(snapshot.get::<String, _>("vat_status"), "registered");
    assert_eq!(
        snapshot.get::<String, _>("rule_version_id"),
        "rv-2026-active"
    );
    assert_eq!(snapshot.get::<i64, _>("tax_year"), 2026);
    assert!(
        snapshot
            .get::<String, _>("source_url")
            .starts_with("https://")
    );

    let issue_audit_metadata: String = sqlx::query_scalar(
        r#"
        SELECT metadata_json FROM audit_events
        WHERE workspace_id = ?1 AND action = 'invoice_issue' AND resource_id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&issued.id)
    .fetch_one(&pool)
    .await
    .expect("issue audit event");
    assert!(issue_audit_metadata.contains("\"businessName\":\"PDF Test Firma\""));
    assert!(issue_audit_metadata.contains("\"ruleVersionId\":\"rv-2026-active\""));

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "planning".to_string(),
            expected_business_profit_minor: Some(1_000_000),
            expected_salary_income_minor: Some(0),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("change tax profile");
    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "exempt_low_turnover".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: None,
        },
    )
    .await
    .expect("change VAT profile");

    sqlx::query("DELETE FROM sole_trader_profiles WHERE workspace_id = ?1")
        .bind(&workspace_id)
        .execute(&pool)
        .await
        .expect("remove business profile");

    let processed = jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
        .await
        .expect("process failing job");
    assert_eq!(processed, 1);

    let job_status: String = sqlx::query_scalar(
        r#"
        SELECT status FROM local_jobs
        WHERE workspace_id = ?1 AND id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(issued.pdf_job_id.as_ref().expect("job id"))
    .fetch_one(&pool)
    .await
    .expect("job status");
    assert_eq!(job_status, "succeeded");

    jobs::refresh_invoice_pdf(&pool, &workspace_id, &issued.id)
        .await
        .expect("refresh PDF from issued snapshot");
    let refresh_job_status: String = sqlx::query_scalar(
        r#"
        SELECT status FROM local_jobs
        WHERE workspace_id = ?1 AND id = (
          SELECT pdf_job_id FROM invoices WHERE workspace_id = ?1 AND id = ?2
        )
        "#,
    )
    .bind(&workspace_id)
    .bind(&issued.id)
    .fetch_one(&pool)
    .await
    .expect("refresh job status");
    assert_eq!(refresh_job_status, "succeeded");


    let processed_retry = jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
        .await
        .expect("retry job");
    assert_eq!(processed_retry, 0);

    let invoice = invoicing::get_invoice(&pool, &workspace_id, &issued.id)
        .await
        .expect("invoice");
    assert!(invoice.pdf_document_id.is_some());
    assert_eq!(
        jobs::invoice_pdf_status(&pool, &workspace_id, &invoice)
            .await
            .expect("status"),
        "succeeded"
    );

    drop(dir);
}

#[tokio::test]
async fn legacy_issued_invoice_pdf_job_requires_explicit_snapshot_recovery() {
    let (_dir, workspace_id, pool) = setup_invoice_workspace().await;
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Legacy Customer".to_string(),
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
                description: "Legacy invoice".to_string(),
                quantity: 1,
                unit_price_minor: 1_000_00,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");
    sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'issued', invoice_number = '2026-9999', issue_date = '2026-01-15'
        WHERE workspace_id = ?1 AND id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&draft.id)
    .execute(&pool)
    .await
    .expect("legacy issued invoice");
    let job_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json)
        VALUES (?1, ?2, 'invoice_pdf_generate', 'queued', ?3)
        "#,
    )
    .bind(&job_id)
    .bind(&workspace_id)
    .bind(
        serde_json::json!({
            "invoiceId": draft.id,
            "invoiceNumber": "2026-9999",
            "format": "pdf"
        })
        .to_string(),
    )
    .execute(&pool)
    .await
    .expect("legacy PDF job");

    assert_eq!(
        jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
            .await
            .expect("process legacy PDF job"),
        1
    );
    let (status, last_error): (String, Option<String>) = sqlx::query_as(
        "SELECT status, last_error FROM local_jobs WHERE id = ?1",
    )
    .bind(&job_id)
    .fetch_one(&pool)
    .await
    .expect("legacy job result");
    assert_eq!(status, "queued");
    assert_eq!(
        last_error.as_deref(),
        Some("Issued invoice snapshot is missing; explicit recovery is required before PDF generation")
    );
    profiles::save_business_profile(
        &pool,
        &workspace_id,
        &BusinessProfileSaveInput {
            business_name: "Mutable Backfill Profile".to_string(),
            owner_name: "Mutable Owner".to_string(),
            residency_country: Some("SE".to_string()),
            sni_code: None,
        },
    )
    .await
    .expect("change current business profile");
    let credit_error = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &invoicing::InvoiceCreditInput {
            source_invoice_id: draft.id.clone(),
            idempotency_key: "legacy-credit-denied".to_string(),
            reason: Some("Correction".to_string()),
            issue_date: Some("2026-01-16".to_string()),
        },
    )
    .await
    .expect_err("legacy invoice must not backfill from mutable profiles");
    assert_eq!(
        credit_error.message,
        "Issued invoice snapshot is missing; explicit recovery is required before crediting"
    );
    let snapshot_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoice_issue_snapshots WHERE workspace_id = ?1 AND invoice_id = ?2",
    )
    .bind(&workspace_id)
    .bind(&draft.id)
    .fetch_one(&pool)
    .await
    .expect("snapshot count");
    assert_eq!(snapshot_count, 0);
}

#[tokio::test]
async fn invoice_pdf_batch_uses_issued_snapshots_after_profile_removal() {
    let (dir, workspace_id, pool) = setup_invoice_workspace().await;
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Batch Customer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");

    let mut issued = Vec::new();
    for index in 0..2 {
        let draft = invoicing::create_draft(
            &pool,
            &workspace_id,
            &InvoiceCreateDraftInput {
                counterparty_id: customer.id.clone(),
                due_date: Some("2026-03-01".to_string()),
                lines: vec![InvoiceLineInput {
                    description: format!("Batch line {index}"),
                    quantity: 1,
                    unit_price_minor: 1_000_00,
                    vat_rate: 0.25,
                    account_number: Some("3041".to_string()),
                }],
            },
        )
        .await
        .expect("draft");

        let invoice = invoicing::issue_invoice(
            &pool,
            &workspace_id,
            &InvoiceIssueInput {
                invoice_id: draft.id.clone(),
                idempotency_key: format!("pdf-batch-issue-{index}"),
                issue_date: Some("2026-01-15".to_string()),
            },
        )
        .await
        .expect("issue");
        issued.push(invoice);
    }

    sqlx::query("DELETE FROM sole_trader_profiles WHERE workspace_id = ?1")
        .bind(&workspace_id)
        .execute(&pool)
        .await
        .expect("remove business profile");

    let processed = jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
        .await
        .expect("process failing jobs");
    assert_eq!(processed, 2);

    for invoice in &issued {
        let job_status: String = sqlx::query_scalar(
            r#"
            SELECT status FROM local_jobs
            WHERE workspace_id = ?1 AND id = ?2
            "#,
        )
        .bind(&workspace_id)
        .bind(invoice.pdf_job_id.as_ref().expect("job id"))
        .fetch_one(&pool)
        .await
        .expect("job status");
        assert_eq!(job_status, "succeeded");
    }


    let processed_retry = jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
        .await
        .expect("retry jobs");
    assert_eq!(processed_retry, 0);

    for invoice in issued {
        let refreshed = invoicing::get_invoice(&pool, &workspace_id, &invoice.id)
            .await
            .expect("invoice");
        assert!(refreshed.pdf_document_id.is_some());
    }

    drop(dir);
}

#[tokio::test]
async fn invoice_pdf_status_ignores_orphaned_document_reference() {
    let (dir, workspace_id, pool) = setup_invoice_workspace().await;
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Status Customer".to_string(),
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
                description: "Status line".to_string(),
                quantity: 1,
                unit_price_minor: 1_000_00,
                vat_rate: 0.25,
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
            invoice_id: draft.id.clone(),
            idempotency_key: "pdf-status-issue".to_string(),
            issue_date: Some("2026-01-15".to_string()),
        },
    )
    .await
    .expect("issue");

    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .expect("disable fk");
    sqlx::query(
        r#"
        UPDATE invoices
        SET pdf_document_id = 'missing-document-id'
        WHERE workspace_id = ?1 AND id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&issued.id)
    .execute(&pool)
    .await
    .expect("orphan document reference");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .expect("enable fk");

    let invoice = invoicing::get_invoice(&pool, &workspace_id, &issued.id)
        .await
        .expect("invoice");
    let status = jobs::invoice_pdf_status(&pool, &workspace_id, &invoice)
        .await
        .expect("status");
    assert_eq!(status, "queued");

    drop(dir);
}

#[tokio::test]
async fn refresh_invoice_pdf_skips_duplicate_queued_job() {
    let (dir, workspace_id, pool) = setup_invoice_workspace().await;
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Refresh Customer".to_string(),
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
            due_date: Some("2026-03-01".to_string()),
            lines: vec![InvoiceLineInput {
                description: "Refresh line".to_string(),
                quantity: 1,
                unit_price_minor: 1_000_00,
                vat_rate: 0.25,
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
            invoice_id: draft.id.clone(),
            idempotency_key: "pdf-refresh-dedupe".to_string(),
            issue_date: Some("2026-01-15".to_string()),
        },
    )
    .await
    .expect("issue");

    let queued_after_issue: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM local_jobs
        WHERE workspace_id = ?1
          AND job_type = 'invoice_pdf_generate'
          AND status IN ('queued', 'running')
          AND json_extract(payload_json, '$.invoiceId') = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&issued.id)
    .fetch_one(&pool)
    .await
    .expect("queued count after issue");
    assert_eq!(queued_after_issue, 1);

    jobs::refresh_invoice_pdf(&pool, &workspace_id, &issued.id)
        .await
        .expect("refresh while job pending");

    let queued_after_refresh: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM local_jobs
        WHERE workspace_id = ?1
          AND job_type = 'invoice_pdf_generate'
          AND status IN ('queued', 'running')
          AND json_extract(payload_json, '$.invoiceId') = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&issued.id)
    .fetch_one(&pool)
    .await
    .expect("queued count after refresh");
    assert_eq!(queued_after_refresh, 1);

    drop(dir);
}

#[tokio::test]
async fn legacy_issued_invoice_refresh_preserves_existing_pdf_and_records_recovery_requirement() {
    let (dir, workspace_id, pool) = setup_invoice_workspace().await;
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Legacy PDF Customer".to_string(),
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
                description: "Legacy PDF invoice".to_string(),
                quantity: 1,
                unit_price_minor: 1_000_00,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");
    let historical_pdf = documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"%PDF-1.4\nlegacy historical invoice\n",
        "invoice-2026-9998.pdf",
        "application/pdf",
    )
    .await
    .expect("historical PDF");
    sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'issued',
            invoice_number = '2026-9998',
            issue_date = '2026-01-15',
            pdf_document_id = ?1
        WHERE workspace_id = ?2 AND id = ?3
        "#,
    )
    .bind(&historical_pdf.id)
    .bind(&workspace_id)
    .bind(&draft.id)
    .execute(&pool)
    .await
    .expect("legacy issued invoice");

    jobs::refresh_invoice_pdf(&pool, &workspace_id, &draft.id)
        .await
        .expect("preserve readable historical PDF");

    let invoice = invoicing::get_invoice(&pool, &workspace_id, &draft.id)
        .await
        .expect("invoice");
    assert_eq!(invoice.pdf_document_id.as_deref(), Some(historical_pdf.id.as_str()));
    let (refresh_job_status, refresh_error): (String, Option<String>) = sqlx::query_as(
        r#"
        SELECT status, last_error FROM local_jobs
        WHERE workspace_id = ?1 AND id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(invoice.pdf_job_id.as_ref().expect("refresh job id"))
    .fetch_one(&pool)
    .await
    .expect("refresh job status");
    assert_eq!(refresh_job_status, "queued");
    assert_eq!(
        refresh_error.as_deref(),
        Some("Issued invoice snapshot is missing; explicit recovery is required before PDF generation")
    );
    assert_eq!(
        jobs::invoice_pdf_status(&pool, &workspace_id, &invoice)
            .await
            .expect("status"),
        "succeeded"
    );

    let recovery = invoicing::legacy_issued_invoice_snapshot_recovery_status(
        &pool,
        &workspace_id,
        &draft.id,
    )
    .await
    .expect("recovery state");
    assert!(recovery.recovery_required);
    assert_eq!(
        recovery.preserved_pdf_document_id.as_deref(),
        Some(historical_pdf.id.as_str())
    );

    let recovery_audit_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM audit_events
        WHERE workspace_id = ?1
          AND action = 'invoice_snapshot_recovery_required'
          AND resource_id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&draft.id)
    .fetch_one(&pool)
    .await
    .expect("recovery audit event");
    assert_eq!(recovery_audit_count, 1);

    drop(dir);
}

#[tokio::test]
async fn explicit_legacy_snapshot_recovery_uses_attested_historical_values_not_profiles() {
    let (dir, workspace_id, pool) = setup_invoice_workspace().await;
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Recovery Customer".to_string(),
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
                description: "Recovery invoice".to_string(),
                quantity: 1,
                unit_price_minor: 1_000_00,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");
    let historical_pdf = documents::store_document_bytes(
        &pool,
        &workspace_id,
        b"%PDF-1.4\nhistorical invoice recovery evidence\n",
        "invoice-2026-9997.pdf",
        "application/pdf",
    )
    .await
    .expect("historical PDF");
    sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'issued',
            invoice_number = '2026-9997',
            issue_date = '2026-01-15',
            pdf_document_id = ?1
        WHERE workspace_id = ?2 AND id = ?3
        "#,
    )
    .bind(&historical_pdf.id)
    .bind(&workspace_id)
    .bind(&draft.id)
    .execute(&pool)
    .await
    .expect("legacy issued invoice");

    profiles::save_business_profile(
        &pool,
        &workspace_id,
        &BusinessProfileSaveInput {
            business_name: "Changed Current Profile".to_string(),
            owner_name: "Changed Owner".to_string(),
            residency_country: Some("SE".to_string()),
            sni_code: None,
        },
    )
    .await
    .expect("change profile");
    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "fa_skatt".to_string(),
            expected_business_profit_minor: Some(1_000_000),
            expected_salary_income_minor: Some(600_000),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("change tax profile");
    sqlx::query("UPDATE rule_versions SET status = 'retired' WHERE id = 'rv-2026-active'")
        .execute(&pool)
        .await
        .expect("retire historical rule");
    sqlx::query(
        r#"
        INSERT INTO rule_versions (
          id, tax_year, effective_from, effective_to, source_url, checksum, status
        ) VALUES (?1, 2026, '2026-02-01', NULL, ?2, ?3, 'active')
        "#,
    )
    .bind("rv-2026-replacement")
    .bind("https://example.test/rules/2026-replacement")
    .bind("sha256:2026-replacement")
    .execute(&pool)
    .await
    .expect("activate replacement rule");

    let recovery_input = LegacyIssuedInvoiceSnapshotRecoveryInput {
        invoice_id: draft.id.clone(),
        document_id: historical_pdf.id.clone(),
        business_name: "Historical Firma".to_string(),
        owner_name: "Historical Owner".to_string(),
        tax_status: "f_skatt".to_string(),
        vat_status: "registered".to_string(),
        rule_version_id: "rv-2026-active".to_string(),
        attestation: "I attest that the business identity and displayed tax/VAT wording were transcribed from the retained original invoice PDF, and that any status distinctions and the rule version were checked against contemporaneous records."
            .to_string(),
    };
    invoicing::recover_legacy_issued_invoice_snapshot(&pool, &workspace_id, &recovery_input)
        .await
        .expect("explicit recovery");
    invoicing::recover_legacy_issued_invoice_snapshot(&pool, &workspace_id, &recovery_input)
        .await
        .expect("idempotent recovery retry");

    let snapshot = invoicing::get_issued_invoice_snapshot(&pool, &workspace_id, &draft.id)
        .await
        .expect("recovered snapshot");
    assert_eq!(snapshot.business_name, "Historical Firma");
    assert_eq!(snapshot.owner_name, "Historical Owner");
    assert_eq!(snapshot.tax_status, "f_skatt");
    assert_eq!(snapshot.rule_version_id, "rv-2026-active");
    assert_eq!(snapshot.tax_year, 2026);

    let recovery_audit_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM audit_events
        WHERE workspace_id = ?1
          AND action = 'invoice_snapshot_recovered'
          AND resource_id = ?2
        "#,
    )
    .bind(&workspace_id)
    .bind(&draft.id)
    .fetch_one(&pool)
    .await
    .expect("recovery audit event");
    assert_eq!(recovery_audit_count, 1);

    drop(dir);
}
