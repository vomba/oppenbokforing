use oppenbokforing_desktop_lib::{
    cashflow,
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    expenses::{self, ExpensePostInput},
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceIssueInput, InvoiceLineInput,
    },
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    state::load_golden_scenario,
    vat::{
        self, VatReturnApproveInput, VatReturnDraftCreateInput, VatReturnExportInput,
    },
    workspace::ensure_workspace_ready,
};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_workspace(dir: &tempfile::TempDir) -> (sqlx::SqlitePool, String, std::path::PathBuf) {
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
    .bind("M4 fixture workspace")
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
    .expect("vat profile");

    (pool, workspace_id, data_dir)
}

#[tokio::test]
async fn m4_vat_registered_zero_period_fixture() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    let scenario = load_golden_scenario("vat-registered-zero-period");
    let expected = scenario.expected.as_object().expect("expected");

    let draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "vat-zero-q1".to_string(),
        },
    )
    .await
    .expect("draft");

    assert_eq!(expected["vatReturnRequired"].as_bool(), Some(true));
    assert_eq!(expected["zeroReturn"].as_bool(), Some(draft.zero_return));
    assert_eq!(expected["box49AmountMinor"].as_i64(), Some(draft.box49_amount_minor));
    assert_eq!(draft.box49_amount_minor, 0);

    let approved = vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: draft.id.clone(),
            idempotency_key: "approve-zero-q1".to_string(),
        },
    )
    .await
    .expect("approve");

    assert_eq!(approved.status, "approved");
    assert_eq!(expected["canApproveVatReturn"].as_bool(), Some(true));
}

#[tokio::test]
async fn m4_no_activity_zero_vat_return_fixture() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("yearly vat");

    let scenario = load_golden_scenario("no-activity-zero-vat-return");
    let expected = scenario.expected.as_object().expect("expected");

    let fiscal_year_id = format!("fy-{workspace_id}-2026");
    let has_activity = vat::has_business_activity(&pool, &workspace_id, &fiscal_year_id)
        .await
        .expect("activity");
    assert_eq!(expected["businessIncomeOrExpenses"].as_bool(), Some(has_activity));

    let draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026".to_string(),
            idempotency_key: "vat-zero-year".to_string(),
        },
    )
    .await
    .expect("yearly draft");

    assert_eq!(expected["vatReturnRequired"].as_bool(), Some(true));
    assert_eq!(
        expected["vatReturnBox49Minor"].as_i64(),
        Some(draft.box49_amount_minor)
    );
    assert_eq!(draft.box49_amount_minor, 0);
    assert_eq!(expected["neReviewCanSuggestRemoval"].as_bool(), Some(!has_activity));
    assert_eq!(
        expected["deregisterGuidanceShownIfNoFutureActivity"].as_bool(),
        Some(!has_activity)
    );
}

#[tokio::test]
async fn m4_fiscal_period_lock_after_vat_fixture() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, data_dir) = setup_workspace(&dir).await;

    let scenario = load_golden_scenario("fiscal-period-lock-after-vat");
    let expected = scenario.expected.as_object().expect("expected");

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Lock Test AB".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");

    let draft_invoice = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: Some("2026-03-31".to_string()),
            lines: vec![InvoiceLineInput {
                description: "Pre-lock sale".to_string(),
                quantity: 1,
                unit_price_minor: 10_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft");

    let issued_invoice = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft_invoice.id.clone(),
            idempotency_key: "issue-pre-lock".to_string(),
            issue_date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect("issue before lock");

    let vat_draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "lock-q1-draft".to_string(),
        },
    )
    .await
    .expect("vat draft");

    vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: vat_draft.id,
            idempotency_key: "lock-q1-approve".to_string(),
        },
    )
    .await
    .expect("approve");

    let locked = vat::period_is_locked(&pool, &workspace_id, "2026-Q1")
        .await
        .expect("locked check");
    assert_eq!(expected["periodLocked"].as_bool(), Some(locked));

    let receipt_path = data_dir.join("receipt.pdf");
    fs::write(&receipt_path, b"pdf").expect("receipt");

    let doc = oppenbokforing_desktop_lib::documents::document_import(
        &pool,
        &workspace_id,
        &oppenbokforing_desktop_lib::documents::DocumentImportInput {
            source_path: receipt_path.to_string_lossy().to_string(),
            filename: "receipt.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            idempotency_key: "doc-lock-test".to_string(),
        },
    )
    .await
    .expect("doc");

    let post_err = expenses::expense_post(
        &pool,
        &workspace_id,
        &ExpensePostInput {
            amount_minor_ex_vat: 1000,
            vat_rate: 0.25,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: Some(doc.id),
            no_document_reason: None,
            staged_transaction_id: None,
            idempotency_key: "expense-in-locked".to_string(),
            date: Some("2026-02-15".to_string()),
        },
    )
    .await
    .expect_err("posting in locked period should fail");

    assert_eq!(post_err.code, "locked_period");
    assert_eq!(expected["newPostingsRejected"].as_bool(), Some(true));
    assert_eq!(expected["reversalStillAllowed"].as_bool(), Some(false));

    let credit_err = invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &invoicing::InvoiceCreditInput {
            source_invoice_id: issued_invoice.id,
            idempotency_key: "credit-in-locked".to_string(),
            reason: None,
        },
    )
    .await
    .expect_err("credit in locked period should fail");

    assert_eq!(credit_err.code, "locked_period");
}

#[tokio::test]
async fn m4_vat_return_export_offline() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, data_dir) = setup_workspace(&dir).await;

    let draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "export-q1".to_string(),
        },
    )
    .await
    .expect("draft");

    let approved = vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: draft.id.clone(),
            idempotency_key: "export-approve".to_string(),
        },
    )
    .await
    .expect("approve");

    let exported = vat::vat_return_export(
        &pool,
        &workspace_id,
        &VatReturnExportInput {
            vat_return_id: approved.id,
            export_directory: None,
        },
    )
    .await
    .expect("export");

    let export_rel = exported.export_path.expect("export path");
    let export_abs = data_dir.join("exports").join(&export_rel);
    assert!(export_abs.exists(), "export file should exist offline");
}

#[tokio::test]
async fn m4_cashflow_overview_exempt_profile() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "exempt_low_turnover".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("exempt vat profile");

    let overview = cashflow::cashflow_overview_get(&pool, &workspace_id, 2026)
        .await
        .expect("overview");

    let expected_period =
        vat::current_reporting_period_key("quarterly", chrono::Utc::now().date_naive());

    assert_eq!(overview.tax_reserve_minor, 0);
    assert_eq!(overview.vat_reserve_minor, 0);
    assert_eq!(overview.receivables_balance_minor, 0);
    assert_eq!(overview.spendable_cash_minor, 0);
    assert_eq!(
        overview.vat_period_key.as_deref(),
        Some(expected_period.as_str()),
        "exempt profile gets turnover monitoring period key"
    );
}

#[tokio::test]
async fn m4_cashflow_tax_reserve_from_ledger_profit() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "exempt_low_turnover".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("exempt vat profile");

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Cashflow Customer".to_string(),
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
                description: "Sale".to_string(),
                quantity: 1,
                unit_price_minor: 1_000_000,
                vat_rate: 0.0,
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
            invoice_id: draft.id,
            idempotency_key: "cashflow-tax".to_string(),
            issue_date: Some("2026-06-01".to_string()),
        },
    )
    .await
    .expect("issue");

    let overview = cashflow::cashflow_overview_get(&pool, &workspace_id, 2026)
        .await
        .expect("overview");

    assert_eq!(overview.tax_reserve_minor, 300_000);
    assert_eq!(overview.receivables_balance_minor, 1_000_000);
    assert_eq!(overview.spendable_cash_minor, 700_000);
}

#[tokio::test]
async fn m4_cashflow_tax_reserve_includes_fa_skatt_salary() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "fa_skatt".to_string(),
            expected_business_profit_minor: Some(500_000),
            expected_salary_income_minor: Some(2_000_000),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("fa-skatt profile");

    let overview = cashflow::cashflow_overview_get(&pool, &workspace_id, 2026)
        .await
        .expect("overview");

    assert_eq!(overview.tax_reserve_minor, 600_000);
}

#[tokio::test]
async fn m4_cashflow_overview_after_vat_approval() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    let draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "cashflow-post-approve".to_string(),
        },
    )
    .await
    .expect("draft");

    vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: draft.id,
            idempotency_key: "cashflow-approve".to_string(),
        },
    )
    .await
    .expect("approve");

    let overview = cashflow::cashflow_overview_get(&pool, &workspace_id, 2026)
        .await
        .expect("overview after lock should not draft");
    assert!(overview.vat_reserve_minor >= 0);
}

#[tokio::test]
async fn m4_vat_boxes_net_credit_reversal() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "VAT Net AB".to_string(),
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
            due_date: Some("2026-03-31".to_string()),
            lines: vec![InvoiceLineInput {
                description: "VAT sale".to_string(),
                quantity: 1,
                unit_price_minor: 10_000,
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
            idempotency_key: "vat-net-issue".to_string(),
            issue_date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect("issue");

    invoicing::credit_invoice(
        &pool,
        &workspace_id,
        &invoicing::InvoiceCreditInput {
            source_invoice_id: issued.id,
            idempotency_key: "vat-net-credit".to_string(),
            reason: None,
        },
    )
    .await
    .expect("credit");

    let vat_draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "vat-net-q1".to_string(),
        },
    )
    .await
    .expect("vat draft");

    assert_eq!(vat_draft.box49_amount_minor, 0);
    assert!(vat_draft.zero_return);
}

#[tokio::test]
async fn m4_vat_threshold_monitoring() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "exempt_low_turnover".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("exempt vat");

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Threshold Customer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");

    for (idx, amount) in [(0, 11_000_000_i64), (1, 1_500_000_i64)].iter() {
        let draft = invoicing::create_draft(
            &pool,
            &workspace_id,
            &InvoiceCreateDraftInput {
                counterparty_id: customer.id.clone(),
                due_date: None,
                lines: vec![InvoiceLineInput {
                    description: format!("Sale {idx}"),
                    quantity: 1,
                    unit_price_minor: *amount,
                    vat_rate: 0.0,
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
                invoice_id: draft.id,
                idempotency_key: format!("thresh-{idx}"),
                issue_date: Some("2026-06-01".to_string()),
            },
        )
        .await
        .expect("issue");
    }

    let status = vat::vat_threshold_status(&pool, &workspace_id, 2026)
        .await
        .expect("threshold");
    assert!(status.must_register_for_vat);
    assert_eq!(status.annual_turnover_minor, 12_500_000);
}

#[tokio::test]
async fn m4_vat_12_percent_maps_to_correct_boxes() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "VAT 12 AB".to_string(),
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
            due_date: Some("2026-02-28".to_string()),
            lines: vec![InvoiceLineInput {
                description: "12% service".to_string(),
                quantity: 1,
                unit_price_minor: 10_000,
                vat_rate: 0.12,
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
            invoice_id: draft.id,
            idempotency_key: "vat12-issue".to_string(),
            issue_date: Some("2026-02-15".to_string()),
        },
    )
    .await
    .expect("issue");

    let vat_draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "vat12-q1".to_string(),
        },
    )
    .await
    .expect("vat draft");

    let box06 = vat_draft
        .boxes
        .iter()
        .find(|b| b.box_code == "06")
        .map(|b| b.amount_minor)
        .unwrap_or(0);
    let box11 = vat_draft
        .boxes
        .iter()
        .find(|b| b.box_code == "11")
        .map(|b| b.amount_minor)
        .unwrap_or(0);
    assert_eq!(box06, 10_000);
    assert_eq!(box11, 1_200);
}

#[tokio::test]
async fn m4_vat_mixed_rates_split_sales_boxes() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Mixed Rate AB".to_string(),
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
            due_date: Some("2026-03-31".to_string()),
            lines: vec![
                InvoiceLineInput {
                    description: "25% service".to_string(),
                    quantity: 1,
                    unit_price_minor: 10_000,
                    vat_rate: 0.25,
                    account_number: Some("3041".to_string()),
                },
                InvoiceLineInput {
                    description: "12% service".to_string(),
                    quantity: 1,
                    unit_price_minor: 5_000,
                    vat_rate: 0.12,
                    account_number: Some("3041".to_string()),
                },
            ],
        },
    )
    .await
    .expect("draft");

    invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: draft.id,
            idempotency_key: "mixed-rate-issue".to_string(),
            issue_date: Some("2026-03-15".to_string()),
        },
    )
    .await
    .expect("issue");

    let vat_draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "mixed-rate-q1".to_string(),
        },
    )
    .await
    .expect("vat draft");

    let box05 = vat_draft
        .boxes
        .iter()
        .find(|b| b.box_code == "05")
        .map(|b| b.amount_minor)
        .unwrap_or(0);
    let box06 = vat_draft
        .boxes
        .iter()
        .find(|b| b.box_code == "06")
        .map(|b| b.amount_minor)
        .unwrap_or(0);
    assert_eq!(box05, 10_000);
    assert_eq!(box06, 5_000);
}

#[tokio::test]
async fn m4_vat_draft_rejects_mismatched_reporting_period() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("yearly vat");

    let err = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "bad-period".to_string(),
        },
    )
    .await
    .expect_err("quarterly key on yearly profile");

    assert_eq!(err.code, "validation_error");
}

#[tokio::test]
async fn m4_vat_draft_rejects_exempt_profile() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "exempt_low_turnover".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("exempt vat");

    let err = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "exempt-draft".to_string(),
        },
    )
    .await
    .expect_err("exempt profile cannot create VAT return");

    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("registered VAT profile"));
}

#[tokio::test]
async fn m4_vat_approve_idempotency_rejects_different_return() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    let q1 = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "idem-q1".to_string(),
        },
    )
    .await
    .expect("q1");

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "yearly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("yearly");

    let yearly = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026".to_string(),
            idempotency_key: "idem-year".to_string(),
        },
    )
    .await
    .expect("year");

    vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: q1.id.clone(),
            idempotency_key: "shared-approve".to_string(),
        },
    )
    .await
    .expect("approve q1");

    let err = vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: yearly.id,
            idempotency_key: "shared-approve".to_string(),
        },
    )
    .await
    .expect_err("same key different return");

    assert_eq!(err.code, "validation_error");
}

#[tokio::test]
async fn m4_vat_approve_refreshes_boxes_after_late_posting() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id, _) = setup_workspace(&dir).await;

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Late poster".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");

    let draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "late-draft".to_string(),
        },
    )
    .await
    .expect("empty draft");
    assert_eq!(draft.box49_amount_minor, 0);

    let invoice_draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Late sale".to_string(),
                quantity: 1,
                unit_price_minor: 8_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("invoice draft");

    invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: invoice_draft.id,
            idempotency_key: "late-issue".to_string(),
            issue_date: Some("2026-03-01".to_string()),
        },
    )
    .await
    .expect("issue");

    let approved = vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: draft.id,
            idempotency_key: "late-approve".to_string(),
        },
    )
    .await
    .expect("approve");

    assert!(approved.box49_amount_minor > 0);
}
