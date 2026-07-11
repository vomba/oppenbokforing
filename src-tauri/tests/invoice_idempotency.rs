use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceCreditInput, InvoiceIssueInput, InvoiceLineInput,
        InvoiceUpdateDraftInput,
    },
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    workspace::ensure_workspace_ready,
};
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_workspace() -> (tempfile::TempDir, String, sqlx::SqlitePool) {
    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .expect("connect");

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("Idempotency test workspace")
    .bind(dir.path().join("workspace.sqlite").to_string_lossy().to_string())
    .bind(dir.path().join("documents").to_string_lossy().to_string())
    .bind(dir.path().join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace");

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

async fn create_customer(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
) -> oppenbokforing_desktop_lib::counterparties::Counterparty {
    counterparties::create_counterparty(
        pool,
        workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Idempotency Customer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer")
}

#[tokio::test]
async fn issue_invoice_idempotency_replay_returns_cached() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: Some("2026-03-01".to_string()),
            lines: vec![InvoiceLineInput {
                description: "Service".to_string(),
                quantity: 1,
                unit_price_minor: 500_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

    let first = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "issue-replay-key".to_string(),
            issue_date: Some("2026-01-20".to_string()),
        },
    )
    .await
    .expect("issue");

    let replay = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "issue-replay-key".to_string(),
            issue_date: Some("2026-01-20".to_string()),
        },
    )
    .await
    .expect("replay");

    assert_eq!(first.id, replay.id);
    assert_eq!(replay.invoice_number.as_deref(), Some("2026-0001"));
}

#[tokio::test]
async fn credit_invoice_idempotency_replay_returns_cached() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Billable hours".to_string(),
                quantity: 2,
                unit_price_minor: 100_000,
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
            idempotency_key: "issue-for-credit".to_string(),
            issue_date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect("issue");

    let first = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id.clone(),
            idempotency_key: "credit-replay-key".to_string(),
            reason: Some("Correction".to_string()),
        },
    )
    .await
    .expect("credit");

    let replay = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id,
            idempotency_key: "credit-replay-key".to_string(),
            reason: Some("Correction".to_string()),
        },
    )
    .await
    .expect("credit replay");

    assert_eq!(first.id, replay.id);
    assert_eq!(first.invoice_kind, "credit_note");
}

#[tokio::test]
async fn credit_invoice_rejects_second_credit_for_same_source() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![
                InvoiceLineInput {
                    description: "Line A".to_string(),
                    quantity: 1,
                    unit_price_minor: 100_000,
                    vat_rate: 0.25,
                    account_number: Some("3041".to_string()),
                },
                InvoiceLineInput {
                    description: "Line B".to_string(),
                    quantity: 1,
                    unit_price_minor: 200_000,
                    vat_rate: 0.25,
                    account_number: Some("3041".to_string()),
                },
            ],
        },
    )
    .await
    .expect("draft");

    let issued = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "issue-multi-line".to_string(),
            issue_date: Some("2026-02-10".to_string()),
        },
    )
    .await
    .expect("issue");

    let _first = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id.clone(),
            idempotency_key: "credit-first".to_string(),
            reason: None,
        },
    )
    .await
    .expect("first credit");

    let second = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id,
            idempotency_key: "credit-second".to_string(),
            reason: None,
        },
    )
    .await
    .expect("second credit returns existing");

    assert_eq!(second.invoice_kind, "credit_note");
    assert_eq!(second.lines.len(), 2);
}

#[tokio::test]
async fn issue_idempotency_rejects_key_reused_for_different_invoice() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;

    let first_draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id.clone(),
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "First".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("first draft");

    let second_draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Second".to_string(),
                quantity: 1,
                unit_price_minor: 200_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("second draft");

    invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: first_draft.id,
            idempotency_key: "shared-issue-key".to_string(),
            issue_date: Some("2026-01-10".to_string()),
        },
    )
    .await
    .expect("issue first");

    let err = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: second_draft.id,
            idempotency_key: "shared-issue-key".to_string(),
            issue_date: Some("2026-01-11".to_string()),
        },
    )
    .await
    .expect_err("reject mismatched invoice");

    assert_eq!(err.code, "validation_error");
}

#[tokio::test]
async fn issue_uses_fiscal_year_from_issue_date() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Cross-year draft".to_string(),
                quantity: 1,
                unit_price_minor: 300_000,
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
            idempotency_key: "issue-2027".to_string(),
            issue_date: Some("2027-01-15".to_string()),
        },
    )
    .await
    .expect("issue in 2027");

    assert_eq!(issued.invoice_number.as_deref(), Some("2027-0001"));

    let fiscal_year_id: String = sqlx::query_scalar(
        r#"
        SELECT fiscal_year_id FROM invoices WHERE id = ?1
        "#,
    )
    .bind(&draft.id)
    .fetch_one(&pool)
    .await
    .expect("fiscal year");

    assert_eq!(fiscal_year_id, format!("fy-{workspace_id}-2027"));
}

#[tokio::test]
async fn issue_already_issued_invoice_with_new_key_returns_success() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Retry after success".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

    let first = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "issue-first-key".to_string(),
            issue_date: Some("2026-03-01".to_string()),
        },
    )
    .await
    .expect("issue");

    let retry = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "issue-retry-new-key".to_string(),
            issue_date: Some("2026-03-01".to_string()),
        },
    )
    .await
    .expect("re-issue should return issued invoice");

    assert_eq!(first.id, retry.id);
    assert_eq!(retry.status, "issued");
    assert_eq!(retry.invoice_number.as_deref(), Some("2026-0001"));
}

#[tokio::test]
async fn issue_unknown_invoice_returns_not_found() {
    let (_dir, workspace_id, pool) = setup_workspace().await;

    let err = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: Uuid::new_v4().to_string(),
            idempotency_key: "issue-missing".to_string(),
            issue_date: Some("2026-01-01".to_string()),
        },
    )
    .await
    .expect_err("missing invoice");

    assert_eq!(err.code, "validation_error");
    assert!(
        err.details
            .as_ref()
            .and_then(|details| details.first())
            .and_then(|field| field.field.as_deref())
            == Some("invoiceId")
    );
}

#[tokio::test]
async fn create_draft_rejects_foreign_counterparty() {
    let (dir, workspace_id, pool) = setup_workspace().await;
    let other_workspace_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&other_workspace_id)
    .bind("Other workspace")
    .bind(dir.path().join("other.sqlite").to_string_lossy().to_string())
    .bind(dir.path().join("other-docs").to_string_lossy().to_string())
    .bind(dir.path().join("other-exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("other workspace");

    let foreign_customer = counterparties::create_counterparty(
        &pool,
        &other_workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Foreign customer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("foreign customer");

    let err = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: foreign_customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Should fail".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect_err("foreign counterparty");

    assert_eq!(err.code, "validation_error");
    assert!(
        err.details
            .as_ref()
            .and_then(|details| details.first())
            .and_then(|field| field.field.as_deref())
            == Some("counterpartyId")
    );
}

#[tokio::test]
async fn update_draft_rejects_when_invoice_is_issued() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Draft".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

    invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "issue-before-update".to_string(),
            issue_date: Some("2026-04-01".to_string()),
        },
    )
    .await
    .expect("issued");

    let err = invoicing::update_draft(
        &pool,
        &workspace_id,
        &InvoiceUpdateDraftInput {
            invoice_id: draft.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Attempted update".to_string(),
                quantity: 1,
                unit_price_minor: 200_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect_err("should reject issued invoice update");

    assert_eq!(err.code, "validation_error");
    assert!(
        err.details
            .as_ref()
            .and_then(|details| details.first())
            .and_then(|field| field.field.as_deref())
            == Some("invoiceId")
    );
}

#[tokio::test]
async fn issue_invoice_rejects_invalid_issue_date_format() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Bad date".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

    let err = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "bad-date".to_string(),
            issue_date: Some("2026-not-a-date".to_string()),
        },
    )
    .await
    .expect_err("invalid date");

    assert_eq!(err.code, "validation_error");
    assert!(
        err.details
            .as_ref()
            .and_then(|details| details.first())
            .and_then(|field| field.field.as_deref())
            == Some("issueDate")
    );
}
