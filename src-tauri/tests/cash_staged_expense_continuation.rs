//! Public new-business cash continuation after a genuinely imported/matched expense.
//! Synthetic bank evidence is not bank authentication or named-bank validation.
#[path = "cash_receipt_recognition/fixture.rs"]
mod fixture;
use fixture::{synthetic_bank_pdf, synthetic_bank_pdf_for, CashFixture};
use oppenbokforing_desktop_lib::{
    documents,
    expenses::{self, ExpensePostInput},
    imports::{self, CsvImportCreateInput, StagedTransactionsListInput},
    invoicing::{
        self,
        cash::{
            self, CashHistoryGetInput, CashIssueInput, CashIssuePreflightInput,
            CashReceiptRecordInput,
        },
        InvoiceCreateDraftInput, InvoiceLineInput,
    },
};
use serde_json::json;
use sqlx::Row;

async fn staged_expense(f: &CashFixture, day: &str) -> (String, String) {
    let path = f.dir.path().join("ordinary-expense.csv");
    std::fs::write(
        &path,
        format!(
            "date,description,amount_minor\n{day},Synthetic ordinary business expense,-12500\n"
        ),
    )
    .unwrap();
    let import = imports::csv_import_create(
        &f.pool,
        &f.workspace,
        &CsvImportCreateInput {
            source_path: path.to_string_lossy().into(),
            idempotency_key: "ordinary-expense-import".into(),
        },
    )
    .await
    .expect("public negative CSV import remains available");
    let staged = imports::staged_transactions_list(
        &f.pool,
        &f.workspace,
        &StagedTransactionsListInput {
            status: Some("staged".into()),
            limit: None,
            before_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].csv_import_id.as_deref(), Some(import.id.as_str()));
    assert_eq!(staged[0].transaction_date, day);
    assert_eq!(staged[0].amount_minor, -12500);
    let expense = expenses::expense_post(
        &f.pool,
        &f.workspace,
        &ExpensePostInput {
            amount_minor_ex_vat: 10000,
            vat_rate: 0.25,
            expense_account_number: "5610".into(),
            payment_account_number: "1930".into(),
            document_id: None,
            no_document_reason: Some("Synthetic ordinary business expense regression".into()),
            staged_transaction_id: Some(staged[0].id.clone()),
            idempotency_key: "ordinary-staged-expense".into(),
            date: Some(day.into()),
        },
    )
    .await
    .expect("public ordinary expense must commit with staged transaction ID");
    assert_eq!(
        (
            expense.debit_expense_minor,
            expense.debit_input_vat_minor,
            expense.credit_payment_minor
        ),
        (10000, 2500, 12500)
    );
    let voucher = expense.voucher_id.unwrap();
    let matched = sqlx::query("SELECT m.match_kind,m.invoice_id,m.voucher_id,s.status FROM reconciliation_matches m JOIN staged_transactions s ON s.id=m.staged_transaction_id WHERE m.workspace_id=?1 AND m.staged_transaction_id=?2")
        .bind(&f.workspace).bind(&staged[0].id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(matched.get::<String, _>("match_kind"), "expense");
    assert_eq!(matched.get::<Option<String>, _>("invoice_id"), None);
    assert_eq!(matched.get::<String, _>("voucher_id"), voucher);
    assert_eq!(matched.get::<String, _>("status"), "matched");
    println!("EXPENSE COMMITTED: public CSV {day} -12500 ore; public staged expense 5610 debit 10000 / 2641 debit 2500 / 1930 credit 12500; expense match, no invoice link");
    (staged[0].id.clone(), voucher)
}

async fn unstaged_expense(f: &CashFixture, day: &str) {
    let expense = expenses::expense_post(
        &f.pool,
        &f.workspace,
        &ExpensePostInput {
            amount_minor_ex_vat: 10000,
            vat_rate: 0.25,
            expense_account_number: "5610".into(),
            payment_account_number: "1930".into(),
            document_id: None,
            no_document_reason: Some("Synthetic unstaged expense review regression".into()),
            staged_transaction_id: None,
            idempotency_key: "ordinary-unstaged-expense".into(),
            date: Some(day.into()),
        },
    )
    .await
    .expect("public unstaged expense posting reproduces existing service behavior");
    assert!(expense.voucher_id.is_some());
    let matches: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reconciliation_matches WHERE workspace_id=?1")
            .bind(&f.workspace)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        matches, 0,
        "this regression must not rely on a staging/match guard"
    );
    println!("UNSTAGED EXPENSE COMMITTED: {day}; public 10000/2500/12500 posting, no reconciliation match");
}

async fn assert_unstaged_history_blocks_all_transitions(
    f: &CashFixture,
    input: CashIssuePreflightInput,
    issue: CashIssueInput,
    receipt: CashReceiptRecordInput,
) {
    let before = snapshot(f).await;
    let preflight = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &input).await;
    let issuance = cash::invoice_cash_issue(&f.pool, &f.workspace, &issue).await;
    let recognition = cash::invoice_cash_receipt_record(&f.pool, &f.workspace, &receipt).await;
    assert!(
        preflight.is_err() && issuance.is_err() && recognition.is_err(),
        "contradictory unstaged history must deny preflight/issue/receipt, got success={:?}",
        (preflight.is_ok(), issuance.is_ok(), recognition.is_ok())
    );
    for error in [
        preflight.unwrap_err(),
        issuance.unwrap_err(),
        recognition.unwrap_err(),
    ] {
        assert!(
            error.message.contains("history contradicts"),
            "wrong rejection: {error:?}"
        );
    }
    assert_eq!(
        snapshot(f).await,
        before,
        "rejected continuation must preserve existing expense/history and create no new cash state"
    );
}

#[tokio::test]
async fn unstaged_prior_origin_expense_blocks_cash_preflight_commit_and_receipt() {
    let f = CashFixture::issued().await;
    let input = next_issue_input(&f).await;
    let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    let issue = issue_input(input.clone(), review.assessment.review_fingerprint);
    let receipt = receipt_input(&f).await;
    unstaged_expense(&f, "2025-12-31").await;
    assert_unstaged_history_blocks_all_transitions(&f, input, issue, receipt).await;
}

#[tokio::test]
async fn genuine_unstaged_ordinary_expense_preserves_issue_and_receipt_continuation() {
    let f = CashFixture::issued().await;
    unstaged_expense(&f, "2026-01-22").await;
    let input = next_issue_input(&f).await;
    let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    let issued = cash::invoice_cash_issue(
        &f.pool,
        &f.workspace,
        &issue_input(input, review.assessment.review_fingerprint),
    )
    .await
    .unwrap();
    assert_eq!(issued.invoice_number.as_deref(), Some("2026-0002"));
    assert!(issued.voucher_id.is_none());
    let receipt = receipt_input(&f).await;
    cash::invoice_cash_receipt_record(&f.pool, &f.workspace, &receipt)
        .await
        .unwrap();
    let counts: (i64,i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM vouchers WHERE workspace_id=?1),(SELECT COUNT(*) FROM cash_receipt_recognitions WHERE workspace_id=?1)")
        .bind(&f.workspace).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        counts,
        (2, 1),
        "one ordinary expense and one actual-date cash receipt"
    );
}

#[tokio::test]
async fn unstaged_expense_requires_original_ordinary_posting_provenance() {
    for corruption in [
        "DELETE FROM local_jobs WHERE job_type='expense_post'",
        "UPDATE local_jobs SET status='failed' WHERE job_type='expense_post'",
        "UPDATE local_jobs SET payload_json=json_set(payload_json,'$.result.voucherId','unrelated') WHERE job_type='expense_post'",
        "UPDATE local_jobs SET payload_json=json_set(payload_json,'$.stagedTransactionId','invented-staging') WHERE job_type='expense_post'",
        "UPDATE local_jobs SET payload_json=json_set(payload_json,'$.result.debitExpenseMinor',9999) WHERE job_type='expense_post'",
        "UPDATE accounts SET account_type='asset' WHERE number='5610'",
        "UPDATE accounts SET number='2640' WHERE number='2641'",
    ] {
        let f = CashFixture::issued().await;
        unstaged_expense(&f, "2026-01-22").await;
        let input = next_issue_input(&f).await;
        let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &input).await.unwrap();
        let issue = issue_input(input.clone(), review.assessment.review_fingerprint);
        let receipt = receipt_input(&f).await;
        sqlx::query(corruption).execute(&f.pool).await.unwrap();
        assert_unstaged_history_blocks_all_transitions(&f, input, issue, receipt).await;
    }
}

async fn next_issue_input(f: &CashFixture) -> CashIssuePreflightInput {
    let history = cash::invoice_cash_history_get(
        &f.pool,
        &f.workspace,
        &CashHistoryGetInput {
            invoice_id: f.issued.id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    let draft = invoicing::create_draft(
        &f.pool,
        &f.workspace,
        &InvoiceCreateDraftInput {
            counterparty_id: f.issued.counterparty_id.clone(),
            due_date: Some("2026-02-28".into()),
            lines: vec![InvoiceLineInput {
                description: "Another completed ordinary service".into(),
                quantity: 1,
                unit_price_minor: 1000000,
                vat_rate: 0.25,
                account_number: Some("3041".into()),
            }],
        },
    )
    .await
    .unwrap();
    CashIssuePreflightInput {
        invoice_id: draft.id,
        issue_date: "2026-01-23".into(),
        facts: history.issue.facts,
        origin_review: None,
    }
}

async fn receipt_input(f: &CashFixture) -> CashReceiptRecordInput {
    // The retained statement is independent of the expense CSV and invoice PDF.
    // Explicitly replace the fixture's Jan 20 date/reference with the actual facts.
    let bytes = synthetic_bank_pdf_for(
        "2026-01-23",
        "SYNTHETIC-STAGED-EXPENSE-CONTINUATION",
        "2026-0001",
    );
    assert_ne!(bytes, synthetic_bank_pdf());
    let doc = documents::store_document_bytes(
        &f.pool,
        &f.workspace,
        &bytes,
        "SYNTHETIC-receipt.pdf",
        "application/pdf",
    )
    .await
    .unwrap();
    serde_json::from_value(json!({"invoiceId":f.issued.id,"documentId":doc.id,"idempotencyKey":"staged-expense-first-receipt","facts":{
        "actualReceivedAmountMinor":1250000,"actualReceivedOn":"2026-01-23","currency":"SEK","receivedAlready":true,
        "bankReceiptIdentity":{"bankId":"SYNTHETIC-TEST-BANK","receivingAccountReference":"SYNTHETIC-BUSINESS-ACCOUNT-001","transactionReference":"SYNTHETIC-STAGED-EXPENSE-CONTINUATION"},
        "sourceEntryLocator":"page 1 / ref SYNTHETIC-STAGED-EXPENSE-CONTINUATION","bankPdfMatchesReceiptConfirmed":true,"issuedCaseStillAppliesConfirmed":true,"noOtherReceiptsOrAdjustmentsConfirmed":true}})).unwrap()
}

fn issue_input(i: CashIssuePreflightInput, fingerprint: String) -> CashIssueInput {
    CashIssueInput {
        invoice_id: i.invoice_id,
        issue_date: i.issue_date,
        facts: i.facts,
        origin_review: None,
        idempotency_key: "next-issue-after-staged-expense".into(),
        review_fingerprint: fingerprint,
    }
}

#[tokio::test]
async fn staged_ordinary_expense_allows_next_cash_issue() {
    let f = CashFixture::issued().await;
    staged_expense(&f, "2026-01-22").await;
    let i = next_issue_input(&f).await;
    let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &i)
        .await
        .expect("genuine staged ordinary expense must permit next cash issue preflight");
    assert_eq!(review.current_taxable_base_minor, 1000000);
    assert_eq!(review.projected_taxable_base_minor, 2000000);
    let issued = cash::invoice_cash_issue(
        &f.pool,
        &f.workspace,
        &issue_input(i, review.assessment.review_fingerprint),
    )
    .await
    .expect("reviewed next public cash issue");
    assert_eq!(issued.invoice_number.as_deref(), Some("2026-0002"));
    assert_eq!(issued.voucher_id, None);
    let vouchers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vouchers WHERE workspace_id=?1")
        .bind(&f.workspace)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(vouchers, 1, "only the expense is posted, not either issue");
}

#[tokio::test]
async fn staged_ordinary_expense_allows_first_full_pdf_cash_receipt() {
    let f = CashFixture::issued().await;
    staged_expense(&f, "2026-01-22").await;
    let i = receipt_input(&f).await;
    cash::invoice_cash_receipt_record(&f.pool, &f.workspace, &i)
        .await
        .expect("genuine staged ordinary expense must permit first full PDF cash receipt");
    let history = cash::invoice_cash_history_get(
        &f.pool,
        &f.workspace,
        &CashHistoryGetInput {
            invoice_id: f.issued.id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        history.receipt.unwrap().facts.actual_received_on.as_deref(),
        Some("2026-01-23")
    );
    let lines: Vec<(String,i64,i64)> = sqlx::query_as("SELECT a.number,j.debit_minor,j.credit_minor FROM journal_lines j JOIN accounts a ON a.id=j.account_id JOIN vouchers v ON v.id=j.voucher_id WHERE v.workspace_id=?1 ORDER BY a.number,j.debit_minor")
        .bind(&f.workspace).fetch_all(&f.pool).await.unwrap();
    assert_eq!(
        lines,
        vec![
            ("1930".into(), 0, 12500),
            ("1930".into(), 1250000, 0),
            ("2611".into(), 0, 250000),
            ("2641".into(), 2500, 0),
            ("3041".into(), 0, 1000000),
            ("5610".into(), 10000, 0)
        ]
    );
    // Completed recovery does not post again or consume the expense's CSV source.
    cash::invoice_cash_receipt_record(&f.pool, &f.workspace, &i)
        .await
        .unwrap();
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM vouchers),(SELECT COUNT(*) FROM reconciliation_matches),(SELECT COUNT(*) FROM cash_receipt_recognitions)").fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts, (2, 2, 1));
}

async fn snapshot(f: &CashFixture) -> Vec<String> {
    use sqlx::Column;
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    let mut result = Vec::new();
    for table in tables {
        let rows = sqlx::query(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))
            .fetch_all(&f.pool)
            .await
            .unwrap();
        if let Some(row) = rows.first() {
            let columns = row
                .columns()
                .iter()
                .map(|c| format!("quote(\"{}\")", c.name()))
                .collect::<Vec<_>>();
            let values: Vec<String> = sqlx::query_scalar(&format!(
                "SELECT {} FROM \"{table}\" ORDER BY rowid",
                columns.join("||'|'||")
            ))
            .fetch_all(&f.pool)
            .await
            .unwrap();
            result.extend(values.into_iter().map(|v| format!("{table}:{v}")));
        }
    }
    result
}

#[tokio::test]
async fn forged_or_mismatched_expense_matches_deny_preflight_commit_and_receipt_atomically() {
    // SQL is only destructive corruption after genuine public issuance/import/
    // expense posting. No invented cash proof or positive authority is seeded.
    let corruptions = [
        "UPDATE reconciliation_matches SET match_kind='unknown'",
        "UPDATE reconciliation_matches SET match_kind='invoice_payment'",
        "UPDATE reconciliation_matches SET invoice_id=(SELECT id FROM invoices WHERE status='issued')",
        "UPDATE reconciliation_matches SET voucher_id=NULL",
        "UPDATE staged_transactions SET status='staged'",
        "UPDATE staged_transactions SET status='ignored'",
        "UPDATE staged_transactions SET amount_minor=12500",
        "UPDATE staged_transactions SET amount_minor=0",
        "UPDATE staged_transactions SET amount_minor=-12501",
        "UPDATE staged_transactions SET transaction_date='2026-01-21'",
        "UPDATE staged_transactions SET workspace_id='foreign'",
        "UPDATE vouchers SET status='draft'",
        "UPDATE vouchers SET status='reversed'",
        "UPDATE vouchers SET source_type='manual'",
        "UPDATE vouchers SET source_id='unrelated-source'",
        "UPDATE vouchers SET workspace_id='foreign'",
        "UPDATE vouchers SET fiscal_year_id=NULL",
        "UPDATE vouchers SET accounting_date=NULL",
        "UPDATE vouchers SET accounting_date='2026-01-21'",
        "UPDATE local_jobs SET payload_json=json_set(payload_json,'$.stagedTransactionId',NULL) WHERE job_type='expense_post'",
        "UPDATE local_jobs SET payload_json=json_set(payload_json,'$.result.voucherId','unrelated-voucher') WHERE job_type='expense_post'",
        "UPDATE local_jobs SET workspace_id='foreign' WHERE job_type='expense_post'",
        "UPDATE accounts SET workspace_id='foreign' WHERE number='5610'",
        "UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE number='3041') WHERE account_id=(SELECT id FROM accounts WHERE number='5610')",
        "UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE number='1510') WHERE account_id=(SELECT id FROM accounts WHERE number='5610')",
        "UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE number='2611') WHERE account_id=(SELECT id FROM accounts WHERE number='2641')",
        "UPDATE accounts SET number='2621' WHERE number='2641'",
        "UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE number='1930') WHERE account_id=(SELECT id FROM accounts WHERE number='5610')",
        "UPDATE journal_lines SET debit_minor=9999 WHERE account_id=(SELECT id FROM accounts WHERE number='5610')",
    ];
    for corruption in corruptions {
        let f = CashFixture::issued().await;
        staged_expense(&f, "2026-01-22").await;
        let preflight = next_issue_input(&f).await;
        let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &preflight)
            .await
            .unwrap();
        let issue = issue_input(preflight.clone(), review.assessment.review_fingerprint);
        let receipt = receipt_input(&f).await;
        sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) SELECT 'foreign','Foreign corruption target',database_path||'.foreign',documents_path,exports_path FROM workspaces")
            .execute(&f.pool).await.unwrap();
        for trigger in [
            "vouchers_no_update_when_posted",
            "journal_lines_no_update_for_posted_voucher",
        ] {
            sqlx::query(&format!("DROP TRIGGER {trigger}"))
                .execute(&f.pool)
                .await
                .unwrap();
        }
        sqlx::query(corruption)
            .execute(&f.pool)
            .await
            .expect(corruption);
        let before = snapshot(&f).await;
        let a = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &preflight)
            .await
            .unwrap_err();
        let b = cash::invoice_cash_issue(&f.pool, &f.workspace, &issue)
            .await
            .unwrap_err();
        let c = cash::invoice_cash_receipt_record(&f.pool, &f.workspace, &receipt)
            .await
            .unwrap_err();
        for error in [a, b, c] {
            assert!(
                error.message.contains("history contradicts")
                    || (corruption.contains("SET invoice_id=")
                        && error.message == "Cash invoice already paid"),
                "wrong guard for {corruption}: {error:?}"
            );
        }
        assert_eq!(
            snapshot(&f).await,
            before,
            "failed continuation changed state for {corruption}"
        );
    }
}

#[tokio::test]
async fn staged_expense_cannot_backdate_history_before_actual_short_first_year() {
    let f = CashFixture::issued_dates("2026-01-05", "2026-01-10", "2026-01-15").await;
    staged_expense(&f, "2026-01-22").await;
    let i = next_issue_input(&f).await;
    // The ordinary, correctly dated short-year continuation is supported.
    cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &i)
        .await
        .unwrap();
    let receipt = receipt_input(&f).await;
    sqlx::query("DROP TRIGGER vouchers_no_update_when_posted")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE vouchers SET accounting_date='2026-01-02'")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE staged_transactions SET transaction_date='2026-01-02'")
        .execute(&f.pool)
        .await
        .unwrap();
    let before = snapshot(&f).await;
    assert!(
        cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &i)
            .await
            .is_err()
    );
    assert!(
        cash::invoice_cash_receipt_record(&f.pool, &f.workspace, &receipt)
            .await
            .is_err()
    );
    assert_eq!(snapshot(&f).await, before);
}

#[tokio::test]
async fn expense_label_cannot_link_an_unmatched_voucher_even_with_same_date_and_amount() {
    let f = CashFixture::issued().await;
    staged_expense(&f, "2026-01-22").await;
    // A second real posting has identical amounts/accounts/date but never consumed
    // this staging. Merely pointing an expense-labelled match at it is not proof.
    let unrelated = expenses::expense_post(
        &f.pool,
        &f.workspace,
        &ExpensePostInput {
            amount_minor_ex_vat: 10000,
            vat_rate: 0.25,
            expense_account_number: "5610".into(),
            payment_account_number: "1930".into(),
            document_id: None,
            no_document_reason: Some("Separate synthetic ordinary expense".into()),
            staged_transaction_id: None,
            idempotency_key: "separate-unmatched-expense".into(),
            date: Some("2026-01-22".into()),
        },
    )
    .await
    .unwrap();
    let preflight = next_issue_input(&f).await;
    let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &preflight)
        .await
        .unwrap();
    let issue = issue_input(preflight.clone(), review.assessment.review_fingerprint);
    let receipt = receipt_input(&f).await;
    sqlx::query("UPDATE reconciliation_matches SET voucher_id=?1")
        .bind(unrelated.voucher_id.unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    let before = snapshot(&f).await;
    assert!(
        cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &preflight)
            .await
            .is_err()
    );
    assert!(cash::invoice_cash_issue(&f.pool, &f.workspace, &issue)
        .await
        .is_err());
    assert!(
        cash::invoice_cash_receipt_record(&f.pool, &f.workspace, &receipt)
            .await
            .is_err()
    );
    assert_eq!(snapshot(&f).await, before);
}
