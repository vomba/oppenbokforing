use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceIssueInput, InvoiceLineInput,
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
}

#[tokio::test]
async fn invoice_pdf_job_requeues_after_transient_failure() {
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
    assert_eq!(job_status, "queued");

    profiles::save_business_profile(
        &pool,
        &workspace_id,
        &profiles::BusinessProfileSaveInput {
            business_name: "PDF Test Firma".to_string(),
            owner_name: "Owner".to_string(),
            residency_country: Some("SE".to_string()),
            sni_code: Some("62010".to_string()),
        },
    )
    .await
    .expect("restore business");

    let processed_retry = jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
        .await
        .expect("retry job");
    assert_eq!(processed_retry, 1);

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
async fn invoice_pdf_batch_processes_remaining_jobs_after_requeue() {
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
        assert_eq!(job_status, "queued");
    }

    profiles::save_business_profile(
        &pool,
        &workspace_id,
        &profiles::BusinessProfileSaveInput {
            business_name: "PDF Test Firma".to_string(),
            owner_name: "Owner".to_string(),
            residency_country: Some("SE".to_string()),
            sni_code: Some("62010".to_string()),
        },
    )
    .await
    .expect("restore business");

    let processed_retry = jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id)
        .await
        .expect("retry jobs");
    assert_eq!(processed_retry, 2);

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
