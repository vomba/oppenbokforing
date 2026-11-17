//! First cash behavioral tracer: identity-only issue, not receipt recognition.
//! Oracle: reviewed 2026-cash-pdf-full evidence. No historical seeding or live-rule
//! activation SQL is used to make this new business appear eligible.

use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    invoicing::{
        self,
        cash::{
            self, CashBusinessOriginFacts, CashHistoryGetInput, CashIssueFacts, CashIssueInput,
            CashIssuePreflightInput,
        },
        InvoiceCreateDraftInput, InvoiceLineInput,
    },
    profiles::{
        self, BusinessProfileSaveInput, OnboardingProfileSaveInput, TaxProfileSaveInput,
        VatProfileSaveInput,
    },
    workspace::{bootstrap_accounting_defaults, ensure_workspace_ready},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use tempfile::tempdir;
use uuid::Uuid;

const CASH_DIGEST: &str = "08d21e5dc4a055698b6a180e665141770bdb7f2c7325eea4e6d5bee4d54e7f7b";
const VALIDATION: &str = "cash-recognition-se-service25-2026-pdf-full-v1";
const RULE: &str = "rv-2026-method-recognition-v2";

async fn assert_no_recognition(pool: &SqlitePool, workspace: &str) {
    let vouchers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vouchers WHERE workspace_id=?1")
        .bind(workspace)
        .fetch_one(pool)
        .await
        .unwrap();
    let lines: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines j JOIN vouchers v ON v.id=j.voucher_id WHERE v.workspace_id=?1")
        .bind(workspace).fetch_one(pool).await.unwrap();
    let receivable_lines: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE a.workspace_id=?1 AND a.number='1510'")
        .bind(workspace).fetch_one(pool).await.unwrap();
    assert_eq!(vouchers, 0, "cash issue must not recognize a voucher");
    assert_eq!(lines, 0, "cash issue must not recognize any journal line");
    assert_eq!(receivable_lines, 0, "cash issue must not invent 1510");
}

#[tokio::test]
async fn qualifying_cash_issue_preserves_identity_without_recognition() {
    let bytes = include_bytes!("../../fixtures/accounting-validation/2026-cash-pdf-full.json");
    assert_eq!(format!("{:x}", Sha256::digest(bytes)), CASH_DIGEST);
    let oracle: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(oracle["validationId"], VALIDATION);
    assert_eq!(oracle["example"]["businessStartedOn"], "2026-01-01");
    assert_eq!(oracle["example"]["firstFinancialYearEndsOn"], "2026-12-31");
    assert_eq!(oracle["example"]["completedOn"], "2026-01-10");
    assert_eq!(oracle["example"]["issueDate"], "2026-01-15");
    assert_eq!(oracle["example"]["netMinor"], 1_000_000);
    assert_eq!(oracle["example"]["vatMinor"], 250_000);
    assert_eq!(oracle["example"]["grossMinor"], 1_250_000);
    assert_eq!(
        oracle["recognition"]["cashIssueIdentity"]["voucherCountDelta"],
        0
    );
    assert_eq!(
        oracle["recognition"]["cashIssueIdentity"]["journalLineCountDelta"],
        0
    );

    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("documents")).unwrap();
    std::fs::create_dir_all(dir.path().join("exports")).unwrap();
    let database = dir.path().join("workspace.sqlite");
    let pool = connect_workspace(&database)
        .await
        .expect("fresh migrated database");
    let workspace = Uuid::new_v4().to_string();
    // Workspace creation is the existing integration-test setup convention.
    // All business/profile/draft state below uses public services, not gate edits.
    sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES(?1,'Cash identity tracer',?2,?3,?4)")
        .bind(&workspace).bind(database.to_string_lossy().as_ref())
        .bind(dir.path().join("documents").to_string_lossy().as_ref())
        .bind(dir.path().join("exports").to_string_lossy().as_ref())
        .execute(&pool).await.expect("workspace");
    ensure_workspace_ready(&pool, &workspace).await.unwrap();
    // Explicit event-year bootstrap keeps later processing years valid without
    // a test clock or pretending that the business started in the current year.
    bootstrap_accounting_defaults(&pool, &workspace, 2026)
        .await
        .unwrap();
    profiles::save_onboarding_profiles(
        &pool,
        &workspace,
        &OnboardingProfileSaveInput {
            business: BusinessProfileSaveInput {
                business_name: "New Cash Firma".into(),
                owner_name: "Owner".into(),
                residency_country: Some("SE".into()),
                sni_code: Some("62010".into()),
            },
            tax: TaxProfileSaveInput {
                tax_status: "f_skatt".into(),
                expected_business_profit_minor: Some(1_000_000),
                expected_salary_income_minor: Some(0),
                active_rule_year: Some(2026),
            },
            vat: VatProfileSaveInput {
                vat_status: "registered".into(),
                reporting_period: "yearly".into(),
                accounting_method: "cash_method".into(),
                voluntary_registration_date: None,
                vat_filing_deadline_regime: Some("annual_may_12".into()),
            },
        },
    )
    .await
    .expect("new annual registered cash business with F-skatt");
    let customer = counterparties::create_counterparty(
        &pool,
        &workspace,
        &CounterpartyCreateInput {
            kind: "customer".into(),
            name: "Domestic Buyer".into(),
            email: None,
            org_number: None,
        },
    )
    .await
    .unwrap();
    let draft = invoicing::create_draft(
        &pool,
        &workspace,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: Some("2026-02-14".into()),
            lines: vec![InvoiceLineInput {
                description: "Ordinary completed domestic service".into(),
                quantity: 1,
                unit_price_minor: 1_000_000,
                vat_rate: 0.25,
                account_number: Some("3041".into()),
            }],
        },
    )
    .await
    .expect("cash draft remains available");
    assert_eq!(draft.status, "draft");
    assert_eq!(draft.total_ex_vat_minor, 1_000_000);
    assert_eq!(draft.total_vat_minor, 250_000);
    assert_eq!(draft.total_inc_vat_minor, 1_250_000);
    assert_no_recognition(&pool, &workspace).await;
    let issued_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoices WHERE workspace_id=?1 AND status!='draft'",
    )
    .bind(&workspace)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        issued_count, 0,
        "actual issuance history must also be empty"
    );
    let year = sqlx::query("SELECT starts_on,ends_on FROM fiscal_years WHERE id=?1")
        .bind(format!("fy-{workspace}-2026"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(year.get::<String, _>("starts_on"), "2026-01-01");
    assert_eq!(year.get::<String, _>("ends_on"), "2026-12-31");

    let facts = CashIssueFacts {
        ordinary_domestic_service: Some(true),
        service_completed_on: Some("2026-01-10".into()),
        prior_payment_received: Some(false),
        no_eu_trade_in_year: Some(true),
        annual_vat_registration_confirmed: Some(true),
        expected_taxable_base_within_limit: Some(true),
        expected_net_turnover_within_cash_limit: Some(true),
        ordinary_timely_sending_confirmed: Some(true),
        registered_bokslutsmetoden_confirmed: Some(true),
        cash_treatment_consistent_confirmed: Some(true),
        // Full twelve-month first year: the conditional short-year review is
        // genuinely not applicable, not silently inferred as a favorable fact.
        short_year_turnover_adjustment_reviewed: None,
    };
    let origin_review = CashBusinessOriginFacts {
        new_business_started_in_2026: Some(true),
        business_started_on: Some("2026-01-01".into()),
        first_financial_year_ends_on: Some("2026-12-31".into()),
        zero_opening_balances_confirmed: Some(true),
        no_opening_unpaid_invoices_or_bills_confirmed: Some(true),
        no_prior_unrecorded_activity_confirmed: Some(true),
        outside_workspace_history_reviewed: Some(true),
    };
    println!("Setup complete: clean annual cash/F-skatt workspace, explicit origin/facts, exact 1000000/250000/1250000 draft; calling public cash preflight");
    let review = cash::invoice_cash_issue_preflight(
        &pool,
        &workspace,
        &CashIssuePreflightInput {
            invoice_id: draft.id.clone(),
            issue_date: "2026-01-15".into(),
            facts: facts.clone(),
            origin_review: Some(origin_review.clone()),
        },
    )
    .await
    .expect("qualifying cash issue must reach a review without invoice-date recognition");
    assert_eq!(review.invoice_id, draft.id);
    assert_eq!(review.assessment.validation_id, VALIDATION);
    assert_eq!(review.assessment.evidence_digest, CASH_DIGEST);
    assert_eq!(review.assessment.rule_version_id, RULE);
    assert_eq!(review.assessment.accounting_date, None);
    assert_eq!(review.assessment.completed_on, "2026-01-10");
    assert_eq!(review.assessment.issued_on, "2026-01-15");
    assert_eq!(review.assessment.accounting_year, 2026);
    assert_eq!(review.assessment.period_key, "2026");
    assert_eq!(review.assessment.due_on, "2027-05-12");
    assert_eq!(review.assessment.deadline_regime, "annual_may_12");
    assert_eq!(review.reviewed_invoice.total_inc_vat_minor, 1_250_000);
    assert_eq!(review.current_taxable_base_minor, 0);
    assert_eq!(review.projected_taxable_base_minor, 1_000_000);
    assert_no_recognition(&pool, &workspace).await;

    let issued = cash::invoice_cash_issue(
        &pool,
        &workspace,
        &CashIssueInput {
            invoice_id: draft.id.clone(),
            issue_date: "2026-01-15".into(),
            facts: facts.clone(),
            origin_review: Some(origin_review.clone()),
            idempotency_key: "cash-identity-first-issue".into(),
            review_fingerprint: review.assessment.review_fingerprint.clone(),
        },
    )
    .await
    .expect("qualifying reviewed cash issue must preserve identity only");
    assert_eq!(issued.id, draft.id);
    assert_eq!(issued.status, "issued");
    assert_eq!(issued.invoice_number.as_deref(), Some("2026-0001"));
    assert_eq!(issued.issue_date.as_deref(), Some("2026-01-15"));
    assert_eq!(issued.voucher_id, None);
    assert_eq!(issued.payment_voucher_id, None);
    assert_eq!(issued.total_ex_vat_minor, 1_000_000);
    assert_eq!(issued.total_vat_minor, 250_000);
    assert_eq!(issued.total_inc_vat_minor, 1_250_000);
    let job = sqlx::query("SELECT job_type,status FROM local_jobs WHERE workspace_id=?1 AND id=?2")
        .bind(&workspace)
        .bind(issued.pdf_job_id.as_ref().expect("issued PDF job"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(job.get::<String, _>("job_type"), "invoice_pdf_generate");
    assert_eq!(job.get::<String, _>("status"), "queued");
    let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE workspace_id=?1 AND resource_type='invoice' AND resource_id=?2 AND action='invoice_issue'")
        .bind(&workspace).bind(&issued.id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        audit_count, 1,
        "numbered identity retains immutable issue audit"
    );
    assert_no_recognition(&pool, &workspace).await;

    let origin = cash::cash_business_origin_get(&pool, &workspace)
        .await
        .unwrap()
        .expect("first successful cash issue anchors origin");
    assert_eq!(origin.first_invoice_id, issued.id);
    assert_eq!(origin.id, review.assessment.origin_id);
    assert_eq!(
        serde_json::to_value(&origin.facts).unwrap(),
        serde_json::to_value(&origin_review).unwrap()
    );
    let history = cash::invoice_cash_history_get(
        &pool,
        &workspace,
        &CashHistoryGetInput {
            invoice_id: issued.id.clone(),
        },
    )
    .await
    .unwrap()
    .expect("issued cash identity retains provenance");
    assert_eq!(history.invoice_id, issued.id);
    assert_eq!(history.origin.id, origin.id);
    assert_eq!(history.issue.invoice_id, issued.id);
    assert_eq!(
        serde_json::to_value(&history.issue.facts).unwrap(),
        serde_json::to_value(&facts).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&history.issue.assessment).unwrap(),
        serde_json::to_value(&review.assessment).unwrap()
    );
    assert!(history.receipt.is_none(), "issue is not bank receipt proof");
}
