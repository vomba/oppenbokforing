//! Owned fresh-business fixture. Business state is created only by public services.
//! The sole setup INSERT creates the workspace container (existing test convention).

use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    invoicing::{
        self,
        cash::{
            self, CashBusinessOriginFacts, CashIssueFacts, CashIssueInput, CashIssuePreflightInput,
        },
        InvoiceCreateDraftInput, InvoiceLineInput, InvoiceSummary,
    },
    profiles::{
        self, BusinessProfileSaveInput, OnboardingProfileSaveInput, TaxProfileSaveInput,
        VatProfileSaveInput,
    },
    workspace::{bootstrap_accounting_defaults, ensure_workspace_ready},
};
use sqlx::SqlitePool;
use tempfile::{tempdir, TempDir};
use uuid::Uuid;

pub const CASH_DIGEST: &str = "08d21e5dc4a055698b6a180e665141770bdb7f2c7325eea4e6d5bee4d54e7f7b";
pub const VALIDATION: &str = "cash-recognition-se-service25-2026-pdf-full-v1";
pub const RULE: &str = "rv-2026-method-recognition-v2";

pub struct CashFixture {
    // Keep the retained document archive alive for the entire test, including replay.
    pub dir: TempDir,
    pub pool: SqlitePool,
    pub workspace: String,
    pub issued: InvoiceSummary,
}

pub async fn assert_no_recognition(pool: &SqlitePool, workspace: &str) {
    let vouchers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vouchers WHERE workspace_id=?1")
        .bind(workspace)
        .fetch_one(pool)
        .await
        .unwrap();
    let lines: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines j JOIN vouchers v ON v.id=j.voucher_id WHERE v.workspace_id=?1")
        .bind(workspace).fetch_one(pool).await.unwrap();
    assert_eq!(vouchers, 0, "cash issuance must not post a voucher");
    assert_eq!(lines, 0, "cash issuance must not post any journal lines");
}

impl CashFixture {
    pub async fn issued() -> Self {
        Self::issued_dates("2026-01-01", "2026-01-10", "2026-01-15").await
    }
    pub async fn issued_dates(start: &str, completed: &str, issue: &str) -> Self {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("documents")).unwrap();
        std::fs::create_dir_all(dir.path().join("exports")).unwrap();
        let database = dir.path().join("workspace.sqlite");
        let pool = connect_workspace(&database)
            .await
            .expect("fresh migrated database");
        let workspace = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES(?1,'Synthetic cash receipt regression',?2,?3,?4)")
            .bind(&workspace).bind(database.to_string_lossy().as_ref())
            .bind(dir.path().join("documents").to_string_lossy().as_ref())
            .bind(dir.path().join("exports").to_string_lossy().as_ref())
            .execute(&pool).await.expect("workspace container");
        ensure_workspace_ready(&pool, &workspace).await.unwrap();
        // Explicit event year, not a clock bypass or historical business backfill.
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
        .expect("public onboarding: new registered annual cash/F-skatt SE business");
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
        .expect("public cash draft");
        assert_eq!(draft.status, "draft");
        assert_eq!(
            (
                draft.total_ex_vat_minor,
                draft.total_vat_minor,
                draft.total_inc_vat_minor
            ),
            (1_000_000, 250_000, 1_250_000)
        );
        assert_no_recognition(&pool, &workspace).await;
        let issues: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE workspace_id=?1 AND status!='draft'",
        )
        .bind(&workspace)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(issues, 0, "actual issuance history starts empty");
        let facts = CashIssueFacts {
            ordinary_domestic_service: Some(true),
            service_completed_on: Some(completed.into()),
            prior_payment_received: Some(false),
            no_eu_trade_in_year: Some(true),
            annual_vat_registration_confirmed: Some(true),
            expected_taxable_base_within_limit: Some(true),
            expected_net_turnover_within_cash_limit: Some(true),
            ordinary_timely_sending_confirmed: Some(true),
            registered_bokslutsmetoden_confirmed: Some(true),
            cash_treatment_consistent_confirmed: Some(true),
            // Not applicable to this full twelve-month first year (not an unknown eligibility fact).
            short_year_turnover_adjustment_reviewed: if start == "2026-01-01" {
                None
            } else {
                Some(true)
            },
        };
        let origin = CashBusinessOriginFacts {
            new_business_started_in_2026: Some(true),
            business_started_on: Some(start.into()),
            first_financial_year_ends_on: Some("2026-12-31".into()),
            zero_opening_balances_confirmed: Some(true),
            no_opening_unpaid_invoices_or_bills_confirmed: Some(true),
            no_prior_unrecorded_activity_confirmed: Some(true),
            outside_workspace_history_reviewed: Some(true),
        };
        println!("SETUP: fresh workspace; public annual registered cash/F-skatt SE onboarding; explicit zero-opening/no-unpaid/no-prior/outside-history origin {start}-2026-12-31; completed {completed}; draft 1000000/250000/1250000 ore; zero journal");
        let preflight = cash::invoice_cash_issue_preflight(
            &pool,
            &workspace,
            &CashIssuePreflightInput {
                invoice_id: draft.id.clone(),
                issue_date: issue.into(),
                facts: facts.clone(),
                origin_review: Some(origin.clone()),
            },
        )
        .await
        .expect("public qualifying cash preflight");
        assert_eq!(preflight.assessment.validation_id, VALIDATION);
        assert_eq!(preflight.assessment.evidence_digest, CASH_DIGEST);
        assert_eq!(preflight.assessment.rule_version_id, RULE);
        assert_eq!(preflight.assessment.accounting_date, None);
        assert_eq!(preflight.assessment.completed_on, completed);
        assert_eq!(preflight.assessment.issued_on, issue);
        assert_no_recognition(&pool, &workspace).await;
        let issued = cash::invoice_cash_issue(
            &pool,
            &workspace,
            &CashIssueInput {
                invoice_id: draft.id,
                issue_date: issue.into(),
                facts,
                origin_review: Some(origin),
                idempotency_key: "receipt-fixture-public-issue".into(),
                review_fingerprint: preflight.assessment.review_fingerprint,
            },
        )
        .await
        .expect("public identity-only cash issue");
        assert_eq!(issued.status, "issued");
        assert_eq!(issued.invoice_number.as_deref(), Some("2026-0001"));
        assert_eq!(issued.issue_date.as_deref(), Some(issue));
        assert_eq!(issued.voucher_id, None);
        assert_eq!(issued.payment_voucher_id, None);
        assert!(
            issued.pdf_job_id.is_some(),
            "issued identity retains PDF job"
        );
        assert_no_recognition(&pool, &workspace).await;
        println!("ISSUED: public cash issue 2026-0001 on {issue}; immutable identity/PDF job; zero vouchers and zero journal lines; receipt not recorded");
        Self {
            dir,
            pool,
            workspace,
            issued,
        }
    }
}

/// A real, renderable one-page PDF container with simulated bank-entry text.
/// This is NOT bank authentication, OCR, a real bank receipt, or named-bank format
/// validation. It is independently authored external-style test evidence, never
/// generated by the application's invoice PDF job. Facts below are source literals,
/// not values copied from an invoice result or inferred from its gross total.
pub fn synthetic_bank_pdf() -> Vec<u8> {
    synthetic_bank_pdf_for("2026-01-20", "SYNTHETIC-TXN-20260120-001", "2026-0001")
}

pub fn synthetic_bank_pdf_for(received_on: &str, reference: &str, invoice_number: &str) -> Vec<u8> {
    let text = [
        "SYNTHETIC BANK RECEIPT - NOT A REAL BANK ARTIFACT",
        "TEST ONLY - simulated executed incoming credit, not a scheduled payment",
        "Actual received date: 2026-01-20",
        "Currency: SEK; actual received amount: 12500.00 SEK / 1250000 ore",
        "Payer: Domestic Buyer; receiving business: New Cash Firma",
        "Bank ID: SYNTHETIC-TEST-BANK",
        "Receiving account: SYNTHETIC-BUSINESS-ACCOUNT-001",
        "Transaction reference: SYNTHETIC-TXN-20260120-001",
        "Invoice reference supplied by payer: 2026-0001",
        "Entry locator: page 1 / ref SYNTHETIC-TXN-20260120-001",
        "Synthetic fixture: no bank authentication, OCR or named-bank validation",
    ];
    let mut stream = String::from("BT\n/F1 9 Tf\n40 790 Td\n16 TL\n");
    for line in text {
        let line = line
            .replace("2026-01-20", received_on)
            .replace("SYNTHETIC-TXN-20260120-001", reference)
            .replace("2026-0001", invoice_number);
        assert!(
            !line.contains(['(', ')', '\\']),
            "PDF literals need escaping"
        );
        stream.push_str(&format!("({line}) Tj\nT*\n"));
    }
    stream.push_str("ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    let mut pdf = b"%PDF-1.4\n% synthetic test evidence\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for (index, offset) in offsets.iter().enumerate() {
        assert!(pdf[*offset..].starts_with(format!("{} 0 obj\n", index + 1).as_bytes()));
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    pdf
}
