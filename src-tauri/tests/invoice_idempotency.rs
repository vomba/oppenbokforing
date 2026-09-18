use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceCreditInput, InvoiceIssueInput,
        InvoiceIssuePreflightInput, InvoiceLineInput, InvoiceUpdateDraftInput,
    },
    profiles::{self, BusinessProfileSaveInput, TaxProfileSaveInput, VatProfileSaveInput},
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

    profiles::save_business_profile(
        &pool,
        &workspace_id,
        &BusinessProfileSaveInput {
            business_name: "Idempotency Test Firma".to_string(),
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
async fn planning_tax_profile_rejects_issue_before_persisting_anything() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;
    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Tax status gate".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

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
    .expect("planning tax profile");

    let error = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "planning-tax-status".to_string(),
            issue_date: Some("2026-01-20".to_string()),
        },
    )
    .await
    .expect_err("planning tax status must not issue an invoice");

    assert_eq!(error.code, "validation_error");
    assert_eq!(
        error.details.as_ref().and_then(|details| details[0].field.as_deref()),
        Some("taxStatus")
    );

    let (status, next_number, voucher_count, idempotency_count, pdf_job_count): (
        String,
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        r#"
        SELECT
          (SELECT status FROM invoices WHERE id = ?1),
          (SELECT next_number FROM invoice_sequences WHERE workspace_id = ?2 AND fiscal_year_id = ?3),
          (SELECT COUNT(*) FROM vouchers WHERE workspace_id = ?2),
          (SELECT COUNT(*) FROM local_jobs WHERE workspace_id = ?2 AND job_type = 'invoice_issue'),
          (SELECT COUNT(*) FROM local_jobs WHERE workspace_id = ?2 AND job_type = 'invoice_pdf_generate')
        "#,
    )
    .bind(&draft.id)
    .bind(&workspace_id)
    .bind(format!("fy-{workspace_id}-2026"))
    .fetch_one(&pool)
    .await
    .expect("unchanged invoice state");

    assert_eq!(
        (status.as_str(), next_number, voucher_count, idempotency_count, pdf_job_count),
        ("draft", 1, 0, 0, 0)
    );
}

#[tokio::test]
async fn issue_rechecks_tax_status_after_the_draft_was_created() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;
    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Tax status changes before issue".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

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
    .expect("downgrade tax profile");

    let error = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "tax-status-downgrade-before-issue".to_string(),
            issue_date: Some("2026-01-20".to_string()),
        },
    )
    .await
    .expect_err("a downgraded tax profile must block issuance");

    assert_eq!(error.code, "validation_error");
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.first())
            .and_then(|detail| detail.field.as_deref()),
        Some("taxStatus")
    );
    let status: String = sqlx::query_scalar("SELECT status FROM invoices WHERE id = ?1")
        .bind(&draft.id)
        .fetch_one(&pool)
        .await
        .expect("draft remains unissued");
    assert_eq!(status, "draft");
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

    let early_credit_key = "credit-before-issued-invoice";
    let error = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id.clone(),
            idempotency_key: early_credit_key.to_string(),
            reason: Some("Correction".to_string()),
            issue_date: Some("2025-12-31".to_string()),
        },
    )
    .await
    .expect_err("credit before the issued invoice must be rejected");

    assert_eq!(error.code, "validation_error");
    assert_eq!(
        error.details.as_ref().and_then(|details| details[0].field.as_deref()),
        Some("issueDate")
    );

    let (credit_invoice_count, credit_note_count, reversal_voucher_count, idempotency_count, next_number): (i64, i64, i64, i64, i64) =
        sqlx::query_as(
            r#"
            SELECT
              (SELECT COUNT(*) FROM invoices WHERE workspace_id = ?1 AND source_invoice_id = ?2),
              (SELECT COUNT(*) FROM credit_notes WHERE workspace_id = ?1 AND source_invoice_id = ?2),
              (SELECT COUNT(*) FROM vouchers v JOIN invoices i ON i.id = v.source_id WHERE v.workspace_id = ?1 AND i.source_invoice_id = ?2),
              (SELECT COUNT(*) FROM local_jobs WHERE workspace_id = ?1 AND job_type = 'invoice_credit' AND idempotency_key = ?3),
              (SELECT next_number FROM invoice_sequences WHERE workspace_id = ?1 AND fiscal_year_id = ?4)
            "#,
        )
        .bind(&workspace_id)
        .bind(&issued.id)
        .bind(early_credit_key)
        .bind(format!("fy-{workspace_id}-2026"))
        .fetch_one(&pool)
        .await
        .expect("persistence counts");

    assert_eq!(
        (
            credit_invoice_count,
            credit_note_count,
            reversal_voucher_count,
            idempotency_count,
            next_number
        ),
        (0, 0, 0, 0, 2)
    );

    let first = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id.clone(),
            idempotency_key: "credit-replay-key".to_string(),
            reason: Some("Correction".to_string()),
            issue_date: Some("2026-02-10".to_string()),
        },
    )
    .await
    .expect("credit");

    let changed_date = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id.clone(),
            idempotency_key: "credit-replay-key".to_string(),
            reason: Some("Correction".to_string()),
            issue_date: Some("2026-02-11".to_string()),
        },
    )
    .await
    .expect_err("a replay with a different credit date must be rejected");
    assert_eq!(
        changed_date
            .details
            .as_ref()
            .and_then(|details| details[0].field.as_deref()),
        Some("issueDate")
    );

    let changed_reason = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id.clone(),
            idempotency_key: "credit-replay-key".to_string(),
            reason: Some("Different correction".to_string()),
            issue_date: Some("2026-02-10".to_string()),
        },
    )
    .await
    .expect_err("a replay with a different correction reason must be rejected");
    assert_eq!(
        changed_reason
            .details
            .as_ref()
            .and_then(|details| details[0].field.as_deref()),
        Some("reason")
    );

    let replay = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: issued.id,
            idempotency_key: "credit-replay-key".to_string(),
            reason: Some("Correction".to_string()),
            issue_date: Some("2026-02-10".to_string()),
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
            issue_date: Some("2026-02-10".to_string()),
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
            issue_date: Some("2026-02-10".to_string()),
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

    sqlx::query(
        r#"
        INSERT INTO rule_versions (
          id, tax_year, effective_from, source_url, checksum, status
        ) VALUES ('rv-2027-test', 2027, '2027-01-01', 'https://example.test/rules/2027', 'test-checksum-2027', 'active')
        "#,
    )
    .execute(&pool)
    .await
    .expect("2027 rule version");

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
async fn issue_requires_active_rule_provenance_for_the_issue_year() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;
    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Unprovenanced year".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

    let error = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "issue-without-2027-rule".to_string(),
            issue_date: Some("2027-01-15".to_string()),
        },
    )
    .await
    .expect_err("unprovenanced issue year must be rejected");
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.first())
            .and_then(|detail| detail.field.as_deref()),
        Some("ruleVersion")
    );

    let (status, fiscal_year_count, snapshot_count): (String, i64, i64) = sqlx::query_as(
        r#"
        SELECT
          (SELECT status FROM invoices WHERE id = ?1),
          (SELECT COUNT(*) FROM fiscal_years WHERE id = ?2),
          (SELECT COUNT(*) FROM invoice_issue_snapshots WHERE invoice_id = ?1)
        "#,
    )
    .bind(&draft.id)
    .bind(format!("fy-{workspace_id}-2027"))
    .fetch_one(&pool)
    .await
    .expect("unprovenanced issue state");
    assert_eq!((status.as_str(), fiscal_year_count, snapshot_count), ("draft", 0, 0));
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
async fn exempt_threshold_breach_preflight_blocks_issue_and_records_provenance() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
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
    .expect("exempt VAT profile");
    let customer = create_customer(&pool, &workspace_id).await;

    let at_threshold = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id.clone(),
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Threshold sale".to_string(),
                quantity: 1,
                unit_price_minor: 12_000_000,
                vat_rate: 0.0,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft at threshold");
    invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: at_threshold.id,
            idempotency_key: "issue-at-threshold".to_string(),
            issue_date: Some("2026-01-10".to_string()),
        },
    )
    .await
    .expect("equality remains governed by the seeded rule outcome");

    let breach = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Breach sale".to_string(),
                quantity: 1,
                unit_price_minor: 1,
                vat_rate: 0.0,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("breach draft");

    let preflight = invoicing::invoice_issue_preflight(
        &pool,
        &workspace_id,
        &InvoiceIssuePreflightInput {
            invoice_id: breach.id.clone(),
            issue_date: Some("2026-02-10".to_string()),
        },
    )
    .await
    .expect("preflight");

    assert!(preflight.requires_vat_treatment_review);
    assert_eq!(preflight.projected_turnover_minor, 12_000_001);
    assert_eq!(preflight.rule_version_id.as_deref(), Some("rv-2026-active"));
    assert_eq!(preflight.tax_year, 2026);
    assert!(preflight.source_url.as_deref().is_some_and(|source_url| !source_url.is_empty()));

    let error = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: breach.id,
            idempotency_key: "issue-threshold-breach".to_string(),
            issue_date: Some("2026-02-10".to_string()),
        },
    )
    .await
    .expect_err("zero-VAT exempt invoice must be blocked after threshold breach");

    assert_eq!(error.code, "validation_error");
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.first())
            .and_then(|detail| detail.field.as_deref()),
        Some("vatStatus")
    );
    let block_event_metadata: String = sqlx::query_scalar(
        "SELECT metadata_json FROM audit_events WHERE workspace_id = ?1 AND action = 'invoice_issue_blocked_vat_threshold' LIMIT 1",
    )
    .bind(&workspace_id)
    .fetch_one(&pool)
    .await
    .expect("block audit event");
    assert!(block_event_metadata.contains("\"ruleVersionId\":\"rv-2026-active\""));
    assert!(block_event_metadata.contains("\"sourceUrl\":\"https://"));
}

#[tokio::test]
async fn credit_after_locked_original_period_posts_in_selected_open_year() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;
    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "2026 service".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");
    let source = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "issue-before-lock".to_string(),
            issue_date: Some("2026-12-20".to_string()),
        },
    )
    .await
    .expect("issue");
    sqlx::query(
        r#"
        INSERT INTO fiscal_periods (
          id, workspace_id, fiscal_year_id, period_key, starts_on, ends_on, status
        ) VALUES (?1, ?2, ?3, '2026-Q4', '2026-10-01', '2026-12-31', 'locked')
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&workspace_id)
    .bind(format!("fy-{workspace_id}-2026"))
    .execute(&pool)
    .await
    .expect("lock source fiscal period");

    let credit = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: source.id.clone(),
            idempotency_key: "credit-after-lock".to_string(),
            reason: Some("Customer correction".to_string()),
            issue_date: Some("2027-01-10".to_string()),
        },
    )
    .await
    .expect("credit in selected open year");

    assert_eq!(credit.issue_date.as_deref(), Some("2027-01-10"));
    assert_eq!(credit.source_invoice_id.as_deref(), Some(source.id.as_str()));
    let (fiscal_year_id, accounting_date): (String, String) = sqlx::query_as(
        r#"
        SELECT i.fiscal_year_id, v.accounting_date
        FROM invoices i
        JOIN vouchers v ON v.id = i.voucher_id
        WHERE i.id = ?1
        "#,
    )
    .bind(&credit.id)
    .fetch_one(&pool)
    .await
    .expect("credit posting");
    assert_eq!(fiscal_year_id, format!("fy-{workspace_id}-2027"));
    assert_eq!(accounting_date, "2027-01-10");
}

#[tokio::test]
async fn credit_preserves_historical_vat_after_profile_becomes_exempt() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;
    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "VAT-charged service".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");
    let source = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "issue-before-vat-exemption".to_string(),
            issue_date: Some("2026-01-20".to_string()),
        },
    )
    .await
    .expect("VAT invoice");

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
    .expect("exempt VAT profile");

    let credit = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: source.id.clone(),
            idempotency_key: "credit-historical-vat-invoice".to_string(),
            reason: Some("Customer correction".to_string()),
            issue_date: Some("2026-01-21".to_string()),
        },
    )
    .await
    .expect("historical VAT treatment remains creditable");

    assert_eq!(credit.source_invoice_id.as_deref(), Some(source.id.as_str()));
    assert_eq!(credit.total_ex_vat_minor, source.total_ex_vat_minor);
    assert_eq!(credit.total_vat_minor, source.total_vat_minor);
    assert_eq!(credit.total_inc_vat_minor, source.total_inc_vat_minor);
    assert_eq!(credit.lines.len(), source.lines.len());
    assert_eq!(credit.lines[0].vat_rate_bp, source.lines[0].vat_rate_bp);
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

#[tokio::test]
async fn issue_rejects_a_year_closed_after_fiscal_year_resolution_without_period_rows() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;
    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Closed year issue".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

    let fiscal_year_id = format!("fy-{workspace_id}-2026");
    let period_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_periods WHERE fiscal_year_id = ?1",
    )
    .bind(&fiscal_year_id)
    .fetch_one(&pool)
    .await
    .expect("period count");
    assert_eq!(period_count, 0);
    sqlx::query("UPDATE fiscal_years SET status = 'closed' WHERE id = ?1")
        .bind(&fiscal_year_id)
        .execute(&pool)
        .await
        .expect("close fiscal year");

    let error = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id.clone(),
            idempotency_key: "issue-closed-year".to_string(),
            issue_date: Some("2026-05-01".to_string()),
        },
    )
    .await
    .expect_err("closed fiscal year must reject issue");
    assert_eq!(error.code, "locked_period");

    let (status, next_number, voucher_count, snapshot_count): (String, i64, i64, i64) =
        sqlx::query_as(
            r#"
            SELECT
              (SELECT status FROM invoices WHERE id = ?1),
              (SELECT next_number FROM invoice_sequences WHERE fiscal_year_id = ?2),
              (SELECT COUNT(*) FROM vouchers WHERE workspace_id = ?3),
              (SELECT COUNT(*) FROM invoice_issue_snapshots WHERE invoice_id = ?1)
            "#,
        )
        .bind(&draft.id)
        .bind(&fiscal_year_id)
        .bind(&workspace_id)
        .fetch_one(&pool)
        .await
        .expect("issue state remains unchanged");
    assert_eq!(
        (status.as_str(), next_number, voucher_count, snapshot_count),
        ("draft", 1, 0, 0)
    );
}

#[tokio::test]
async fn credit_rejects_a_selected_year_closed_after_fiscal_year_resolution_without_period_rows() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    let customer = create_customer(&pool, &workspace_id).await;
    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Closed year credit".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");
    let source = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "issue-before-closed-credit".to_string(),
            issue_date: Some("2026-06-01".to_string()),
        },
    )
    .await
    .expect("issue source");

    let fiscal_year_id = format!("fy-{workspace_id}-2027");
    sqlx::query(
        r#"
        INSERT INTO fiscal_years (id, workspace_id, starts_on, ends_on, status)
        VALUES (?1, ?2, '2027-01-01', '2027-12-31', 'closed')
        "#,
    )
    .bind(&fiscal_year_id)
    .bind(&workspace_id)
    .execute(&pool)
    .await
    .expect("close selected fiscal year");
    sqlx::query(
        r#"
        INSERT INTO invoice_sequences (id, workspace_id, fiscal_year_id, prefix, next_number)
        VALUES (?1, ?2, ?3, '2027-', 1)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&workspace_id)
    .bind(&fiscal_year_id)
    .execute(&pool)
    .await
    .expect("sequence");
    let period_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_periods WHERE fiscal_year_id = ?1",
    )
    .bind(&fiscal_year_id)
    .fetch_one(&pool)
    .await
    .expect("period count");
    assert_eq!(period_count, 0);

    let error = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &InvoiceCreditInput {
            source_invoice_id: source.id.clone(),
            idempotency_key: "credit-closed-year".to_string(),
            reason: Some("Correction".to_string()),
            issue_date: Some("2027-01-10".to_string()),
        },
    )
    .await
    .expect_err("closed selected fiscal year must reject credit");
    assert_eq!(error.code, "locked_period");

    let (source_status, credit_count, next_number): (String, i64, i64) = sqlx::query_as(
        r#"
        SELECT
          (SELECT status FROM invoices WHERE id = ?1),
          (SELECT COUNT(*) FROM invoices WHERE source_invoice_id = ?1),
          (SELECT next_number FROM invoice_sequences WHERE fiscal_year_id = ?2)
        "#,
    )
    .bind(&source.id)
    .bind(&fiscal_year_id)
    .fetch_one(&pool)
    .await
    .expect("credit state remains unchanged");
    assert_eq!((source_status.as_str(), credit_count, next_number), ("issued", 0, 1));
}
