use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    invoicing::{self, InvoiceCreateDraftInput, InvoiceIssueInput, InvoiceLineInput},
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
    state::load_golden_scenario,
    tax_tasks::{self, TaxTaskListInput},
    vat::{self, VatReturnApproveInput, VatReturnDraftCreateInput, VatReturnExportInput, VatReturnTraceInput},
    workspace::ensure_workspace_ready,
};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_workspace(
    dir: &tempfile::TempDir,
    vat_status: &str,
    reporting_period: &str,
    vat_filing_deadline_regime: Option<&str>,
) -> (sqlx::SqlitePool, String) {
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
    .bind("Tax task fixture workspace")
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
    let legacy_missing_regime =
        vat_status != "exempt_low_turnover" && vat_filing_deadline_regime.is_none();
    let regime_to_save = if vat_status == "exempt_low_turnover" {
        None
    } else {
        vat_filing_deadline_regime.or(match reporting_period {
            "monthly" => Some("monthly_12"),
            "yearly" => Some("annual_may_12"),
            _ => Some("quarterly_12"),
        })
    };
    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: vat_status.to_string(),
            reporting_period: reporting_period.to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: regime_to_save.map(str::to_string),
        },
    )
    .await
    .expect("vat profile");
    if legacy_missing_regime {
        sqlx::query(
            "UPDATE vat_profiles SET vat_filing_deadline_regime = NULL WHERE workspace_id = ?1",
        )
        .bind(&workspace_id)
        .execute(&pool)
        .await
        .expect("legacy null regime");
    }

    (pool, workspace_id)
}

fn calendar_input(as_of_date: &str) -> TaxTaskListInput {
    TaxTaskListInput {
        as_of_date: as_of_date.to_string(),
    }
}

#[tokio::test]
async fn tax_task_calendar_uses_fixture_schedule_provenance_and_deterministic_ordering() {
    let scenario = load_golden_scenario("tax-task-calendar").expect("golden scenario");
    let expected = scenario.expected.as_object().expect("expected");
    let registered = expected["registeredQuarterlyOpenPeriod"].as_object().expect("registered");
    let year_end = expected["yearEnd"].as_object().expect("year end");
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "registered", "quarterly", Some("quarterly_12")).await;

    let tasks = tax_tasks::tax_task_list(&pool, &workspace_id, &calendar_input("2026-05-01"))
        .await
        .expect("task list");
    let vat_task = tasks
        .iter()
        .find(|task| task.kind == "vat_return")
        .expect("VAT task");
    assert_eq!(vat_task.status, registered["status"].as_str().expect("status"));
    assert_eq!(vat_task.target, registered["target"].as_str().expect("target"));
    assert_eq!(vat_task.period_key, registered["periodKey"].as_str().expect("period key"));
    assert_eq!(vat_task.due_on.as_deref(), registered["dueOn"].as_str());
    assert_eq!(vat_task.rule_version_id, "rv-2026-active");
    assert_eq!(vat_task.tax_year, 2026);
    assert!(scenario.sources.contains(&vat_task.source_url));

    let year_end_task = tasks
        .iter()
        .find(|task| task.kind == "year_end")
        .expect("year end task");
    assert_eq!(year_end_task.status, year_end["openStatus"].as_str().expect("status"));
    assert_eq!(year_end_task.due_on.as_deref(), year_end["dueOn"].as_str());
    assert!(tasks.iter().position(|task| task.id == year_end_task.id)
        < tasks.iter().position(|task| task.id == vat_task.id));
}

#[tokio::test]
async fn tax_task_calendar_marks_past_due_vat_as_overdue_and_exempt_profiles_have_none() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "registered", "quarterly", Some("quarterly_12")).await;
    let overdue = tax_tasks::tax_task_list(&pool, &workspace_id, &calendar_input("2026-05-13"))
        .await
        .expect("overdue task list");
    assert_eq!(
        overdue.iter().find(|task| task.kind == "vat_return").expect("VAT task").status,
        "overdue"
    );
    let unresolved = tax_tasks::tax_task_list(&pool, &workspace_id, &calendar_input("2026-08-18"))
        .await
        .expect("unresolved task list");
    let periods: Vec<&str> = unresolved
        .iter()
        .filter(|task| task.kind == "vat_return")
        .map(|task| task.period_key.as_str())
        .collect();
    assert_eq!(periods, vec!["2026-Q1", "2026-Q2", "2026-Q3"]);

    let exempt_dir = tempdir().expect("tempdir");
    let (exempt_pool, exempt_workspace_id) =
        setup_workspace(&exempt_dir, "exempt_low_turnover", "quarterly", None).await;
    let exempt = tax_tasks::tax_task_list(
        &exempt_pool,
        &exempt_workspace_id,
        &calendar_input("2026-05-01"),
    )
    .await
    .expect("exempt task list");
    assert!(exempt.iter().all(|task| task.kind != "vat_return"));
}

#[tokio::test]
async fn tax_task_calendar_routes_missing_deadline_regime_to_undated_profile_review() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "registered", "quarterly", None).await;

    let tasks = tax_tasks::tax_task_list(&pool, &workspace_id, &calendar_input("2026-05-01"))
        .await
        .expect("task list");
    let review = tasks
        .iter()
        .find(|task| task.kind == "profile_review")
        .expect("profile review");
    assert_eq!(review.status, "date_unavailable");
    assert_eq!(review.target, "onboarding");
    assert_eq!(review.due_on, None);
    assert_eq!(tasks.last().expect("last task").id, review.id);
}

#[tokio::test]
async fn tax_task_calendar_keeps_exported_approved_return_external_submission_required() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "registered", "quarterly", Some("quarterly_12")).await;
    let draft = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "tax-task-draft".to_string(),
        },
    )
    .await
    .expect("draft");
    vat::vat_return_approve(
        &pool,
        &workspace_id,
        &VatReturnApproveInput {
            vat_return_id: draft.id.clone(),
            idempotency_key: "tax-task-approve".to_string(),
        },
    )
    .await
    .expect("approve");
    vat::vat_return_export(
        &pool,
        &workspace_id,
        &VatReturnExportInput {
            vat_return_id: draft.id,
            export_directory: None,
        },
    )
    .await
    .expect("export");

    let tasks = tax_tasks::tax_task_list(&pool, &workspace_id, &calendar_input("2026-05-01"))
        .await
        .expect("task list");
    assert_eq!(
        tasks.iter().find(|task| task.kind == "vat_return").expect("VAT task").status,
        "prepared_external_submission_required"
    );
}

#[tokio::test]
async fn vat_return_trace_returns_only_workspace_owned_concrete_voucher_ids() {
    let dir = tempdir().expect("tempdir");
    let (pool, workspace_id) = setup_workspace(&dir, "registered", "quarterly", Some("quarterly_12")).await;
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace_id,
        &CounterpartyCreateInput {
            kind: "customer".to_string(),
            name: "Trace customer".to_string(),
            email: None,
            org_number: None,
        },
    )
    .await
    .expect("customer");
    let invoice = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id.clone(),
            due_date: Some("2026-03-31".to_string()),
            lines: vec![InvoiceLineInput {
                description: "Traceable work".to_string(),
                quantity: 1,
                unit_price_minor: 10_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("invoice draft");
    let issued = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: invoice.id,
            idempotency_key: "trace-issue".to_string(),
            issue_date: Some("2026-03-01".to_string()),
        },
    )
    .await
    .expect("issue");
    let vat_return = vat::vat_return_draft_create(
        &pool,
        &workspace_id,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".to_string(),
            idempotency_key: "trace-vat-draft".to_string(),
        },
    )
    .await
    .expect("VAT draft");
    let later_invoice = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: Some("2026-03-31".to_string()),
            lines: vec![InvoiceLineInput {
                description: "Later traceable work".to_string(),
                quantity: 1,
                unit_price_minor: 5_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("later invoice draft");
    let later_issued = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {
            invoice_id: later_invoice.id,
            idempotency_key: "later-trace-issue".to_string(),
            issue_date: Some("2026-03-15".to_string()),
        },
    )
    .await
    .expect("later issue");

    let trace = vat::vat_return_trace(
        &pool,
        &workspace_id,
        &VatReturnTraceInput {
            vat_return_id: vat_return.id,
        },
    )
    .await
    .expect("trace");
    assert_eq!(trace.rule_version_id, "rv-2026-active");
    let trace_box = trace
        .boxes
        .iter()
        .find(|box_trace| box_trace.box_number == "10")
        .expect("box 10 trace");
    assert_eq!(trace_box.amount_minor, 3_750);
    assert!(trace_box
        .voucher_ids
        .contains(&issued.voucher_id.clone().expect("voucher id")));
    assert!(trace_box
        .voucher_ids
        .contains(&later_issued.voucher_id.clone().expect("later voucher id")));
}
