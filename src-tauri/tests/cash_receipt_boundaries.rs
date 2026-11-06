//! Public receipt boundaries against genuine public new-business issuance.
//! All PDF text is synthetic, never an external bank execution/authentication claim.
#[path = "cash_receipt_recognition/fixture.rs"]
mod fixture;
use fixture::{synthetic_bank_pdf, synthetic_bank_pdf_for, CashFixture};
use oppenbokforing_desktop_lib::{
    documents,
    error::AppError,
    invoicing::{
        self,
        cash::{self, *},
    },
};
use serde_json::{json, Value};
use sqlx::Row;

async fn input(f: &CashFixture) -> CashReceiptRecordInput {
    input_on(f, "2026-01-20").await
}
async fn input_on(f: &CashFixture, day: &str) -> CashReceiptRecordInput {
    let doc = documents::store_document_bytes(
        &f.pool,
        &f.workspace,
        &synthetic_bank_pdf_for(day, "SYNTHETIC-TXN-20260120-001", "2026-0001"),
        "SYNTHETIC.pdf",
        "application/pdf",
    )
    .await
    .unwrap();
    serde_json::from_value(json!({"invoiceId":f.issued.id,"documentId":doc.id,"idempotencyKey":"cash-receipt-boundary","facts":{
        "actualReceivedAmountMinor":1250000,"actualReceivedOn":day,"currency":"SEK","receivedAlready":true,
        "bankReceiptIdentity":{"bankId":"SYNTHETIC-TEST-BANK","receivingAccountReference":"SYNTHETIC-BUSINESS-ACCOUNT-001","transactionReference":"SYNTHETIC-TXN-20260120-001"},
        "sourceEntryLocator":"page 1 / ref SYNTHETIC-TXN-20260120-001","bankPdfMatchesReceiptConfirmed":true,"issuedCaseStillAppliesConfirmed":true,"noOtherReceiptsOrAdjustmentsConfirmed":true}})).unwrap()
}
async fn record(f: &CashFixture, i: &CashReceiptRecordInput) -> Result<Value, AppError> {
    cash::invoice_cash_receipt_record(&f.pool, &f.workspace, i)
        .await
        .map(|r| serde_json::to_value(r).unwrap())
}
async fn state(f: &CashFixture) -> Vec<String> {
    let mut result = Vec::new();
    for table in [
        "invoices",
        "invoice_lines",
        "invoice_sequences",
        "cash_invoice_issues",
        "cash_business_origins",
        "cash_receipt_recognitions",
        "vouchers",
        "journal_lines",
        "staged_transactions",
        "reconciliation_matches",
        "local_jobs",
        "audit_events",
        "documents",
    ] {
        let rows = sqlx::query(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .fetch_all(&f.pool)
            .await
            .unwrap();
        // All rows/columns as SQLite-quoted values: deterministic including nulls.
        let columns = if let Some(row) = rows.first() {
            use sqlx::Column;
            row.columns()
                .iter()
                .map(|c| format!("quote(\"{}\")", c.name()))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if !columns.is_empty() {
            let rows: Vec<String> = sqlx::query_scalar(&format!(
                "SELECT {} FROM {table} ORDER BY rowid",
                columns.join("||'|'||")
            ))
            .fetch_all(&f.pool)
            .await
            .unwrap();
            result.extend(rows.into_iter().map(|r| format!("{table}:{r}")));
        }
    }
    result
}
async fn blocked_unchanged(f: &CashFixture, i: &CashReceiptRecordInput) {
    let before = state(f).await;
    assert!(
        record(f, i).await.is_err(),
        "unsupported input succeeded: {}",
        serde_json::to_string(i).unwrap()
    );
    assert_eq!(
        state(f).await,
        before,
        "failure leaked posting/reservation/replay or changed issued identity"
    );
}

#[tokio::test]
async fn every_nullable_fact_missing_false_and_non_full_amount_is_closed() {
    let f = CashFixture::issued().await;
    let valid = input(&f).await;
    let v = serde_json::to_value(&valid).unwrap();
    for field in v["facts"].as_object().unwrap().keys() {
        for omit in [false, true] {
            let mut candidate = v.clone();
            if omit {
                candidate["facts"].as_object_mut().unwrap().remove(field);
            } else {
                candidate["facts"][field] = Value::Null;
            }
            blocked_unchanged(&f, &serde_json::from_value(candidate).unwrap()).await;
        }
    }
    for field in [
        "receivedAlready",
        "bankPdfMatchesReceiptConfirmed",
        "issuedCaseStillAppliesConfirmed",
        "noOtherReceiptsOrAdjustmentsConfirmed",
    ] {
        let mut candidate = v.clone();
        candidate["facts"][field] = json!(false);
        blocked_unchanged(&f, &serde_json::from_value(candidate).unwrap()).await;
    }
    for amount in [1249999, 1250001, 0, -1, i64::MAX] {
        let mut candidate = valid.clone();
        candidate.facts.actual_received_amount_minor = Some(amount);
        blocked_unchanged(&f, &candidate).await;
    }
    for currency in ["", "EUR", "sek", " SEK ", "unknown"] {
        let mut candidate = valid.clone();
        candidate.facts.currency = Some(currency.into());
        blocked_unchanged(&f, &candidate).await;
    }
    for locator in ["", "  ", "page\n1"] {
        let mut candidate = valid.clone();
        candidate.facts.source_entry_locator = Some(locator.into());
        blocked_unchanged(&f, &candidate).await;
    }
    assert!(
        record(&f, &valid).await.is_ok(),
        "negative attempts cannot poison later valid receipt"
    );
}

#[tokio::test]
async fn dates_and_ambiguous_bank_identity_are_not_inferred() {
    let f = CashFixture::issued().await;
    let valid = input(&f).await;
    for day in [
        "",
        "2026-1-20",
        "2026-02-30",
        "2025-12-31",
        "2027-01-01",
        "2026-01-14",
        "2026-12-31",
    ] {
        // Last date is only a runtime negative when it is actually future.
        if day == "2026-12-31"
            && chrono::Utc::now().date_naive()
                >= chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()
        {
            continue;
        }
        let mut i = valid.clone();
        i.facts.actual_received_on = Some(day.into());
        blocked_unchanged(&f, &i).await;
    }
    for part in [
        "bankId",
        "receivingAccountReference",
        "transactionReference",
    ] {
        for raw in [
            "",
            " ",
            "unknown",
            "n/a",
            "bank\nentry",
            "Référence",
            "ambiguous  reference",
        ] {
            let mut i = serde_json::to_value(&valid).unwrap();
            i["facts"]["bankReceiptIdentity"][part] = json!(raw);
            blocked_unchanged(&f, &serde_json::from_value(i).unwrap()).await;
        }
    }
}

#[tokio::test]
async fn malformed_header_only_and_dangling_content_pdfs_are_not_bank_evidence() {
    let f = CashFixture::issued().await;
    let mut i = input(&f).await;
    let dangling = String::from_utf8(synthetic_bank_pdf())
        .unwrap()
        .replace("/Contents 4 0 R", "/Contents 9 0 R")
        .into_bytes();
    for bytes in [
        b"%PDF-1.4\n%%EOF\n".to_vec(),
        b"just mime metadata".to_vec(),
        dangling,
    ] {
        let doc = documents::store_document_bytes(
            &f.pool,
            &f.workspace,
            &bytes,
            "fake.pdf",
            "application/pdf",
        )
        .await;
        if let Ok(doc) = doc {
            i.document_id = doc.id;
            blocked_unchanged(&f, &i).await;
        }
    }
}

#[tokio::test]
async fn retained_missing_tampered_foreign_and_metadata_only_sources_stop_atomically() {
    let f = CashFixture::issued().await;
    let valid = input(&f).await;
    let doc = documents::document_get(&f.pool, &f.workspace, &valid.document_id)
        .await
        .unwrap();
    let path = f.dir.path().join("documents").join(&doc.object_path);
    std::fs::write(&path, b"tampered").unwrap();
    blocked_unchanged(&f, &valid).await;
    std::fs::remove_file(&path).unwrap();
    blocked_unchanged(&f, &valid).await;
    let mut missing = valid.clone();
    missing.document_id = "metadata-only-or-missing".into();
    blocked_unchanged(&f, &missing).await;
    let foreign = CashFixture::issued().await;
    let foreign_input = input(&foreign).await;
    missing.document_id = foreign_input.document_id;
    blocked_unchanged(&f, &missing).await;
}

#[tokio::test]
async fn receipt_period_not_issue_period_and_actual_first_year_are_enforced() {
    let f = CashFixture::issued().await;
    let i = input_on(&f, "2026-02-20").await;
    sqlx::query("INSERT INTO fiscal_periods(id,workspace_id,fiscal_year_id,period_key,starts_on,ends_on,status) SELECT 'locked-receipt-period',workspace_id,id,'2026-02','2026-02-01','2026-02-28','locked' FROM fiscal_years WHERE workspace_id=?1 AND ends_on='2026-12-31'").bind(&f.workspace).execute(&f.pool).await.unwrap();
    blocked_unchanged(&f, &i).await;
    sqlx::query("UPDATE fiscal_periods SET status='open' WHERE workspace_id=?1")
        .bind(&f.workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO fiscal_periods(id,workspace_id,fiscal_year_id,period_key,starts_on,ends_on,status) SELECT 'locked-issue-period',workspace_id,id,'2026-01','2026-01-01','2026-01-31','locked' FROM fiscal_years WHERE workspace_id=?1 AND ends_on='2026-12-31'").bind(&f.workspace).execute(&f.pool).await.unwrap();
    assert!(
        record(&f, &i).await.is_ok(),
        "issue-period lock cannot replace receipt-period check"
    );
    let short = CashFixture::issued_dates("2026-06-01", "2026-06-10", "2026-06-15").await;
    let i = input_on(&short, "2026-06-20").await;
    assert!(
        record(&short, &i).await.is_ok(),
        "December-ending short first year is included"
    );
}

#[tokio::test]
async fn current_posture_policy_snapshot_and_year_changes_stop_new_receipt() {
    let edits=[
        "UPDATE sole_trader_profiles SET residency_country='NO' WHERE workspace_id=?1",
        "UPDATE sole_trader_profiles SET business_name='Other business' WHERE workspace_id=?1",
        "UPDATE tax_profiles SET tax_status='planning' WHERE workspace_id=?1",
        "UPDATE tax_profiles SET tax_status='fa_skatt',expected_salary_income_minor=0 WHERE workspace_id=?1",
        "UPDATE vat_profiles SET accounting_method='invoice_method' WHERE workspace_id=?1",
        "UPDATE vat_profiles SET reporting_period='quarterly' WHERE workspace_id=?1",
        "UPDATE vat_profiles SET vat_status='voluntary_registered',voluntary_registration_date='2026-01-11' WHERE workspace_id=?1",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.scope.annualTaxableBaseLimitMinor',200000000) WHERE family='cash_recognition' AND rule_version_id='rv-2026-method-recognition-v2'",
        "UPDATE rule_versions SET status='retired' WHERE id='rv-2026-method-recognition-v2'",
        "UPDATE fiscal_years SET status='closing' WHERE workspace_id=?1 AND ends_on='2026-12-31'",
        "UPDATE fiscal_years SET starts_on='2026-01-02' WHERE workspace_id=?1 AND ends_on='2026-12-31'",
        "UPDATE fiscal_years SET ends_on='2026-11-30' WHERE workspace_id=?1 AND ends_on='2026-12-31'",
        "UPDATE invoices SET due_date='2026-09-01' WHERE workspace_id=?1",
        "UPDATE invoices SET total_inc_vat_minor=1250001 WHERE workspace_id=?1",
        "UPDATE invoice_lines SET description='Changed service' WHERE invoice_id IN(SELECT id FROM invoices WHERE workspace_id=?1)",
        "UPDATE invoice_lines SET unit_price_minor=1000004 WHERE invoice_id IN(SELECT id FROM invoices WHERE workspace_id=?1)",
        "UPDATE invoice_lines SET quantity=9223372036854775807 WHERE invoice_id IN(SELECT id FROM invoices WHERE workspace_id=?1)",
        "UPDATE invoice_lines SET unit_price_minor=6000000000000000000 WHERE invoice_id IN(SELECT id FROM invoices WHERE workspace_id=?1)",
    ];
    for edit in edits {
        let f = CashFixture::issued().await;
        let i = input(&f).await;
        sqlx::query(edit)
            .bind(&f.workspace)
            .execute(&f.pool)
            .await
            .unwrap();
        blocked_unchanged(&f, &i).await;
    }
    let f = CashFixture::issued().await;
    let i = input(&f).await;
    sqlx::query("DROP TRIGGER invoice_issue_snapshots_immutable_update")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE invoice_issue_snapshots SET business_name='Bad original snapshot' WHERE workspace_id=?1").bind(&f.workspace).execute(&f.pool).await.unwrap();
    blocked_unchanged(&f, &i).await;
}

#[tokio::test]
async fn replay_exact_key_only_precedes_current_guards_and_missing_bytes() {
    let f = CashFixture::issued().await;
    let i = input(&f).await;
    let result = record(&f, &i).await.unwrap();
    let mut new_key = i.clone();
    new_key.idempotency_key = "new-key-not-replay".into();
    blocked_unchanged(&f, &new_key).await;
    let v = serde_json::to_value(&i).unwrap();
    for (field, changed) in [
        ("actualReceivedAmountMinor", json!(1249999)),
        ("actualReceivedOn", json!("2026-01-21")),
        ("currency", json!("EUR")),
        ("receivedAlready", json!(false)),
        ("sourceEntryLocator", json!("other entry")),
        ("bankPdfMatchesReceiptConfirmed", json!(false)),
        ("issuedCaseStillAppliesConfirmed", json!(false)),
        ("noOtherReceiptsOrAdjustmentsConfirmed", json!(false)),
        (
            "bankReceiptIdentity",
            json!({"bankId":"other","receivingAccountReference":"other","transactionReference":"other"}),
        ),
    ] {
        let mut candidate = v.clone();
        candidate["facts"][field] = changed;
        blocked_unchanged(&f, &serde_json::from_value(candidate).unwrap()).await;
    }
    for field in ["invoiceId", "documentId"] {
        let mut candidate = v.clone();
        candidate[field] = json!("different");
        blocked_unchanged(&f, &serde_json::from_value(candidate).unwrap()).await;
    }
    let doc = documents::document_get(&f.pool, &f.workspace, &i.document_id)
        .await
        .unwrap();
    std::fs::remove_file(f.dir.path().join("documents").join(doc.object_path)).unwrap();
    sqlx::query("UPDATE vat_profiles SET accounting_method='invoice_method',reporting_period='quarterly' WHERE workspace_id=?1").bind(&f.workspace).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE fiscal_years SET status='closing' WHERE workspace_id=?1")
        .bind(&f.workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE rule_versions SET status='retired' WHERE id='rv-2026-method-recognition-v2'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    let before = state(&f).await;
    assert_eq!(record(&f, &i).await.unwrap(), result);
    let history = cash::invoice_cash_history_get(
        &f.pool,
        &f.workspace,
        &CashHistoryGetInput {
            invoice_id: i.invoice_id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert!(history
        .receipt
        .unwrap()
        .assessment
        .coverage_summary
        .contains("AT POSTING"));
    assert_eq!(
        state(&f).await,
        before,
        "readonly history/recovery must not mutate"
    );
    f.pool.close().await;
    let reopened =
        oppenbokforing_desktop_lib::db::connect_workspace(&f.dir.path().join("workspace.sqlite"))
            .await
            .unwrap();
    assert_eq!(
        serde_json::to_value(
            cash::invoice_cash_receipt_record(&reopened, &f.workspace, &i)
                .await
                .unwrap()
        )
        .unwrap(),
        result
    );
}

#[tokio::test]
async fn duplicate_races_new_keys_and_cross_operation_collisions_are_atomic() {
    for same_key in [false, true] {
        let f = CashFixture::issued().await;
        let i = input(&f).await;
        let mut other = i.clone();
        if !same_key {
            other.idempotency_key = "racing-new-key".into();
        }
        let (a, b) = tokio::join!(record(&f, &i), record(&f, &other));
        if same_key {
            assert_eq!(a.unwrap(), b.unwrap());
        } else {
            assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cash_receipt_recognitions")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(count, 3);
    }
    let f = CashFixture::issued().await;
    let mut i = input(&f).await;
    i.idempotency_key = "receipt-fixture-public-issue".into();
    blocked_unchanged(&f, &i).await;
}

#[tokio::test]
async fn write_failure_rolls_back_planned_proof_before_posting_and_can_retry() {
    for table in [
        "vouchers",
        "journal_lines",
        "staged_transactions",
        "reconciliation_matches",
        "local_jobs",
        "audit_events",
    ] {
        let f = CashFixture::issued().await;
        let i = input(&f).await;
        sqlx::query(&format!("CREATE TRIGGER fail_receipt BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'injected receipt failure'); END")).execute(&f.pool).await.unwrap();
        blocked_unchanged(&f, &i).await;
        sqlx::query("DROP TRIGGER fail_receipt")
            .execute(&f.pool)
            .await
            .unwrap();
        assert!(
            record(&f, &i).await.is_ok(),
            "reservation rolled back after {table} failure"
        );
    }
}

async fn second_issue(f: &CashFixture) -> String {
    second_issue_amount(f, 1_000_000).await
}
async fn second_issue_amount(f: &CashFixture, net: i64) -> String {
    let draft = invoicing::create_draft(
        &f.pool,
        &f.workspace,
        &invoicing::InvoiceCreateDraftInput {
            counterparty_id: f.issued.counterparty_id.clone(),
            due_date: Some("2026-02-28".into()),
            lines: vec![invoicing::InvoiceLineInput {
                description: "Second completed domestic service".into(),
                quantity: 1,
                unit_price_minor: net,
                vat_rate: 0.25,
                account_number: Some("3041".into()),
            }],
        },
    )
    .await
    .unwrap();
    let h = cash::invoice_cash_history_get(
        &f.pool,
        &f.workspace,
        &CashHistoryGetInput {
            invoice_id: f.issued.id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    let p = cash::invoice_cash_issue_preflight(
        &f.pool,
        &f.workspace,
        &CashIssuePreflightInput {
            invoice_id: draft.id.clone(),
            issue_date: "2026-01-16".into(),
            facts: h.issue.facts.clone(),
            origin_review: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        p.current_taxable_base_minor, 1_000_000,
        "paid/unpaid cash issues count exactly once"
    );
    cash::invoice_cash_issue(
        &f.pool,
        &f.workspace,
        &CashIssueInput {
            invoice_id: draft.id.clone(),
            issue_date: "2026-01-16".into(),
            facts: h.issue.facts,
            origin_review: None,
            idempotency_key: "second-public-issue".into(),
            review_fingerprint: p.assessment.review_fingerprint,
        },
    )
    .await
    .unwrap();
    draft.id
}

#[tokio::test]
async fn distinct_receipt_continuation_counts_paid_and_unpaid_once_but_reserves_pdf_and_bank_tuple()
{
    for paid_first in [false, true] {
        let f = CashFixture::issued().await;
        let first = input(&f).await;
        if paid_first {
            record(&f, &first).await.unwrap();
        }
        let second = second_issue(&f).await;
        if !paid_first {
            record(&f, &first).await.unwrap();
        }
        let mut i = first.clone();
        i.invoice_id = second;
        i.idempotency_key = "second-receipt".into();
        i.facts
            .bank_receipt_identity
            .as_mut()
            .unwrap()
            .transaction_reference = "SECOND-SYNTHETIC-TXN".into();
        blocked_unchanged(&f, &i).await; // different true entry, same PDF is still reserved
        let mut bytes = synthetic_bank_pdf();
        bytes.extend_from_slice(b"\n% distinct synthetic PDF wrapper\n");
        let doc = documents::store_document_bytes(
            &f.pool,
            &f.workspace,
            &bytes,
            "different.pdf",
            "application/pdf",
        )
        .await
        .unwrap();
        i.document_id = doc.id;
        i.facts.bank_receipt_identity = first.facts.bank_receipt_identity.clone();
        blocked_unchanged(&f, &i).await; // tuple reserved across wrappers
        i.facts.bank_receipt_identity.as_mut().unwrap().bank_id = "  synthetic-test-bank  ".into();
        blocked_unchanged(&f, &i).await; // normalized duplicate
        i.facts
            .bank_receipt_identity
            .as_mut()
            .unwrap()
            .transaction_reference = "SECOND-SYNTHETIC-TXN".into();
        i.facts.source_entry_locator = Some("page 1 / ref SECOND-SYNTHETIC-TXN".into());
        let distinct = documents::store_document_bytes(
            &f.pool,
            &f.workspace,
            &synthetic_bank_pdf_for("2026-01-20", "SECOND-SYNTHETIC-TXN", "2026-0002"),
            "second-SYNTHETIC.pdf",
            "application/pdf",
        )
        .await
        .unwrap();
        i.document_id = distinct.id;
        record(&f, &i).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vouchers")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE a.number='1510'").fetch_one(&f.pool).await.unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn history_validates_durable_receipt_instead_of_blind_json_decode() {
    for edit in [
        "UPDATE staged_transactions SET amount_minor=1250001",
        "UPDATE staged_transactions SET transaction_date='2026-01-21'",
        "UPDATE journal_lines SET credit_minor=249999 WHERE credit_minor=250000",
        "UPDATE vouchers SET accounting_date='2026-01-15'",
        "UPDATE cash_receipt_recognitions SET facts_json=json_set(facts_json,'$.actualReceivedAmountMinor',1)",
        "UPDATE cash_receipt_recognitions SET context_json=json_set(context_json,'$.issueFingerprint','bad')",
        "UPDATE cash_receipt_recognitions SET assessment_json=json_set(assessment_json,'$.evidenceDigest','bad')",
        "UPDATE local_jobs SET payload_json='{}' WHERE job_type='invoice_cash_receipt_record'",
        "DELETE FROM audit_events WHERE action='invoice_cash_receipt_record'",
    ] {
        let f=CashFixture::issued().await;let i=input(&f).await;record(&f,&i).await.unwrap();
        // Offline corruption simulation only, never used to make a business eligible.
        sqlx::query("DROP TRIGGER cash_receipt_recognitions_immutable_update").execute(&f.pool).await.unwrap();
        sqlx::query("DROP TRIGGER journal_lines_no_update_for_posted_voucher").execute(&f.pool).await.unwrap();
        sqlx::query("DROP TRIGGER vouchers_no_update_when_posted").execute(&f.pool).await.unwrap();
        sqlx::query("DROP TRIGGER audit_events_no_delete").execute(&f.pool).await.unwrap();
        sqlx::query(edit).execute(&f.pool).await.unwrap();let before=state(&f).await;
        assert!(cash::invoice_cash_history_get(&f.pool,&f.workspace,&CashHistoryGetInput{invoice_id:i.invoice_id.clone()}).await.is_err(),"{edit}");
        assert!(record(&f,&i).await.is_err(),"corrupt replay: {edit}");assert_eq!(state(&f).await,before);
    }
}

#[tokio::test]
async fn approved_receipt_year_blocks_even_if_external_edit_left_year_open() {
    let f = CashFixture::issued().await;
    let i = input(&f).await;
    sqlx::query("INSERT INTO year_end_packages(id,workspace_id,fiscal_year_id,status,rule_version_id) SELECT 'externally-approved',workspace_id,id,'approved','rv-2026-method-recognition-v2' FROM fiscal_years WHERE workspace_id=?1 AND ends_on='2026-12-31'").bind(&f.workspace).execute(&f.pool).await.unwrap();
    blocked_unchanged(&f, &i).await;
}

#[tokio::test]
async fn generated_invoice_pdf_current_old_job_hash_and_metadata_only_are_rejected() {
    use oppenbokforing_desktop_lib::jobs;
    let f = CashFixture::issued().await;
    let valid = input(&f).await;
    assert_eq!(
        jobs::process_pending_invoice_pdf_jobs(&f.pool, &f.workspace)
            .await
            .unwrap(),
        1
    );
    let issued = invoicing::get_invoice(&f.pool, &f.workspace, &f.issued.id)
        .await
        .unwrap();
    let original = issued.pdf_document_id.unwrap();
    let mut i = valid.clone();
    i.document_id = original.clone();
    blocked_unchanged(&f, &i).await;
    jobs::refresh_invoice_pdf(&f.pool, &f.workspace, &f.issued.id)
        .await
        .unwrap();
    jobs::process_pending_invoice_pdf_jobs(&f.pool, &f.workspace)
        .await
        .unwrap();
    // Detach the current pointer: old result/hash history, not just this pointer,
    // must reject the generated PDF. This is adversarial evidence state only.
    sqlx::query("UPDATE invoices SET pdf_document_id=NULL WHERE workspace_id=?1")
        .bind(&f.workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    blocked_unchanged(&f, &i).await;
    let fake = documents::store_document_bytes(
        &f.pool,
        &f.workspace,
        b"plain metadata is not PDF bytes",
        "metadata.txt",
        "text/plain",
    )
    .await
    .unwrap();
    sqlx::query("UPDATE documents SET mime_type='application/pdf' WHERE id=?1")
        .bind(&fake.id)
        .execute(&f.pool)
        .await
        .unwrap();
    i.document_id = fake.id;
    blocked_unchanged(&f, &i).await;
    // PDF refresh is an ordinary identity-preserving workflow, not a reason to
    // reject valid external receipt evidence or historical receipt inspection.
    record(&f, &valid).await.unwrap();
}

#[tokio::test]
async fn ordinary_expense_continuation_and_encrypted_backup_reopen_preserve_receipt_proof() {
    use oppenbokforing_desktop_lib::{backup, expenses};
    let f = CashFixture::issued().await;
    let i = input(&f).await;
    expenses::expense_post(
        &f.pool,
        &f.workspace,
        &expenses::ExpensePostInput {
            amount_minor_ex_vat: 10000,
            vat_rate: 0.25,
            expense_account_number: "5610".into(),
            payment_account_number: "1930".into(),
            document_id: None,
            no_document_reason: Some("Synthetic ordinary expense regression".into()),
            staged_transaction_id: None,
            idempotency_key: "ordinary-cash-expense".into(),
            date: Some("2026-01-17".into()),
        },
    )
    .await
    .unwrap();
    let result = record(&f, &i).await.unwrap();
    second_issue(&f).await;
    let get = CashHistoryGetInput {
        invoice_id: i.invoice_id.clone(),
    };
    let before = serde_json::to_value(
        cash::invoice_cash_history_get(&f.pool, &f.workspace, &get)
            .await
            .unwrap(),
    )
    .unwrap();
    let destination = tempfile::tempdir().unwrap();
    let restored_root = tempfile::tempdir().unwrap();
    let package = backup::create_backup_package(
        &f.pool,
        &f.workspace,
        f.dir.path(),
        &f.dir.path().join("workspace.sqlite"),
        destination.path(),
        "synthetic-receipt-backup-only",
        None,
    )
    .await
    .unwrap();
    let restored = backup::restore_backup_package(
        &backup::BackupRestoreInput {
            backup_path: package.backup_path,
            confirm_overwrite: true,
            passphrase: "synthetic-receipt-backup-only".into(),
        },
        restored_root.path(),
    )
    .await
    .unwrap();
    let pool = oppenbokforing_desktop_lib::db::open_existing_workspace(std::path::Path::new(
        &restored.database_path,
    ))
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(
            cash::invoice_cash_history_get(&pool, &f.workspace, &get)
                .await
                .unwrap()
        )
        .unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_value(
            cash::invoice_cash_receipt_record(&pool, &f.workspace, &i)
                .await
                .unwrap()
        )
        .unwrap(),
        result
    );
    documents::verify_retained_document(&pool, &f.workspace, &i.document_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn annual_base_at_exact_limit_allows_receipt_without_readding_original_issue() {
    let f = CashFixture::issued().await;
    let first = input(&f).await;
    second_issue_amount(&f, 99_000_000).await; // public preflight/commit: total known base exactly SEK 1m
    record(&f, &first).await.unwrap(); // must not add the first invoice a second time
    let total: i64 = sqlx::query_scalar("SELECT SUM(net_minor) FROM cash_invoice_issues")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(total, 100_000_000);
    let history = cash::invoice_cash_history_get(
        &f.pool,
        &f.workspace,
        &CashHistoryGetInput {
            invoice_id: first.invoice_id,
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert!(history.receipt.is_some());
}

#[tokio::test]
async fn competing_invoices_cannot_consume_same_pdf_or_bank_tuple_concurrently() {
    for same_pdf in [false, true] {
        let f = CashFixture::issued().await;
        let first = input(&f).await;
        let mut other = first.clone();
        other.invoice_id = second_issue(&f).await;
        other.idempotency_key = "other-invoice-racing".into();
        if same_pdf {
            other
                .facts
                .bank_receipt_identity
                .as_mut()
                .unwrap()
                .transaction_reference = "another-entry".into();
        } else {
            let mut bytes = synthetic_bank_pdf();
            bytes.extend_from_slice(b"\n% another wrapper for same tuple\n");
            let doc = documents::store_document_bytes(
                &f.pool,
                &f.workspace,
                &bytes,
                "other-wrapper.pdf",
                "application/pdf",
            )
            .await
            .unwrap();
            other.document_id = doc.id;
        }
        let (a, b) = tokio::join!(record(&f, &first), record(&f, &other));
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        let proofs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cash_receipt_recognitions")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(proofs, 1);
        let journals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(journals, 3);
    }
}

#[tokio::test]
async fn retained_pdf_is_size_bounded_before_parsing() {
    let f = CashFixture::issued().await;
    let i = input(&f).await;
    let document = documents::document_get(&f.pool, &f.workspace, &i.document_id)
        .await
        .unwrap();
    let path = f.dir.path().join("documents").join(document.object_path);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(10 * 1024 * 1024 + 1)
        .unwrap();
    blocked_unchanged(&f, &i).await;
}
