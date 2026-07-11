use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    documents::{self, DocumentImportInput, DocumentListInput},
    expenses::{self, ExpensePostInput},
    imports::{self, CsvImportCreateInput, StagedTransactionsListInput},
    invoicing::{self, InvoiceCreateDraftInput, InvoiceIssueInput, InvoiceLineInput},
    ledger::{self, VoucherGetInput, VoucherListInput},
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    vat::{self, VatReturnApproveInput, VatReturnDraftCreateInput},
    year_end::{self, YearEndPackageCreateInput, YearEndReadinessInput},
    workspace::ensure_workspace_ready,
};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

async fn bootstrap_workspace(
    name: &str,
) -> (tempfile::TempDir, sqlx::SqlitePool, String, std::path::PathBuf) {
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
    .bind(name)
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

    (dir, pool, workspace_id, data_dir)
}

#[tokio::test]
async fn m7_voucher_list_and_get_after_invoice_issue() {
    let (_dir, pool, workspace_id, _data_dir) = bootstrap_workspace("M7 voucher read").await;

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Read Test Customer".to_string(),
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
            invoice_id: draft.id,
            idempotency_key: "issue-read-1".to_string(),
            issue_date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect("issued");

    let vouchers = ledger::voucher_list(
        &pool,
        &workspace_id,
        &VoucherListInput {
            status: Some("posted".to_string()),
            source_type: None,
            limit: None,
            before_id: None,
        },
    )
    .await
    .expect("voucher list");

    assert!(!vouchers.is_empty());
    let voucher_id = issued.voucher_id.expect("voucher id");
    let summary = vouchers
        .iter()
        .find(|row| row.id == voucher_id)
        .expect("issued voucher in list");
    assert_eq!(summary.source_type, "invoice");
    assert!(summary.debit_total_minor > 0);
    assert_eq!(summary.debit_total_minor, summary.credit_total_minor);

    let detail = ledger::voucher_get(
        &pool,
        &workspace_id,
        &VoucherGetInput {
            voucher_id: voucher_id.clone(),
        },
    )
    .await
    .expect("voucher get");

    assert_eq!(detail.id, voucher_id);
    assert!(!detail.lines.is_empty());
}

#[tokio::test]
async fn m7_account_list_returns_balances() {
    let (_dir, pool, workspace_id, _data_dir) = bootstrap_workspace("M7 account list").await;

    let accounts = ledger::account_list(&pool, &workspace_id)
        .await
        .expect("accounts");

    assert!(!accounts.is_empty());
    assert!(accounts.iter().any(|row| row.number == "1510"));
}

#[tokio::test]
async fn m7_staged_transactions_list_filters_status() {
    let (_dir, pool, workspace_id, data_dir) = bootstrap_workspace("M7 staged list").await;

    let csv_path = data_dir.join("bank.csv");
    fs::write(&csv_path, "date,description,amount_minor\n2026-02-15,Payment,1250000\n")
        .expect("csv");

    imports::csv_import_create(
        &pool,
        &workspace_id,
        &CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-read-1".to_string(),
        },
    )
    .await
    .expect("csv import");

    let staged = imports::staged_transactions_list(
        &pool,
        &workspace_id,
        &StagedTransactionsListInput {
            status: Some("staged".to_string()),
            limit: None,
            before_id: None,
        },
    )
    .await
    .expect("staged list");

    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].amount_minor, 1_250_000);
}

#[tokio::test]
async fn m7_document_list_unattached_only() {
    let (_dir, pool, workspace_id, data_dir) = bootstrap_workspace("M7 document list").await;

    let source_path = data_dir.join("receipt.png");
    fs::write(&source_path, b"receipt").expect("receipt");

    documents::document_import(
        &pool,
        &workspace_id,
        &DocumentImportInput {
            source_path: source_path.to_string_lossy().to_string(),
            filename: "receipt.png".to_string(),
            mime_type: "image/png".to_string(),
            idempotency_key: "doc-list-1".to_string(),
        },
    )
    .await
    .expect("import");

    let unattached = documents::document_list(
        &pool,
        &workspace_id,
        &DocumentListInput {
            unattached_only: Some(true),
            limit: None,
            before_id: None,
        },
    )
    .await
    .expect("documents");

    assert_eq!(unattached.len(), 1);

    expenses::expense_post(
        &pool,
        &workspace_id,
        &ExpensePostInput {
            amount_minor_ex_vat: 1_000_00,
            vat_rate: 0.25,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: Some(unattached[0].id.clone()),
            no_document_reason: None,
            staged_transaction_id: None,
            idempotency_key: "expense-doc-attach".to_string(),
            date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect("expense");

    let unattached_after = documents::document_list(
        &pool,
        &workspace_id,
        &DocumentListInput {
            unattached_only: Some(true),
            limit: None,
            before_id: None,
        },
    )
    .await
    .expect("documents after attach");

    assert!(unattached_after.is_empty());
}

#[tokio::test]
async fn m7_fiscal_period_list_includes_locked_after_vat_approve() {
    let (_dir, pool, workspace_id, _data_dir) = bootstrap_workspace("M7 fiscal periods").await;

    let draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "vat-draft-read".to_string(),
        },
    )
    .await
    .expect("vat draft");

    vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: draft.id,
            idempotency_key: "vat-approve-read".to_string(),
        },
    )
    .await
    .expect("vat approve");

    let periods = vat::fiscal_period_list(&pool, &workspace_id)
        .await
        .expect("periods");

    let q1 = periods
        .iter()
        .find(|row| row.period_key == "2026-Q1")
        .expect("q1 period");
    assert_eq!(q1.status, "locked");
}

#[tokio::test]
async fn m7_read_commands_are_workspace_scoped() {
    let (_dir_a, pool_a, workspace_a, _) = bootstrap_workspace("M7 scope A").await;
    let (_dir_b, pool_b, workspace_b, data_dir_b) = bootstrap_workspace("M7 scope B").await;

    let csv_path = data_dir_b.join("bank.csv");
    fs::write(&csv_path, "date,description,amount_minor\n2026-02-15,Payment,10000\n").expect("csv");

    imports::csv_import_create(
        &pool_b,
        &workspace_b,
        &CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-scope-b".to_string(),
        },
    )
    .await
    .expect("csv b");

    let staged_a = imports::staged_transactions_list(
        &pool_a,
        &workspace_a,
        &StagedTransactionsListInput {
            status: Some("staged".to_string()),
            limit: None,
            before_id: None,
        },
    )
    .await
    .expect("staged a");

    assert!(staged_a.is_empty());
}

#[tokio::test]
async fn m7_expense_post_marks_staged_transaction_matched() {
    let (_dir, pool, workspace_id, data_dir) =
        bootstrap_workspace("M7 expense staged").await;

    let csv_path = data_dir.join("bank.csv");
    fs::write(
        &csv_path,
        "date,description,amount_minor\n2026-02-15,Office supplies,-62500\n",
    )
    .expect("csv");

    let import = imports::csv_import_create(
        &pool,
        &workspace_id,
        &CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-expense-staged".to_string(),
        },
    )
    .await
    .expect("csv import");

    let staged = imports::staged_transactions_list(
        &pool,
        &workspace_id,
        &StagedTransactionsListInput {
            status: Some("staged".to_string()),
            limit: None,
            before_id: None,
        },
    )
    .await
    .expect("staged");

    assert_eq!(staged.len(), 1);

    expenses::expense_post(
        &pool,
        &workspace_id,
        &ExpensePostInput {
            amount_minor_ex_vat: 50_000,
            vat_rate: 0.25,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: None,
            no_document_reason: Some("Small receipt lost".to_string()),
            staged_transaction_id: Some(staged[0].id.clone()),
            idempotency_key: "expense-staged-1".to_string(),
            date: Some("2026-02-15".to_string()),
        },
    )
    .await
    .expect("expense");

    let staged_after = imports::staged_transactions_list(
        &pool,
        &workspace_id,
        &StagedTransactionsListInput {
            status: Some("staged".to_string()),
            limit: None,
            before_id: None,
        },
    )
    .await
    .expect("staged after");

    assert!(staged_after.is_empty());

    let matched = imports::staged_transactions_list(
        &pool,
        &workspace_id,
        &StagedTransactionsListInput {
            status: Some("matched".to_string()),
            limit: None,
            before_id: None,
        },
    )
    .await
    .expect("matched");

    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].id, import.first_staged_transaction_id);
}

#[tokio::test]
async fn m7_expense_post_rejects_staged_amount_mismatch() {
    let (_dir, pool, workspace_id, data_dir) =
        bootstrap_workspace("M7 expense staged mismatch").await;

    let csv_path = data_dir.join("bank.csv");
    fs::write(
        &csv_path,
        "date,description,amount_minor\n2026-02-15,Office supplies,-50000\n",
    )
    .expect("csv");

    let import = imports::csv_import_create(
        &pool,
        &workspace_id,
        &CsvImportCreateInput {
            source_path: csv_path.to_string_lossy().to_string(),
            idempotency_key: "csv-expense-mismatch".to_string(),
        },
    )
    .await
    .expect("csv import");

    let err = expenses::expense_post(
        &pool,
        &workspace_id,
        &ExpensePostInput {
            amount_minor_ex_vat: 50_000,
            vat_rate: 0.25,
            expense_account_number: "5610".to_string(),
            payment_account_number: "1930".to_string(),
            document_id: None,
            no_document_reason: Some("Small receipt lost".to_string()),
            staged_transaction_id: Some(import.first_staged_transaction_id),
            idempotency_key: "expense-staged-mismatch".to_string(),
            date: Some("2026-02-15".to_string()),
        },
    )
    .await
    .expect_err("expense should reject staged amount mismatch");

    assert_eq!(err.code, "validation_error");
    assert!(err.message.contains("must match expense payment total"));
}

#[tokio::test]
async fn m7_year_end_readiness_reports_unfiled_vat_periods() {
    let (_dir, pool, workspace_id, _data_dir) =
        bootstrap_workspace("M7 year-end readiness").await;

    let readiness = year_end::year_end_readiness_get(
        &pool,
        &workspace_id,
        &YearEndReadinessInput { fiscal_year: 2026 },
    )
    .await
    .expect("readiness");

    assert!(!readiness.ready_to_approve);
    assert!(
        readiness
            .items
            .iter()
            .any(|item| item.code.starts_with("vat_period_") && !item.satisfied)
    );
}

async fn approve_all_vat_quarters(pool: &sqlx::SqlitePool, workspace_id: &str) {
    for quarter in ["2026-Q1", "2026-Q2", "2026-Q3", "2026-Q4"] {
        let draft = vat::vat_return_draft_create(
            pool,
            workspace_id,
            &VatReturnDraftCreateInput {
                period_key: quarter.to_string(),
                idempotency_key: format!("m7-vat-{quarter}"),
            },
        )
        .await
        .expect("vat draft");
        vat::vat_return_approve(
            pool,
            workspace_id,
            &VatReturnApproveInput {
                vat_return_id: draft.id,
                idempotency_key: format!("m7-vat-approve-{quarter}"),
            },
        )
        .await
        .expect("vat approve");
    }
}

#[tokio::test]
async fn m7_voucher_count_filters_posted_status() {
    let (_dir, pool, workspace_id, _data_dir) = bootstrap_workspace("M7 voucher count").await;

    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Count customer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("counterparty");

    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Service".to_string(),
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
            invoice_id: draft.id,
            idempotency_key: "count-issue".to_string(),
            issue_date: Some("2026-03-01".to_string()),
        },
    )
    .await
    .expect("issued");

    let posted_count = ledger::voucher_count(
        &pool,
        &workspace_id,
        &ledger::VoucherCountInput {
            status: Some("posted".to_string()),
        },
    )
    .await
    .expect("posted count");

    assert!(posted_count >= 1);
}

#[tokio::test]
async fn m7_year_end_readiness_get_is_read_only() {
    let (_dir, pool, workspace_id, data_dir) =
        bootstrap_workspace("M7 readiness read-only").await;

    approve_all_vat_quarters(&pool, &workspace_id).await;

    let package = year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "m7-readiness-package".to_string(),
        },
    )
    .await
    .expect("year-end package");

    let export_path = package.export_path.clone().expect("export path");
    let export_file = data_dir.join("exports").join(&export_path);
    fs::remove_file(&export_file).expect("delete export file");

    let export_path_before: Option<String> = sqlx::query_scalar(
        "SELECT export_path FROM year_end_packages WHERE id = ?1",
    )
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("export path before");

    year_end::year_end_readiness_get(
        &pool,
        &workspace_id,
        &YearEndReadinessInput { fiscal_year: 2026 },
    )
    .await
    .expect("readiness");

    let export_path_after: Option<String> = sqlx::query_scalar(
        "SELECT export_path FROM year_end_packages WHERE id = ?1",
    )
    .bind(&package.id)
    .fetch_one(&pool)
    .await
    .expect("export path after");

    assert_eq!(export_path_before, export_path_after);
    assert!(!export_file.is_file());
}

#[tokio::test]
async fn m7_year_end_readiness_ready_when_vat_filed_and_draft_package() {
    let (_dir, pool, workspace_id, _data_dir) =
        bootstrap_workspace("M7 readiness ready draft").await;

    approve_all_vat_quarters(&pool, &workspace_id).await;

    year_end::year_end_package_create(
        &pool,
        &workspace_id,
        &YearEndPackageCreateInput {
            fiscal_year: 2026,
            idempotency_key: "m7-readiness-ready".to_string(),
        },
    )
    .await
    .expect("year-end package");

    let readiness = year_end::year_end_readiness_get(
        &pool,
        &workspace_id,
        &YearEndReadinessInput { fiscal_year: 2026 },
    )
    .await
    .expect("readiness");

    assert!(readiness.ready_to_approve);
    assert!(
        readiness
            .items
            .iter()
            .any(|item| item.code == "year_end_package_draft" && item.satisfied)
    );
}
