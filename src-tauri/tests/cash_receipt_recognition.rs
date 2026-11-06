//! First positive public cash full-PDF receipt regression. Stop-before-green tracer.
//! The source-reviewed ledger oracle is independent of implementation output.
//! The PDF is visibly SYNTHETIC: this tests retained bytes + explicit owner facts,
//! NOT bank authentication, OCR, real bank receipts or named-bank format validation.

#[path = "cash_receipt_recognition/fixture.rs"]
mod fixture;

use fixture::{synthetic_bank_pdf, CashFixture, CASH_DIGEST, RULE, VALIDATION};
use oppenbokforing_desktop_lib::{
    documents,
    invoicing::{
        self,
        cash::{
            self, BankReceiptIdentity, CashHistoryGetInput, CashReceiptFacts,
            CashReceiptRecordInput,
        },
    },
    reconciliation::ReconciliationMatchResult,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

async fn pdf_jobs(pool: &SqlitePool, workspace: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT json_object('id',id,'type',job_type,'status',status,'attempts',attempts,'payload',payload_json,'error',last_error,'created',created_at,'updated',updated_at,'key',idempotency_key) FROM local_jobs WHERE workspace_id=?1 AND job_type='invoice_pdf_generate' ORDER BY id")
        .bind(workspace).fetch_all(pool).await.unwrap()
}

async fn durable_counts(pool: &SqlitePool, workspace: &str) -> Vec<i64> {
    // Fixed internal table names only; these are observations, never eligibility SQL.
    let mut counts = Vec::new();
    for table in [
        "vouchers",
        "reconciliation_matches",
        "staged_transactions",
        "cash_receipt_recognitions",
        "cash_invoice_issues",
        "cash_business_origins",
        "invoice_issue_snapshots",
        "documents",
        "invoices",
        "local_jobs",
        "audit_events",
    ] {
        counts.push(
            sqlx::query_scalar::<_, i64>(&format!(
                "SELECT COUNT(*) FROM {table} WHERE workspace_id=?1"
            ))
            .bind(workspace)
            .fetch_one(pool)
            .await
            .unwrap(),
        );
    }
    counts.push(sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines j JOIN vouchers v ON v.id=j.voucher_id WHERE v.workspace_id=?1")
        .bind(workspace).fetch_one(pool).await.unwrap());
    counts
}

async fn assert_recognition(
    fixture: &CashFixture,
    input: &CashReceiptRecordInput,
    result: &ReconciliationMatchResult,
    document_sha256: &str,
) {
    let pool = &fixture.pool;
    let workspace = &fixture.workspace;
    let voucher_id = result
        .voucher_id
        .as_deref()
        .expect("receipt recognition voucher ID");
    assert!(!result.match_id.is_empty());
    let voucher_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM vouchers WHERE workspace_id=?1")
            .bind(workspace)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(voucher_count, 1, "one receipt voucher, no issue voucher");
    let voucher = sqlx::query(
        "SELECT status,accounting_date,document_id FROM vouchers WHERE workspace_id=?1 AND id=?2",
    )
    .bind(workspace)
    .bind(voucher_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(voucher.get::<String, _>("status"), "posted");
    assert_eq!(voucher.get::<String, _>("accounting_date"), "2026-01-20");
    assert_eq!(
        voucher.get::<Option<String>, _>("document_id").as_deref(),
        Some(input.document_id.as_str())
    );
    let lines = sqlx::query("SELECT a.number,j.debit_minor,j.credit_minor FROM journal_lines j JOIN accounts a ON a.id=j.account_id JOIN vouchers v ON v.id=j.voucher_id WHERE v.workspace_id=?1 AND v.id=?2 ORDER BY a.number")
        .bind(workspace).bind(voucher_id).fetch_all(pool).await.unwrap();
    let actual: Vec<(String, i64, i64)> = lines
        .into_iter()
        .map(|r| (r.get("number"), r.get("debit_minor"), r.get("credit_minor")))
        .collect();
    // Independently derived: 10000 SEK net * 25% = 2500 VAT; 12500 bank.
    assert_eq!(
        actual,
        vec![
            ("1930".into(), 1_250_000, 0),
            ("2611".into(), 0, 250_000),
            ("3041".into(), 0, 1_000_000),
        ]
    );
    assert_eq!(actual.iter().map(|l| l.1).sum::<i64>(), 1_250_000);
    assert_eq!(actual.iter().map(|l| l.2).sum::<i64>(), 1_250_000);
    let all_lines: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines j JOIN vouchers v ON v.id=j.voucher_id WHERE v.workspace_id=?1")
        .bind(workspace).fetch_one(pool).await.unwrap();
    assert_eq!(all_lines, 3);
    let receivables: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE a.workspace_id=?1 AND a.number='1510'")
        .bind(workspace).fetch_one(pool).await.unwrap();
    assert_eq!(
        receivables, 0,
        "never invent receivable recognition or clearance"
    );

    for table in [
        "reconciliation_matches",
        "staged_transactions",
        "cash_receipt_recognitions",
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE workspace_id=?1"
        ))
        .bind(workspace)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "exactly one persisted {table} row");
    }
    let proof = sqlx::query("SELECT r.invoice_id,r.origin_id,r.document_id,r.document_sha256,r.bank_identity,r.voucher_id,r.match_id,r.staged_transaction_id,r.idempotency_key,r.facts_json,r.assessment_json,r.context_json,r.request_json,r.result_json,m.invoice_id AS matched_invoice,m.voucher_id AS matched_voucher,m.staged_transaction_id AS matched_stage,s.transaction_date,s.amount_minor,s.status AS stage_status,s.csv_import_id FROM cash_receipt_recognitions r JOIN reconciliation_matches m ON m.workspace_id=r.workspace_id AND m.id=r.match_id JOIN staged_transactions s ON s.workspace_id=r.workspace_id AND s.id=r.staged_transaction_id WHERE r.workspace_id=?1 AND r.invoice_id=?2")
        .bind(workspace).bind(&input.invoice_id).fetch_one(pool).await.unwrap();
    assert_eq!(proof.get::<String, _>("invoice_id"), input.invoice_id);
    assert_eq!(proof.get::<String, _>("document_id"), input.document_id);
    assert_eq!(proof.get::<String, _>("document_sha256"), document_sha256);
    assert!(
        !proof.get::<String, _>("bank_identity").trim().is_empty(),
        "reserve actual bank/account/transaction identity"
    );
    assert_eq!(proof.get::<String, _>("voucher_id"), voucher_id);
    assert_eq!(proof.get::<String, _>("match_id"), result.match_id);
    assert_eq!(
        proof.get::<String, _>("idempotency_key"),
        input.idempotency_key
    );
    assert_eq!(proof.get::<String, _>("matched_invoice"), input.invoice_id);
    assert_eq!(proof.get::<String, _>("matched_voucher"), voucher_id);
    assert_eq!(
        proof.get::<String, _>("matched_stage"),
        proof.get::<String, _>("staged_transaction_id")
    );
    assert_eq!(proof.get::<String, _>("transaction_date"), "2026-01-20");
    assert_eq!(proof.get::<i64, _>("amount_minor"), 1_250_000);
    assert_eq!(proof.get::<String, _>("stage_status"), "matched");
    assert_eq!(
        proof.get::<Option<String>, _>("csv_import_id"),
        None,
        "PDF source, not a CSV row"
    );
    let stored_facts: Value = serde_json::from_str(&proof.get::<String, _>("facts_json")).unwrap();
    assert_eq!(stored_facts, serde_json::to_value(&input.facts).unwrap());
    assert_eq!(
        stored_facts["actualReceivedAmountMinor"], 1_250_000,
        "persist explicit actual source amount, not an inferred invoice total"
    );
    assert_eq!(stored_facts["actualReceivedOn"], "2026-01-20");
    assert_eq!(stored_facts["currency"], "SEK");
    let request: Value = serde_json::from_str(&proof.get::<String, _>("request_json")).unwrap();
    assert_eq!(request["invoiceId"], input.invoice_id);
    assert_eq!(request["documentId"], input.document_id);
    assert_eq!(request["facts"], stored_facts);
    let context: Value = serde_json::from_str(&proof.get::<String, _>("context_json")).unwrap();
    assert!(
        context.as_object().is_some_and(|o| !o.is_empty()),
        "retain immutable receipt context"
    );
    let saved_result: Value = serde_json::from_str(&proof.get::<String, _>("result_json")).unwrap();
    assert_eq!(saved_result, serde_json::to_value(result).unwrap());

    let history = cash::invoice_cash_history_get(
        pool,
        workspace,
        &CashHistoryGetInput {
            invoice_id: input.invoice_id.clone(),
        },
    )
    .await
    .unwrap()
    .expect("public immutable cash history");
    assert_eq!(
        history.issue.assessment.accounting_date, None,
        "issue remains identity only"
    );
    assert_eq!(history.issue.assessment.issued_on, "2026-01-15");
    let receipt = history
        .receipt
        .expect("receipt provenance distinct from issue");
    assert_eq!(receipt.invoice_id, input.invoice_id);
    assert_eq!(receipt.assessment.accounting_date, "2026-01-20");
    assert_eq!(receipt.assessment.accounting_year, 2026);
    assert_eq!(receipt.assessment.validation_id, VALIDATION);
    assert_eq!(receipt.assessment.evidence_digest, CASH_DIGEST);
    assert_eq!(receipt.assessment.rule_version_id, RULE);
    assert_eq!(receipt.assessment.origin_id, history.origin.id);
    assert_eq!(
        receipt.assessment.origin_id,
        proof.get::<String, _>("origin_id")
    );
    assert_eq!(receipt.assessment.document_id, input.document_id);
    assert_eq!(receipt.assessment.document_sha256, document_sha256);
    assert_eq!(receipt.assessment.period_key, "2026");
    assert_eq!(receipt.assessment.due_on, "2027-05-12");
    assert!(!receipt.assessment.review_fingerprint.is_empty());
    assert_eq!(serde_json::to_value(&receipt.facts).unwrap(), stored_facts);
    assert_eq!(serde_json::to_value(&receipt.result).unwrap(), saved_result);
    let stored_assessment: Value =
        serde_json::from_str(&proof.get::<String, _>("assessment_json")).unwrap();
    assert_eq!(
        stored_assessment,
        serde_json::to_value(&receipt.assessment).unwrap()
    );
}

#[tokio::test]
async fn full_retained_pdf_receipt_recognizes_cash_once_on_actual_received_date() {
    let evidence = include_bytes!("../../fixtures/accounting-validation/2026-cash-pdf-full.json");
    assert_eq!(format!("{:x}", Sha256::digest(evidence)), CASH_DIGEST);
    let oracle: Value = serde_json::from_slice(evidence).unwrap();
    assert_eq!(oracle["validationId"], VALIDATION);
    assert_eq!(oracle["example"]["netMinor"], 1_000_000);
    assert_eq!(oracle["example"]["vatMinor"], 250_000);
    assert_eq!(oracle["example"]["grossMinor"], 1_250_000);
    assert_eq!(
        oracle["recognition"]["cashFullPdfReceipt"]["accountingDate"],
        "2026-01-20"
    );
    assert_eq!(
        oracle["recognition"]["cashFullPdfReceipt"]["journalLines"],
        json!([
            {"account":"1930", "debitMinor":1250000, "creditMinor":0},
            {"account":"3041", "debitMinor":0, "creditMinor":1000000},
            {"account":"2611", "debitMinor":0, "creditMinor":250000},
        ])
    );
    println!("ORACLE: reviewed cash digest {CASH_DIGEST}; independent net/VAT/gross 1000000/250000/1250000 ore; receipt 2026-01-20, not service/issue date");
    let fixture = CashFixture::issued().await;
    let pool = &fixture.pool;
    let workspace = &fixture.workspace;
    let original = &fixture.issued;
    let snapshot_before = serde_json::to_value(
        invoicing::get_issued_invoice_snapshot(pool, workspace, &original.id)
            .await
            .unwrap(),
    )
    .unwrap();
    let jobs_before = pdf_jobs(pool, workspace).await;
    assert_eq!(jobs_before.len(), 1);
    let history_before = cash::invoice_cash_history_get(
        pool,
        workspace,
        &CashHistoryGetInput {
            invoice_id: original.id.clone(),
        },
    )
    .await
    .unwrap()
    .expect("issued history before receipt");
    assert_eq!(history_before.issue.assessment.accounting_date, None);
    assert!(history_before.receipt.is_none());

    let pdf_bytes = synthetic_bank_pdf();
    let document = documents::store_document_bytes(
        pool,
        workspace,
        &pdf_bytes,
        "SYNTHETIC-NOT-REAL-bank-receipt-2026-01-20.pdf",
        "application/pdf",
    )
    .await
    .expect("retain actual synthetic external-style PDF bytes via public document service");
    assert_eq!(
        document.content_sha256,
        format!("{:x}", Sha256::digest(&pdf_bytes))
    );
    assert_eq!(document.mime_type, "application/pdf");
    let retained_path = fixture
        .dir
        .path()
        .join("documents")
        .join(&document.object_path);
    assert_eq!(std::fs::read(&retained_path).unwrap(), pdf_bytes);
    documents::verify_retained_document(pool, workspace, &document.id)
        .await
        .expect("retained source integrity");
    let generated_document: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoices WHERE workspace_id=?1 AND pdf_document_id=?2",
    )
    .bind(workspace)
    .bind(&document.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        generated_document, 0,
        "source is not a generated invoice PDF"
    );
    // Nine explicit facts, independently transcribed from the simulated bank source.
    // In particular, do NOT populate received amount from original.total_inc_vat_minor.
    let input = CashReceiptRecordInput {
        invoice_id: original.id.clone(),
        document_id: document.id.clone(),
        idempotency_key: "first-full-synthetic-pdf-receipt".into(),
        facts: CashReceiptFacts {
            actual_received_amount_minor: Some(1_250_000),
            actual_received_on: Some("2026-01-20".into()),
            currency: Some("SEK".into()),
            received_already: Some(true),
            bank_receipt_identity: Some(BankReceiptIdentity {
                bank_id: "SYNTHETIC-TEST-BANK".into(),
                receiving_account_reference: "SYNTHETIC-BUSINESS-ACCOUNT-001".into(),
                transaction_reference: "SYNTHETIC-TXN-20260120-001".into(),
            }),
            source_entry_locator: Some("page 1 / ref SYNTHETIC-TXN-20260120-001".into()),
            bank_pdf_matches_receipt_confirmed: Some(true),
            issued_case_still_applies_confirmed: Some(true),
            no_other_receipts_or_adjustments_confirmed: Some(true),
        },
    };
    fixture::assert_no_recognition(pool, workspace).await;
    println!("SOURCE: retained well-formed one-page synthetic PDF with Catalog/Pages/Page/content/font/xref/trailer/EOF; visible NOT A REAL BANK ARTIFACT; simulated executed SEK 1250000 ore on Jan 20, payer/business/account/transaction/invoice reference; public integrity verified; not invoice-generated");
    println!("RECEIPT: calling PUBLIC invoice_cash_receipt_record with nine explicit actual-source facts; expected one Jan 20 voucher 1930 DR 1250000 / 3041 CR 1000000 / 2611 CR 250000; no 1510");
    let result = cash::invoice_cash_receipt_record(pool, workspace, &input)
        .await.expect("qualifying retained full PDF cash receipt must recognize bank/revenue/VAT on actual receipt date");
    assert_recognition(&fixture, &input, &result, &document.content_sha256).await;

    let after = invoicing::get_invoice(pool, workspace, &original.id)
        .await
        .unwrap();
    assert_eq!(after.invoice_number, original.invoice_number);
    assert_eq!(after.issue_date, original.issue_date);
    assert_eq!(after.due_date, original.due_date);
    assert_eq!(after.counterparty_id, original.counterparty_id);
    assert_eq!(after.total_ex_vat_minor, 1_000_000);
    assert_eq!(after.total_vat_minor, 250_000);
    assert_eq!(after.total_inc_vat_minor, 1_250_000);
    assert_eq!(
        after.voucher_id, None,
        "original issue still has no recognition voucher"
    );
    assert_eq!(
        serde_json::to_value(&after.lines).unwrap(),
        serde_json::to_value(&original.lines).unwrap()
    );
    assert_eq!(after.pdf_job_id, original.pdf_job_id);
    assert_eq!(after.pdf_document_id, original.pdf_document_id);
    assert_eq!(
        serde_json::to_value(
            invoicing::get_issued_invoice_snapshot(pool, workspace, &original.id)
                .await
                .unwrap()
        )
        .unwrap(),
        snapshot_before
    );
    assert_eq!(
        pdf_jobs(pool, workspace).await,
        jobs_before,
        "receipt must not rewrite/regenerate issued PDF jobs"
    );
    let history_after = cash::invoice_cash_history_get(
        pool,
        workspace,
        &CashHistoryGetInput {
            invoice_id: original.id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        serde_json::to_value(&history_after.issue).unwrap(),
        serde_json::to_value(&history_before.issue).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&history_after.origin).unwrap(),
        serde_json::to_value(&history_before.origin).unwrap()
    );

    let counts_before_retry = durable_counts(pool, workspace).await;
    let retry = cash::invoice_cash_receipt_record(pool, workspace, &input)
        .await
        .expect("exact completed receipt recovery");
    assert_eq!(
        serde_json::to_value(&retry).unwrap(),
        serde_json::to_value(&result).unwrap()
    );
    assert_eq!(durable_counts(pool, workspace).await, counts_before_retry, "completed retry must not duplicate voucher/lines/match/staging/source/proof/replay/audit/PDF jobs");
    assert_recognition(&fixture, &input, &retry, &document.content_sha256).await;
    let history_retry = cash::invoice_cash_history_get(
        pool,
        workspace,
        &CashHistoryGetInput {
            invoice_id: original.id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        serde_json::to_value(history_retry).unwrap(),
        serde_json::to_value(history_after).unwrap()
    );
    assert_eq!(pdf_jobs(pool, workspace).await, jobs_before);
    assert_eq!(std::fs::read(retained_path).unwrap(), pdf_bytes);
}
