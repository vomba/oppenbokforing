//! Public-boundary tests for the accepted case, not whole-book/PDF validation.
//! Expectations come from the independently derived evidence JSON. Historical
//! fixtures used below are engineering regressions, never validation evidence.
pub mod common;

use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    error::AppError,
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceEligibilityFacts, InvoiceIssueInput,
        InvoiceIssuePreflightInput, InvoiceIssueValidationGetInput, InvoiceLineInput,
    },
    ledger::{self, VatBucket},
    profiles::{self, BusinessProfileSaveInput, TaxProfileSaveInput, VatProfileSaveInput},
    workspace::ensure_workspace_ready,
};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tempfile::{tempdir, TempDir};
use uuid::Uuid;

const RULE: &str = "rv-2026-domestic-service25-exact-v1";
fn evidence() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
    ))
    .unwrap()
}
struct Fixture {
    _dir: TempDir,
    pool: SqlitePool,
    workspace: String,
    customer: String,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite");
        let pool = connect_workspace(&path).await.unwrap();
        let workspace = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES(?1,'Eligibility',?2,?3,?4)")
            .bind(&workspace).bind(path.to_string_lossy().as_ref())
            .bind(dir.path().join("documents").to_string_lossy().as_ref())
            .bind(dir.path().join("exports").to_string_lossy().as_ref()).execute(&pool).await.unwrap();
        ensure_workspace_ready(&pool, &workspace).await.unwrap();
        profiles::save_business_profile(
            &pool,
            &workspace,
            &BusinessProfileSaveInput {
                business_name: "Test firma".into(),
                owner_name: "Owner".into(),
                residency_country: Some("SE".into()),
                sni_code: None,
            },
        )
        .await
        .unwrap();
        profiles::save_tax_profile(
            &pool,
            &workspace,
            &TaxProfileSaveInput {
                tax_status: "f_skatt".into(),
                expected_business_profit_minor: Some(1_000_000),
                expected_salary_income_minor: Some(0),
                active_rule_year: Some(2026),
            },
        )
        .await
        .unwrap();
        profiles::save_vat_profile(
            &pool,
            &workspace,
            &VatProfileSaveInput {
                vat_status: "registered".into(),
                reporting_period: "yearly".into(),
                accounting_method: "invoice_method".into(),
                voluntary_registration_date: None,
                vat_filing_deadline_regime: Some("annual_may_12".into()),
            },
        )
        .await
        .unwrap();
        let customer = counterparties::create_counterparty(
            &pool,
            &workspace,
            &CounterpartyCreateInput {
                kind: "customer".into(),
                name: "Buyer".into(),
                email: None,
                org_number: None,
            },
        )
        .await
        .unwrap()
        .id;
        Self {
            _dir: dir,
            pool,
            workspace,
            customer,
        }
    }
    async fn draft(&self, lines: Vec<InvoiceLineInput>) -> invoicing::InvoiceSummary {
        invoicing::create_draft(
            &self.pool,
            &self.workspace,
            &InvoiceCreateDraftInput {
                counterparty_id: self.customer.clone(),
                due_date: Some("2027-01-30".into()),
                lines,
            },
        )
        .await
        .unwrap()
    }
    fn input(&self, id: &str) -> InvoiceIssueInput {
        InvoiceIssueInput {
            invoice_id: id.into(),
            idempotency_key: Uuid::new_v4().to_string(),
            issue_date: Some("2026-12-31".into()),
            eligibility_facts: Some(common::facts("2026-12-31")),
            review_fingerprint: None,
        }
    }
    async fn preflight(
        &self,
        input: &InvoiceIssueInput,
    ) -> Result<invoicing::InvoiceIssuePreflight, AppError> {
        invoicing::invoice_issue_preflight(
            &self.pool,
            &self.workspace,
            &InvoiceIssuePreflightInput {
                invoice_id: input.invoice_id.clone(),
                issue_date: input.issue_date.clone(),
                eligibility_facts: input.eligibility_facts.clone(),
            },
        )
        .await
    }
    async fn review(&self, mut input: InvoiceIssueInput) -> InvoiceIssueInput {
        input.review_fingerprint = Some(
            self.preflight(&input)
                .await
                .unwrap()
                .eligibility
                .expect("success must carry case assessment")
                .review_fingerprint,
        );
        input
    }
    async fn issue(
        &self,
        input: &InvoiceIssueInput,
    ) -> Result<invoicing::InvoiceSummary, AppError> {
        invoicing::issue_invoice(&self.pool, &self.workspace, input).await
    }
    async fn state(&self) -> Vec<i64> {
        let mut state = Vec::new();
        for table in [
            "vouchers",
            "journal_lines",
            "invoice_issue_validations",
            "invoice_issue_snapshots",
            "local_jobs",
            "audit_events",
        ] {
            state.push(
                sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                    .fetch_one(&self.pool)
                    .await
                    .unwrap(),
            );
        }
        state.push(
            sqlx::query_scalar("SELECT SUM(next_number) FROM invoice_sequences")
                .fetch_one(&self.pool)
                .await
                .unwrap(),
        );
        state.push(
            sqlx::query_scalar("SELECT COUNT(*) FROM invoices WHERE status!='draft'")
                .fetch_one(&self.pool)
                .await
                .unwrap(),
        );
        state
    }
    async fn rejects_both(&self, input: &InvoiceIssueInput) {
        let before = self.state().await;
        eligibility_error(
            self.preflight(input)
                .await
                .expect_err("unsupported preflight"),
        );
        eligibility_error(
            self.issue(input)
                .await
                .expect_err("unsupported new issuance"),
        );
        assert_eq!(
            before,
            self.state().await,
            "rejection cannot allocate/post/persist provenance"
        );
    }
}
fn line(quantity: i64, unit: i64, rate: f64) -> InvoiceLineInput {
    InvoiceLineInput {
        description: "Ordinary domestic completed service".into(),
        quantity,
        unit_price_minor: unit,
        vat_rate: rate,
        account_number: Some("3041".into()),
    }
}
fn eligibility_error(error: AppError) {
    assert_eq!(error.code, "validation_error", "{error:?}");
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|d| d.first())
            .and_then(|d| d.field.as_deref()),
        Some("eligibility"),
        "{error:?}"
    );
}

#[tokio::test]
async fn independently_derived_exact_cases_reach_journal_and_correct_annual_deadline() {
    let evidence = evidence();
    for case in evidence["expected"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case.get("blocked").is_none())
    {
        let f = Fixture::new().await;
        let draft = f
            .draft(
                case["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|l| {
                        line(
                            l["quantity"].as_i64().unwrap(),
                            l["unitPriceMinor"].as_i64().unwrap(),
                            0.25,
                        )
                    })
                    .collect(),
            )
            .await;
        let input = f.review(f.input(&draft.id)).await;
        let assessment = f.preflight(&input).await.unwrap().eligibility.unwrap();
        assert_eq!(assessment.validation_id, evidence["validationId"]);
        assert_eq!(assessment.rule_version_id, evidence["ruleVersionId"]);
        assert_eq!(assessment.accounting_year, 2026);
        assert_eq!(
            assessment.accounting_date,
            evidence["recognition"]["accountingDate"]
        );
        assert_eq!(
            assessment.completed_on,
            evidence["recognition"]["completedOn"]
        );
        assert_eq!(assessment.deadline_regime, "annual_may_12");
        assert_eq!(
            assessment.period_key,
            evidence["calendar"]["accountingPeriod"]
        );
        assert_eq!(assessment.due_on, evidence["calendar"]["dueOn"]);
        assert!(!assessment.coverage_summary.is_empty());
        let issued = f.issue(&input).await.unwrap();
        assert_eq!(issued.total_ex_vat_minor, case["netMinor"]);
        assert_eq!(issued.total_vat_minor, case["vatMinor"]);
        assert_eq!(issued.total_inc_vat_minor, case["grossMinor"]);
        let journal: Vec<(String,i64,i64)> = sqlx::query_as("SELECT a.number,j.debit_minor,j.credit_minor FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE j.voucher_id=?1 ORDER BY a.number")
            .bind(issued.voucher_id.as_deref().unwrap()).fetch_all(&f.pool).await.unwrap();
        assert_eq!(
            journal,
            vec![
                ("1510".into(), case["grossMinor"].as_i64().unwrap(), 0),
                ("2611".into(), 0, case["vatMinor"].as_i64().unwrap()),
                ("3041".into(), 0, case["netMinor"].as_i64().unwrap())
            ]
        );
        let date: String = sqlx::query_scalar("SELECT accounting_date FROM vouchers WHERE id=?1")
            .bind(issued.voucher_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(date, "2026-12-31");
        assert!(journal.iter().all(|(account, _, _)| account != "1930"));
    }
}

#[tokio::test]
async fn every_fact_requires_an_explicit_favorable_answer_and_review_is_required() {
    let f = Fixture::new().await;
    let draft = f.draft(vec![line(1, 1_000_000, 0.25)]).await;
    let good = f.input(&draft.id);
    let mut missing = good.clone();
    missing.eligibility_facts = None;
    f.rejects_both(&missing).await;
    let facts = serde_json::to_value(good.eligibility_facts.as_ref().unwrap()).unwrap();
    for key in facts.as_object().unwrap().keys() {
        let mut omitted = facts.clone();
        omitted.as_object_mut().unwrap().remove(key);
        let mut input = good.clone();
        input.eligibility_facts =
            Some(serde_json::from_value::<InvoiceEligibilityFacts>(omitted).unwrap());
        f.rejects_both(&input).await;
        for invalid in [
            Value::Null,
            if key == "serviceCompletedOn" {
                json!("not-a-date")
            } else {
                json!(key == "priorPaymentReceived")
            },
        ] {
            let mut changed = facts.clone();
            changed[key] = invalid;
            let mut input = good.clone();
            input.eligibility_facts =
                Some(serde_json::from_value::<InvoiceEligibilityFacts>(changed).unwrap());
            f.rejects_both(&input).await;
        }
    }
    assert!(f.preflight(&good).await.unwrap().eligibility.is_some());
    let before = f.state().await;
    eligibility_error(
        f.issue(&good)
            .await
            .expect_err("no final-review fingerprint"),
    );
    let mut empty = good.clone();
    empty.review_fingerprint = Some(String::new());
    eligibility_error(f.issue(&empty).await.expect_err("empty fingerprint"));
    assert_eq!(before, f.state().await);
}

#[tokio::test]
async fn completion_must_be_in_2026_and_not_after_sending_but_can_be_earlier_same_year() {
    let f = Fixture::new().await;
    let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
    for (issue, completion) in [
        ("2026-06-01", "2026-06-02"),
        ("2026-06-01", "2025-12-31"),
        ("2027-01-01", "2026-12-31"),
        ("2025-12-31", "2025-12-31"),
    ] {
        let mut input = f.input(&draft.id);
        input.issue_date = Some(issue.into());
        input.eligibility_facts = Some(common::facts(completion));
        f.rejects_both(&input).await;
    }
    let mut earlier = f.input(&draft.id);
    earlier.eligibility_facts = Some(common::facts("2026-01-01"));
    let reviewed = f.review(earlier).await;
    let assessment = f.preflight(&reviewed).await.unwrap().eligibility.unwrap();
    assert_eq!(assessment.completed_on, "2026-01-01");
    assert_eq!(assessment.accounting_date, "2026-12-31");
    f.issue(&reviewed).await.unwrap();
}

#[tokio::test]
async fn unsupported_rates_accounts_calendars_exemption_and_fractional_lines_are_rejected() {
    for rates in [vec![0.0], vec![0.06], vec![0.12], vec![0.25, 0.12]] {
        let f = Fixture::new().await;
        let draft = f
            .draft(rates.into_iter().map(|r| line(1, 100_000, r)).collect())
            .await;
        f.rejects_both(&f.input(&draft.id)).await;
    }
    for units in [vec![1], vec![2], vec![3], vec![1, 3]] {
        let f = Fixture::new().await;
        let draft = f
            .draft(units.into_iter().map(|u| line(1, u, 0.25)).collect())
            .await;
        f.rejects_both(&f.input(&draft.id)).await;
    }
    for mutation in [
        "UPDATE vat_profiles SET reporting_period='quarterly',vat_filing_deadline_regime='quarterly_12'",
        "UPDATE vat_profiles SET reporting_period='monthly',vat_filing_deadline_regime='monthly_26'",
        "UPDATE vat_profiles SET vat_filing_deadline_regime='annual_feb_26'",
        "UPDATE vat_profiles SET vat_status='exempt_low_turnover'",
        "UPDATE vat_profiles SET vat_status='voluntary_registered',voluntary_registration_date='2027-01-01'",
        "UPDATE invoice_lines SET account_number='3042'",
    ] {
        let f = Fixture::new().await; let draft = f.draft(vec![line(1,100_000,0.25)]).await;
        sqlx::query(mutation).execute(&f.pool).await.unwrap();
        f.rejects_both(&f.input(&draft.id)).await;
    }
}

#[tokio::test]
async fn voluntary_registration_effectiveness_compares_dates_not_text() {
    for (registration, eligible) in [
        ("2027-01-01", false),
        ("+2027-01-01", false),
        ("2026-01-01", true),
    ] {
        let f = Fixture::new().await;
        profiles::save_vat_profile(
            &f.pool,
            &f.workspace,
            &VatProfileSaveInput {
                vat_status: "voluntary_registered".into(),
                reporting_period: "yearly".into(),
                accounting_method: "invoice_method".into(),
                voluntary_registration_date: Some(registration.into()),
                vat_filing_deadline_regime: Some("annual_may_12".into()),
            },
        )
        .await
        .expect("the existing profile API accepts parseable signed years");
        let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
        let input = f.input(&draft.id);
        if eligible {
            let reviewed = f.review(input).await;
            f.issue(&reviewed).await.unwrap();
        } else {
            f.rejects_both(&input).await;
        }
    }
}

#[tokio::test]
async fn stale_final_review_is_not_permission_for_changed_facts_date_profile_or_lines() {
    for change in ["facts", "date", "profile", "lines"] {
        let f = Fixture::new().await;
        let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
        let mut input = f.review(f.input(&draft.id)).await;
        match change {
            "facts" => {
                input
                    .eligibility_facts
                    .as_mut()
                    .unwrap()
                    .service_completed_on = Some("2026-12-30".into())
            }
            "date" => {
                input.issue_date = Some("2026-12-30".into());
                input
                    .eligibility_facts
                    .as_mut()
                    .unwrap()
                    .service_completed_on = Some("2026-12-30".into());
            }
            "profile" => {
                sqlx::query("UPDATE vat_profiles SET voluntary_registration_date='2026-01-01'")
                    .execute(&f.pool)
                    .await
                    .unwrap();
            }
            "lines" => {
                invoicing::update_draft(
                    &f.pool,
                    &f.workspace,
                    &invoicing::InvoiceUpdateDraftInput {
                        invoice_id: draft.id.clone(),
                        due_date: None,
                        lines: vec![line(1, 200_000, 0.25)],
                    },
                )
                .await
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = f.state().await;
        eligibility_error(f.issue(&input).await.expect_err(change));
        assert_eq!(before, f.state().await);
        // A genuinely fresh review of still-supported facts can proceed.
        let refreshed = f.review(input).await;
        f.issue(&refreshed).await.unwrap();
    }
}

#[tokio::test]
async fn checked_arithmetic_and_stored_header_integrity_fail_closed() {
    for mutation in [
        "UPDATE invoice_lines SET quantity=2,unit_price_minor=9223372036854775807",
        "UPDATE invoice_lines SET quantity=1,unit_price_minor=9223372036854775804",
        "UPDATE invoices SET total_ex_vat_minor=100004",
        "UPDATE invoices SET total_vat_minor=25001",
        "UPDATE invoices SET total_inc_vat_minor=125001",
    ] {
        let f = Fixture::new().await;
        let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
        sqlx::query(mutation).execute(&f.pool).await.unwrap();
        f.rejects_both(&f.input(&draft.id)).await;
    }
    // Individually valid line amounts can still overflow aggregate gross
    // (two lines) or aggregate net (three lines).
    for count in [2, 3] {
        let f = Fixture::new().await;
        let draft = f
            .draft((0..count).map(|_| line(1, 4, 0.25)).collect())
            .await;
        sqlx::query("UPDATE invoice_lines SET unit_price_minor=4000000000000000000")
            .execute(&f.pool)
            .await
            .unwrap();
        f.rejects_both(&f.input(&draft.id)).await;
    }
}

#[tokio::test]
async fn annual_taxable_base_limit_is_inclusive_and_uses_the_ledger_not_profit_defaults() {
    for (existing, allowed) in [(99_000_000, true), (99_000_004, false)] {
        let f = Fixture::new().await;
        let old = f.draft(vec![line(1, existing, 0.25)]).await;
        let mut historical = f.input(&old.id);
        historical.eligibility_facts = None;
        common::seed_historical_invoice(&f.pool, &f.workspace, &historical)
            .await
            .unwrap();
        let draft = f.draft(vec![line(1, 1_000_000, 0.25)]).await;
        let input = f.input(&draft.id);
        if allowed {
            let reviewed = f.review(input).await;
            f.issue(&reviewed).await.unwrap();
        } else {
            f.rejects_both(&input).await;
        }
    }
}

#[tokio::test]
async fn provenance_number_snapshot_pdf_job_voucher_audit_and_replay_are_atomic_and_immutable() {
    let f = Fixture::new().await;
    let draft = f.draft(vec![line(1, 1_000_000, 0.25)]).await;
    let input = f.review(f.input(&draft.id)).await;
    let before = f.state().await;
    sqlx::query("CREATE TRIGGER fail_issue_audit BEFORE INSERT ON audit_events WHEN NEW.action='invoice_issue' BEGIN SELECT RAISE(ABORT,'injected issue audit failure'); END").execute(&f.pool).await.unwrap();
    f.issue(&input)
        .await
        .expect_err("late failure rolls back every issuance write");
    assert_eq!(before, f.state().await);
    sqlx::query("DROP TRIGGER fail_issue_audit")
        .execute(&f.pool)
        .await
        .unwrap();
    let issued = f.issue(&input).await.unwrap();
    assert_eq!(issued.invoice_number.as_deref(), Some("2026-0001"));
    assert!(issued.voucher_id.is_some());
    assert!(issued.pdf_job_id.is_some());
    let issue_audit: String = sqlx::query_scalar("SELECT metadata_json FROM audit_events WHERE workspace_id=?1 AND resource_id=?2 AND action='invoice_issue'").bind(&f.workspace).bind(&draft.id).fetch_one(&f.pool).await.unwrap();
    let audit: Value = serde_json::from_str(&issue_audit).unwrap();
    assert_eq!(audit["voucherId"], issued.voucher_id.as_deref().unwrap());
    assert_eq!(audit["pdfJobId"], issued.pdf_job_id.as_deref().unwrap());
    assert_eq!(audit["invoiceNumber"], "2026-0001");
    assert_eq!(audit["ruleVersionId"], RULE);
    let (job_kind, job_status, payload): (String, String, String) =
        sqlx::query_as("SELECT job_type,status,payload_json FROM local_jobs WHERE id=?1")
            .bind(issued.pdf_job_id.as_deref().unwrap())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        (job_kind.as_str(), job_status.as_str()),
        ("invoice_pdf_generate", "queued")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap()["invoiceId"],
        draft.id
    );
    let snapshot_rule: String = sqlx::query_scalar(
        "SELECT rule_version_id FROM invoice_issue_snapshots WHERE invoice_id=?1",
    )
    .bind(&draft.id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(snapshot_rule, RULE);
    let replay_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM local_jobs WHERE job_type='invoice_issue' AND status='succeeded' AND idempotency_key=?1").bind(&input.idempotency_key).fetch_one(&f.pool).await.unwrap();
    assert_eq!(replay_count, 1);
    let provenance = invoicing::invoice_issue_validation_get(
        &f.pool,
        &f.workspace,
        &InvoiceIssueValidationGetInput {
            invoice_id: draft.id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(provenance.invoice_id, draft.id);
    assert_eq!(
        serde_json::to_value(&provenance.facts).unwrap(),
        serde_json::to_value(input.eligibility_facts.as_ref().unwrap()).unwrap()
    );
    assert_eq!(
        provenance.assessment.review_fingerprint,
        input.review_fingerprint.clone().unwrap()
    );
    assert_eq!(provenance.assessment.rule_version_id, RULE);
    assert!(!provenance.recorded_at.is_empty());
    for sql in [
        "UPDATE invoice_issue_validations SET recorded_at=recorded_at WHERE invoice_id=?1",
        "DELETE FROM invoice_issue_validations WHERE invoice_id=?1",
    ] {
        sqlx::query(sql)
            .bind(&draft.id)
            .execute(&f.pool)
            .await
            .expect_err("provenance is immutable");
    }
    // History/replay is preserved under a currently unsupported profile, without facts.
    sqlx::query("UPDATE vat_profiles SET accounting_method='cash_method',reporting_period='quarterly',vat_filing_deadline_regime='quarterly_12'").execute(&f.pool).await.unwrap();
    let state = f.state().await;
    let mut replay = input.clone();
    replay.eligibility_facts = None;
    replay.review_fingerprint = None;
    assert_eq!(f.issue(&replay).await.unwrap().id, issued.id);
    assert_eq!(state, f.state().await);
    replay.idempotency_key = "new-recovery-key".into();
    assert_eq!(f.issue(&replay).await.unwrap().id, issued.id);
    let mut after = f.state().await;
    after[4] -= 1; // only the new completed replay record
    assert_eq!(state, after);
    let retained = invoicing::invoice_issue_validation_get(
        &f.pool,
        &f.workspace,
        &InvoiceIssueValidationGetInput {
            invoice_id: draft.id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        serde_json::to_value(provenance).unwrap(),
        serde_json::to_value(retained).unwrap()
    );
    let other = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES(?1,'Other','other.sqlite','other-docs','other-exports')").bind(&other).execute(&f.pool).await.unwrap();
    assert!(invoicing::invoice_issue_validation_get(
        &f.pool,
        &other,
        &InvoiceIssueValidationGetInput {
            invoice_id: draft.id
        }
    )
    .await
    .unwrap()
    .is_none());
}

#[tokio::test]
async fn historical_unsupported_invoice_replays_without_retroactive_validation() {
    let f = Fixture::new().await;
    let draft = f.draft(vec![line(1, 100_000, 0.12)]).await;
    let mut input = f.input(&draft.id);
    input.eligibility_facts = None;
    let history = common::seed_historical_invoice(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    sqlx::query("UPDATE vat_profiles SET accounting_method='cash_method'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        f.issue(&input).await.unwrap().voucher_id,
        history.voucher_id
    );
    input.idempotency_key = "legacy-new-key".into();
    assert_eq!(
        f.issue(&input).await.unwrap().invoice_number,
        history.invoice_number
    );
    assert!(invoicing::invoice_issue_validation_get(
        &f.pool,
        &f.workspace,
        &InvoiceIssueValidationGetInput {
            invoice_id: draft.id
        }
    )
    .await
    .unwrap()
    .is_none());
}

#[tokio::test]
async fn low_level_invoice_posting_cannot_authorize_raw_buckets_or_reuse_completed_provenance() {
    let f = Fixture::new().await;
    let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
    let input = f.review(f.input(&draft.id)).await;
    let buckets = vec![VatBucket {
        ex_vat_minor: 100_000,
        vat_minor: 25_000,
        vat_code: "VAT25".into(),
    }];
    let fy = format!("fy-{}-2026", f.workspace);
    let before = f.state().await;
    eligibility_error(
        ledger::post_invoice_voucher(
            &f.pool,
            &f.workspace,
            &fy,
            &draft.id,
            "2026-12-31",
            &buckets,
        )
        .await
        .expect_err("preflight alone is not persisted pending context"),
    );
    let mut tx = f.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    eligibility_error(
        ledger::post_invoice_voucher_tx(
            &mut tx,
            &f.workspace,
            &fy,
            &draft.id,
            "2026-12-31",
            &buckets,
        )
        .await
        .expect_err("tx cannot bypass domain review"),
    );
    tx.rollback().await.unwrap();
    assert_eq!(before, f.state().await);
    f.issue(&input).await.unwrap();
    for (id, date, year) in [
        (draft.id.as_str(), "2026-12-31", fy.clone()),
        (draft.id.as_str(), "2026-12-30", fy.clone()),
        (
            draft.id.as_str(),
            "2026-12-31",
            format!("fy-{}-2027", f.workspace),
        ),
        ("other-invoice", "2026-12-31", fy.clone()),
    ] {
        let state = f.state().await;
        eligibility_error(
            ledger::post_invoice_voucher(&f.pool, &f.workspace, &year, id, date, &buckets)
                .await
                .expect_err("completed provenance cannot be reused"),
        );
        assert_eq!(state, f.state().await);
    }
}

#[tokio::test]
async fn issue_waiting_for_writer_rechecks_actual_committed_profile_and_line_values() {
    for mutation in ["UPDATE vat_profiles SET reporting_period='quarterly',vat_filing_deadline_regime='quarterly_12'", "UPDATE invoice_lines SET vat_rate_bp=1200"] {
        let f=Fixture::new().await; let draft=f.draft(vec![line(1,100_000,0.25)]).await;
        let input=f.review(f.input(&draft.id)).await;
        let mut writer=f.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let pool=f.pool.clone(); let workspace=f.workspace.clone();
        let task=tokio::spawn(async move { invoicing::issue_invoice(&pool,&workspace,&input).await });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(!task.is_finished(),"new issuance must wait for the real writer lock");
        sqlx::query(mutation).execute(&mut *writer).await.unwrap(); writer.commit().await.unwrap();
        let state=f.state().await;
        eligibility_error(tokio::time::timeout(std::time::Duration::from_secs(10),task).await.unwrap().unwrap().expect_err("must revalidate after writer commits"));
        assert_eq!(state,f.state().await);
    }
}

#[tokio::test]
async fn actual_relevant_rule_payloads_not_unchanged_ids_or_checksums_bind_eligibility() {
    let mutations = [
        "UPDATE tax_rules SET value_json='not-json' WHERE family='invoice_recognition' AND key='domestic_service25_exact'",
        "DELETE FROM tax_rules WHERE family='invoice_recognition' AND key='domestic_service25_exact'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.evidenceDigest','forged') WHERE family='invoice_recognition' AND key='domestic_service25_exact'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.validationId','other-case') WHERE family='invoice_recognition' AND key='domestic_service25_exact'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.roundingCoverage','rounded') WHERE family='invoice_recognition' AND key='domestic_service25_exact'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.vatRateBp',1200) WHERE family='invoice_recognition' AND key='domestic_service25_exact'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.annualTaxableBaseLimitMinor',100000004) WHERE family='invoice_recognition' AND key='domestic_service25_exact'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.dueOn','2026-05-12') WHERE family='invoice_recognition' AND key='domestic_service25_exact'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.schedules.annual_may_12.\"2026\"','2026-05-12') WHERE family='vat' AND key='filing_deadline_schedules' AND rule_version_id='rv-2026-domestic-service25-exact-v1'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.sourceUrl','https://example.test/unreviewed') WHERE family='vat' AND key='filing_deadline_schedules' AND rule_version_id='rv-2026-domestic-service25-exact-v1'",
        "DELETE FROM tax_rules WHERE family='vat' AND key='filing_deadline_schedules' AND rule_version_id='rv-2026-domestic-service25-exact-v1'",
    ];
    for mutation in mutations {
        let f = Fixture::new().await;
        let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
        let input = f.review(f.input(&draft.id)).await;
        let before: (String, String) =
            sqlx::query_as("SELECT id,checksum FROM rule_versions WHERE id=?1")
                .bind(RULE)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        sqlx::query(mutation).execute(&f.pool).await.unwrap();
        let after: (String, String) =
            sqlx::query_as("SELECT id,checksum FROM rule_versions WHERE id=?1")
                .bind(RULE)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        assert_eq!(
            before, after,
            "the stale review cannot rely on descriptive checksum or version identity"
        );
        f.rejects_both(&input).await;
    }
    for mutation in [
        "UPDATE rule_versions SET status='retired' WHERE id='rv-2026-domestic-service25-exact-v1'",
        "UPDATE rule_versions SET effective_from='2026-02-01' WHERE id='rv-2026-domestic-service25-exact-v1'",
        "UPDATE fiscal_years SET starts_on='2026-02-01' WHERE starts_on='2026-01-01'",
    ] {
        let f=Fixture::new().await; let draft=f.draft(vec![line(1,100_000,0.25)]).await;
        let input=f.review(f.input(&draft.id)).await;
        sqlx::query(mutation).execute(&f.pool).await.unwrap(); f.rejects_both(&input).await;
    }
}

#[tokio::test]
async fn forward_rule_version_preserves_old_engineering_rules_and_binds_exact_manifest_bytes() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new().await;
    let status: String =
        sqlx::query_scalar("SELECT status FROM rule_versions WHERE id='rv-2026-active'")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(status, "retired");
    let old_calendar:String=sqlx::query_scalar("SELECT value_json FROM tax_rules WHERE rule_version_id='rv-2026-active' AND family='vat' AND key='filing_deadline_schedules'").fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&old_calendar).unwrap()["schedules"]["annual_may_12"]["2026"],
        "2026-05-12"
    );
    let rules:Vec<(String,String,String,String)>=sqlx::query_as("SELECT old.family,old.key,old.value_json,new.value_json FROM tax_rules old JOIN tax_rules new ON new.family=old.family AND new.key=old.key WHERE old.rule_version_id='rv-2026-active' AND new.rule_version_id=?1").bind(RULE).fetch_all(&f.pool).await.unwrap();
    assert!(!rules.is_empty());
    for (family, key, old, new) in rules {
        if family == "vat" && key == "filing_deadline_schedules" {
            let mut expected: Value = serde_json::from_str(&old).unwrap();
            expected["schedules"]["annual_may_12"]["2026"] = json!("2027-05-12");
            assert_eq!(serde_json::from_str::<Value>(&new).unwrap(), expected);
        } else {
            assert_eq!(
                old, new,
                "unrelated engineering rules are copied, not promoted to validated coverage"
            );
        }
    }
    let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
    let assessment = f
        .preflight(&f.input(&draft.id))
        .await
        .unwrap()
        .eligibility
        .unwrap();
    let digest = format!(
        "{:x}",
        Sha256::digest(include_bytes!(
            "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
        ))
    );
    assert_eq!(assessment.evidence_digest, digest);
}

#[tokio::test]
async fn validated_pending_context_still_rejects_different_buckets_invoice_date_and_fiscal_year() {
    // Clone the real database BEFORE issuance; transfer only the immutable row
    // captured from successful issuance into the clone's still-pending draft.
    // This is test setup for attacking the lower boundary, never an issuance
    // helper or a runtime permit. No trigger/protection is disabled.
    let f = Fixture::new().await;
    let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
    let input = f.review(f.input(&draft.id)).await;
    let clone_path = f._dir.path().join("pending-clone.sqlite");
    sqlx::query("VACUUM INTO ?1")
        .bind(clone_path.to_string_lossy().as_ref())
        .execute(&f.pool)
        .await
        .unwrap();
    f.issue(&input).await.unwrap();
    let (facts,assessment,context):(String,String,String)=sqlx::query_as("SELECT facts_json,assessment_json,context_json FROM invoice_issue_validations WHERE invoice_id=?1").bind(&draft.id).fetch_one(&f.pool).await.unwrap();
    let pending = connect_workspace(&clone_path).await.unwrap();
    sqlx::query("INSERT INTO invoice_issue_validations(invoice_id,workspace_id,facts_json,assessment_json,context_json) VALUES(?1,?2,?3,?4,?5)")
        .bind(&draft.id).bind(&f.workspace).bind(&facts).bind(&assessment).bind(&context).execute(&pending).await.unwrap();
    // Pending facts must not appear as successful issue history.
    assert!(invoicing::invoice_issue_validation_get(
        &pending,
        &f.workspace,
        &InvoiceIssueValidationGetInput {
            invoice_id: draft.id.clone()
        }
    )
    .await
    .unwrap()
    .is_none());
    let fy = format!("fy-{}-2026", f.workspace);
    let correct = || {
        vec![VatBucket {
            ex_vat_minor: 100_000,
            vat_minor: 25_000,
            vat_code: "VAT25".into(),
        }]
    };
    for (invoice, date, year, buckets) in [
        (
            draft.id.clone(),
            "2026-12-31",
            fy.clone(),
            vec![VatBucket {
                ex_vat_minor: 200_000,
                vat_minor: 50_000,
                vat_code: "VAT25".into(),
            }],
        ),
        (
            draft.id.clone(),
            "2026-12-31",
            fy.clone(),
            vec![VatBucket {
                ex_vat_minor: 100_000,
                vat_minor: 25_000,
                vat_code: "VAT12".into(),
            }],
        ),
        (draft.id.clone(), "2026-12-30", fy.clone(), correct()),
        (
            draft.id.clone(),
            "2026-12-31",
            format!("fy-{}-2027", f.workspace),
            correct(),
        ),
        ("other-invoice".into(), "2026-12-31", fy.clone(), correct()),
    ] {
        eligibility_error(
            ledger::post_invoice_voucher(&pending, &f.workspace, &year, &invoice, date, &buckets)
                .await
                .expect_err("pending context is not a transferable permission"),
        );
        let mut tx = pending.begin_with("BEGIN IMMEDIATE").await.unwrap();
        eligibility_error(
            ledger::post_invoice_voucher_tx(&mut tx, &f.workspace, &year, &invoice, date, &buckets)
                .await
                .expect_err("same checks in tx entrypoint"),
        );
        tx.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vouchers")
            .fetch_one(&pending)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    // Verify the copied row actually represents a valid pending context, so the
    // negative tests above are not all passing because of broken test setup.
    let mut tx = pending.begin_with("BEGIN IMMEDIATE").await.unwrap();
    ledger::post_invoice_voucher_tx(
        &mut tx,
        &f.workspace,
        &fy,
        &draft.id,
        "2026-12-31",
        &correct(),
    )
    .await
    .expect("exact validated pending context control");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn historical_sql_fixture_matches_archived_pre_gate_shapes_without_new_provenance() {
    let archive: Value = serde_json::from_str(include_str!(
        "../../fixtures/historical-regressions/pre-gate-invoice-shapes.json"
    ))
    .unwrap();
    assert_eq!(archive["sourceCommit"], "44f897e");
    assert_eq!(
        archive["classification"],
        "engineering_regression_not_independent_validation"
    );
    for case in archive["cases"].as_array().unwrap() {
        let f = Fixture::new().await;
        let status = if case["id"] == "exempt" {
            "exempt_low_turnover"
        } else {
            "registered"
        };
        sqlx::query("UPDATE vat_profiles SET vat_status=?1,reporting_period='quarterly',vat_filing_deadline_regime=NULL").bind(status).execute(&f.pool).await.unwrap();
        let draft = f
            .draft(
                case["vatRates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| {
                        line(
                            1,
                            case["unitPriceMinorPerLine"].as_i64().unwrap(),
                            r.as_f64().unwrap(),
                        )
                    })
                    .collect(),
            )
            .await;
        let mut input = f.input(&draft.id);
        input.eligibility_facts = None;
        input.issue_date = Some("2026-02-01".into());
        let invoice = common::seed_historical_invoice(&f.pool, &f.workspace, &input)
            .await
            .unwrap();
        assert_eq!(
            invoice.invoice_number.as_deref(),
            case["invoiceNumber"].as_str()
        );
        assert_eq!(invoice.issue_date.as_deref(), case["issueDate"].as_str());
        assert_eq!(invoice.total_ex_vat_minor, case["netMinor"]);
        assert_eq!(invoice.total_vat_minor, case["vatMinor"]);
        assert_eq!(invoice.total_inc_vat_minor, case["grossMinor"]);
        let journal:Vec<(String,i64,i64,Option<String>)>=sqlx::query_as("SELECT a.number,j.debit_minor,j.credit_minor,j.vat_code FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE j.voucher_id=?1 ORDER BY a.number,j.vat_code").bind(invoice.voucher_id.as_deref().unwrap()).fetch_all(&f.pool).await.unwrap();
        assert_eq!(serde_json::to_value(journal).unwrap(), case["journal"]);
        let jobs: Vec<(String, String)> = sqlx::query_as(
            "SELECT job_type,status FROM local_jobs WHERE workspace_id=?1 ORDER BY job_type",
        )
        .bind(&f.workspace)
        .fetch_all(&f.pool)
        .await
        .unwrap();
        assert_eq!(serde_json::to_value(jobs).unwrap(), case["jobs"]);
        let snapshot:(String,i64,String)=sqlx::query_as("SELECT rule_version_id,tax_year,vat_status FROM invoice_issue_snapshots WHERE invoice_id=?1").bind(&invoice.id).fetch_one(&f.pool).await.unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), case["snapshot"]);
        let audit:Vec<String>=sqlx::query_scalar("SELECT action FROM audit_events WHERE resource_id IN (?1,?2) AND action IN ('invoice_issue','voucher_post') ORDER BY action").bind(&invoice.id).bind(invoice.voucher_id.as_deref()).fetch_all(&f.pool).await.unwrap();
        assert_eq!(serde_json::to_value(audit).unwrap(), case["audit"]);
        let provenance: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoice_issue_validations")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(provenance, 0);
    }
}

#[tokio::test]
async fn voluntary_registration_effective_by_completion_is_covered_but_not_future_registration() {
    for (effective, allowed) in [
        ("2026-01-01", true),
        ("2026-12-30", true),
        ("2026-12-31", false),
    ] {
        let f = Fixture::new().await;
        let draft = f.draft(vec![line(1, 100_000, 0.25)]).await;
        sqlx::query("UPDATE vat_profiles SET vat_status='voluntary_registered',voluntary_registration_date=?1").bind(effective).execute(&f.pool).await.unwrap();
        let mut input = f.input(&draft.id);
        input.eligibility_facts = Some(common::facts("2026-12-30"));
        if allowed {
            let reviewed = f.review(input).await;
            f.issue(&reviewed).await.unwrap();
        } else {
            f.rejects_both(&input).await;
        }
    }
}
