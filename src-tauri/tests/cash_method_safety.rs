//! Interim safety contract: selection is retained, but only invoice_method may
//! authorize new affected postings/approvals. Legacy books are simulated with SQL,
//! never by treating an ordinary post-posting profile save as a migration.
pub mod common;

use oppenbokforing_desktop_lib::{
    counterparties::{self, CounterpartyCreateInput},
    db::connect_workspace,
    documents::{self, DocumentImportInput},
    error::AppError,
    expenses::{self, ExpensePostInput},
    imports::{self, CsvImportCreateInput, StagedTransactionsListInput},
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceCreditInput, InvoiceIssueInput, InvoiceLineInput,
    },
    ledger::{self, VatBucket},
    profiles::{
        self, BusinessProfileSaveInput, OnboardingProfileSaveInput, TaxProfileSaveInput,
        VatProfileSaveInput,
    },
    reconciliation::{self, InvoicePaymentRecordInput, ReconciliationMatchCreateInput},
    vat::{self, VatReturnApproveInput, VatReturnDraftCreateInput},
    workspace::ensure_workspace_ready,
    year_end::{self, YearEndPackageApproveInput, YearEndPackageCreateInput},
};
use sqlx::{Row, SqlitePool};
use std::{fs, path::Path, time::Duration};
use tempfile::{tempdir, TempDir};
use uuid::Uuid;

struct Fixture {
    dir: TempDir,
    pool: SqlitePool,
    workspace: String,
    customer: String,
}

fn business() -> BusinessProfileSaveInput {
    BusinessProfileSaveInput {
        business_name: "Safety Test Firma".into(),
        owner_name: "Owner".into(),
        residency_country: Some("SE".into()),
        sni_code: Some("62010".into()),
    }
}

fn tax() -> TaxProfileSaveInput {
    TaxProfileSaveInput {
        tax_status: "f_skatt".into(),
        expected_business_profit_minor: Some(1_000_000),
        expected_salary_income_minor: Some(0),
        active_rule_year: Some(2026),
    }
}

fn vat_profile(method: &str) -> VatProfileSaveInput {
    VatProfileSaveInput {
        vat_status: "registered".into(),
        reporting_period: "yearly".into(),
        accounting_method: method.into(),
        voluntary_registration_date: None,
        vat_filing_deadline_regime: Some("annual_may_12".into()),
    }
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("documents")).unwrap();
        fs::create_dir_all(dir.path().join("exports")).unwrap();
        let database = dir.path().join("workspace.sqlite");
        let pool = connect_workspace(&database).await.expect("database");
        let workspace = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO workspaces (id, name, database_path, documents_path, exports_path) VALUES (?1, 'Cash safety', ?2, ?3, ?4)")
            .bind(&workspace)
            .bind(database.to_string_lossy().as_ref())
            .bind(dir.path().join("documents").to_string_lossy().as_ref())
            .bind(dir.path().join("exports").to_string_lossy().as_ref())
            .execute(&pool).await.expect("workspace");
        ensure_workspace_ready(&pool, &workspace)
            .await
            .expect("bootstrap");
        profiles::save_onboarding_profiles(
            &pool,
            &workspace,
            &OnboardingProfileSaveInput {
                business: business(),
                tax: tax(),
                vat: vat_profile("invoice_method"),
            },
        )
        .await
        .expect("valid invoice-method onboarding");
        let customer = counterparties::create_counterparty(
            &pool,
            &workspace,
            &CounterpartyCreateInput {
                kind: "customer".into(),
                name: "Customer".into(),
                email: None,
                org_number: None,
            },
        )
        .await
        .expect("customer")
        .id;
        Self {
            dir,
            pool,
            workspace,
            customer,
        }
    }

    fn fiscal_year(&self) -> String {
        format!("fy-{}-2026", self.workspace)
    }

    async fn draft(&self) -> invoicing::InvoiceSummary {
        invoicing::create_draft(
            &self.pool,
            &self.workspace,
            &InvoiceCreateDraftInput {
                counterparty_id: self.customer.clone(),
                due_date: Some("2026-03-01".into()),
                lines: vec![InvoiceLineInput {
                    description: "Service".into(),
                    quantity: 1,
                    unit_price_minor: 100_000,
                    vat_rate: 0.25,
                    account_number: Some("3041".into()),
                }],
            },
        )
        .await
        .expect("drafts remain available")
    }

    fn issue_input(&self, invoice: &str, key: &str) -> InvoiceIssueInput {
        InvoiceIssueInput {
            eligibility_facts: None,
            review_fingerprint: None,
            invoice_id: invoice.into(),
            idempotency_key: key.into(),
            issue_date: Some("2026-01-20".into()),
        }
    }

    async fn issued(&self, key: &str) -> invoicing::InvoiceSummary {
        let draft = self.draft().await;
        common::issue_reviewed(
            &self.pool,
            &self.workspace,
            &self.issue_input(&draft.id, key),
        )
        .await
        .expect("invoice_method issue prerequisite")
    }

    async fn legacy_cash(&self) {
        sqlx::query(
            "UPDATE vat_profiles SET accounting_method = 'cash_method' WHERE workspace_id = ?1",
        )
        .bind(&self.workspace)
        .execute(&self.pool)
        .await
        .expect("simulate legacy cash DB");
    }

    async fn missing_method(&self) {
        sqlx::query("DELETE FROM vat_profiles WHERE workspace_id = ?1")
            .bind(&self.workspace)
            .execute(&self.pool)
            .await
            .expect("simulate unresolved method");
    }

    async fn evidence(&self) -> documents::Document {
        let path = self.dir.path().join("statement.pdf");
        fs::write(&path, b"%PDF-1.4 retained bank statement").unwrap();
        documents::document_import(
            &self.pool,
            &self.workspace,
            &DocumentImportInput {
                source_path: path.to_string_lossy().into(),
                filename: "statement.pdf".into(),
                mime_type: "application/pdf".into(),
                idempotency_key: "statement-import".into(),
            },
        )
        .await
        .expect("retain evidence even under cash method")
    }

    async fn csv(&self) -> imports::CsvImportSummary {
        let path = self.dir.path().join("bank.csv");
        fs::write(
            &path,
            "date,description,amount_minor\n2026-02-15,Customer receipt,125000\n",
        )
        .unwrap();
        imports::csv_import_create(
            &self.pool,
            &self.workspace,
            &CsvImportCreateInput {
                source_path: path.to_string_lossy().into(),
                idempotency_key: "bank-import".into(),
            },
        )
        .await
        .expect("CSV remains importable under cash method")
    }

    async fn vat_draft(&self) -> vat::VatReturnSummary {
        vat::vat_return_draft_create(
            &self.pool,
            &self.workspace,
            &VatReturnDraftCreateInput {
                period_key: "2026".into(),
                idempotency_key: "vat-draft".into(),
            },
        )
        .await
        .expect("VAT draft")
    }

    async fn approve_vat(&self) -> vat::VatReturnSummary {
        let draft = self.vat_draft().await;
        vat::vat_return_approve(
            &self.pool,
            &self.workspace,
            &VatReturnApproveInput {
                reviewed_revision: draft.review_revision.clone().expect("review revision"),
                vat_return_id: draft.id,
                idempotency_key: "vat-approve".into(),
            },
        )
        .await
        .expect("invoice_method VAT approval prerequisite")
    }

    async fn year_end_draft(&self) -> year_end::YearEndPackageSummary {
        year_end::year_end_package_create(
            &self.pool,
            &self.workspace,
            &YearEndPackageCreateInput {
                fiscal_year: 2026,
                idempotency_key: "year-end-draft".into(),
            },
        )
        .await
        .expect("year-end draft with real filesystem artifacts")
    }

    async fn expense(&self) -> expenses::ExpensePostResult {
        expenses::expense_post(
            &self.pool,
            &self.workspace,
            &ExpensePostInput {
                amount_minor_ex_vat: 10_000,
                vat_rate: 0.25,
                expense_account_number: "5610".into(),
                payment_account_number: "1930".into(),
                document_id: None,
                no_document_reason: Some("Receipt unavailable".into()),
                staged_transaction_id: None,
                idempotency_key: "expense".into(),
                date: Some("2026-01-10".into()),
            },
        )
        .await
        .expect("expense posting is outside this interim guard")
    }
}

// Content, not just counts: this catches edits to historical rows as well as
// leaked sequences, snapshots, jobs, matches, locks, profile writes, and audits.
// Each fixture has its own database, so snapshot every application table.
type DatabaseSnapshot = Vec<(String, Vec<String>)>;
async fn database_snapshot(pool: &SqlitePool) -> DatabaseSnapshot {
    let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .fetch_all(pool).await.unwrap();
    let mut snapshot = Vec::new();
    for table in tables {
        let columns = sqlx::query(&format!("PRAGMA table_info(\"{table}\")"))
            .fetch_all(pool)
            .await
            .unwrap();
        let expression = columns
            .iter()
            .map(|row| format!("quote(\"{}\")", row.get::<String, _>("name")))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        let mut rows: Vec<String> =
            sqlx::query_scalar(&format!("SELECT {expression} FROM \"{table}\""))
                .fetch_all(pool)
                .await
                .unwrap();
        rows.sort();
        snapshot.push((table, rows));
    }
    snapshot
}

fn artifact_snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for folder in ["documents", "exports"] {
        for entry in walkdir::WalkDir::new(root.join(folder)) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() {
                files.push((
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into(),
                    fs::read(entry.path()).unwrap(),
                ));
            }
        }
    }
    files.sort();
    files
}

fn assert_method_error(error: &AppError, cash: bool) {
    assert_eq!(error.code, "validation_error", "{error:?}");
    assert!(
        error.details.as_ref().is_some_and(|details| details
            .iter()
            .any(|detail| detail.field.as_deref() == Some("accountingMethod"))),
        "{error:?}"
    );
    let message = error.message.to_lowercase();
    if cash {
        assert!(
            message.contains("method")
                && (message.contains("unavailable")
                    || message.contains("not supported")
                    || message.contains("not yet supported")),
            "must explain unavailable method-specific timing: {error:?}"
        );
        assert!(
            message.contains("timing") || message.contains("recognition"),
            "must explain timing limitation: {error:?}"
        );
        assert!(
            message.contains("review")
                && (message.contains("remediat") || message.contains("correct")),
            "must explain reviewed legacy remediation: {error:?}"
        );
    } else {
        assert!(
            message.contains("method"),
            "must explain unresolved method: {error:?}"
        );
    }
}

async fn assert_rejected<T>(
    f: &Fixture,
    before: DatabaseSnapshot,
    files: Vec<(String, Vec<u8>)>,
    result: Result<T, AppError>,
    cash: bool,
) {
    let after = database_snapshot(&f.pool).await;
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("new mutation unexpectedly succeeded without accounting-method guard (database unchanged: {}, artifacts unchanged: {})", before == after, files == artifact_snapshot(f.dir.path())),
    };
    assert_method_error(&error, cash);
    assert_eq!(
        before, after,
        "rejection must preserve all persisted content"
    );
    assert_eq!(
        files,
        artifact_snapshot(f.dir.path()),
        "rejection must preserve retained evidence/artifacts"
    );
}

fn credit_input(source: &str, key: &str) -> InvoiceCreditInput {
    InvoiceCreditInput {
        source_invoice_id: source.into(),
        idempotency_key: key.into(),
        reason: Some("Correction".into()),
        issue_date: Some("2026-02-01".into()),
    }
}
fn staged_input(invoice: &str, staged: &str) -> ReconciliationMatchCreateInput {
    ReconciliationMatchCreateInput {
        staged_transaction_id: staged.into(),
        match_kind: "invoice_payment".into(),
        invoice_id: Some(invoice.into()),
        idempotency_key: "staged-payment".into(),
    }
}
fn direct_input(invoice: &str, document: &str) -> InvoicePaymentRecordInput {
    InvoicePaymentRecordInput {
        invoice_id: invoice.into(),
        document_id: document.into(),
        payment_date: Some("2026-02-15".into()),
        idempotency_key: "direct-payment".into(),
    }
}
fn buckets() -> Vec<VatBucket> {
    vec![VatBucket {
        vat_code: "VAT25".into(),
        ex_vat_minor: 100_000,
        vat_minor: 25_000,
    }]
}

#[derive(Clone, Copy)]
enum Mutation {
    Preflight,
    Issue,
    Credit,
    StagedPayment,
    DirectPayment,
    LedgerInvoice,
    LedgerReversal,
    LedgerInvoiceTx,
    LedgerReversalTx,
    VatApproval,
    YearEndApproval,
}

async fn rejects_new_mutation(mutation: Mutation, cash: bool) {
    let f = Fixture::new().await;
    // Every legacy case has genuine posted history and issue-time provenance.
    let source = f.issued("historical-issue").await;
    let draft = f.draft().await;
    let vat_draft = if matches!(mutation, Mutation::VatApproval) {
        Some(f.vat_draft().await)
    } else {
        None
    };
    let package = if matches!(mutation, Mutation::YearEndApproval) {
        // Approve the configured annual VAT period and capture a current ledger
        // fingerprint BEFORE conversion. Missing prerequisites must not make RED.
        f.approve_vat().await;
        Some(f.year_end_draft().await)
    } else {
        None
    };
    if cash {
        f.legacy_cash().await;
    } else {
        f.missing_method().await;
    }
    // Evidence and staging remain legal, including for blocked settlement cases.
    let document = f.evidence().await;
    let imported = f.csv().await;
    let before = database_snapshot(&f.pool).await;
    let files = artifact_snapshot(f.dir.path());
    let result = match mutation {
        Mutation::Preflight => invoicing::invoice_issue_preflight(
            &f.pool,
            &f.workspace,
            &invoicing::InvoiceIssuePreflightInput {
                eligibility_facts: None,
                invoice_id: draft.id.clone(),
                issue_date: Some("2026-01-20".into()),
            },
        )
        .await
        .map(|_| ()),
        Mutation::Issue => invoicing::issue_invoice(
            &f.pool,
            &f.workspace,
            &f.issue_input(&draft.id, "new-issue"),
        )
        .await
        .map(|_| ()),
        Mutation::Credit => invoicing::credit_invoice(
            &f.pool,
            &f.workspace,
            &credit_input(&source.id, "new-credit"),
        )
        .await
        .map(|_| ()),
        Mutation::StagedPayment => reconciliation::reconciliation_match_create(
            &f.pool,
            &f.workspace,
            &staged_input(&source.id, &imported.first_staged_transaction_id),
        )
        .await
        .map(|_| ()),
        Mutation::DirectPayment => reconciliation::invoice_payment_record(
            &f.pool,
            &f.workspace,
            &direct_input(&source.id, &document.id),
        )
        .await
        .map(|_| ()),
        Mutation::LedgerInvoice => ledger::post_invoice_voucher(
            &f.pool,
            &f.workspace,
            &f.fiscal_year(),
            &draft.id,
            "2026-02-15",
            &buckets(),
        )
        .await
        .map(|_| ()),
        Mutation::LedgerReversal => ledger::post_reversal_voucher(
            &f.pool,
            &f.workspace,
            &f.fiscal_year(),
            &draft.id,
            source.voucher_id.as_deref().unwrap(),
            "2026-02-15",
            &buckets(),
        )
        .await
        .map(|_| ()),
        Mutation::LedgerInvoiceTx | Mutation::LedgerReversalTx => {
            let mut tx = f.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
            let result = if matches!(mutation, Mutation::LedgerInvoiceTx) {
                ledger::post_invoice_voucher_tx(
                    &mut tx,
                    &f.workspace,
                    &f.fiscal_year(),
                    &draft.id,
                    "2026-02-15",
                    &buckets(),
                )
                .await
            } else {
                ledger::post_reversal_voucher_tx(
                    &mut tx,
                    &f.workspace,
                    &f.fiscal_year(),
                    &draft.id,
                    "2026-02-15",
                    &buckets(),
                )
                .await
            };
            // Commit even on rejection: callers must not receive partial writes
            // from a low-level helper that claims to reject before posting.
            tx.commit().await.unwrap();
            result.map(|_| ())
        }
        Mutation::VatApproval => vat::vat_return_approve(
            &f.pool,
            &f.workspace,
            &VatReturnApproveInput {
                reviewed_revision: vat_draft.as_ref().unwrap().review_revision.clone().expect("review revision"),
                vat_return_id: vat_draft.unwrap().id,
                idempotency_key: "new-vat-approval".into(),
            },
        )
        .await
        .map(|_| ()),
        Mutation::YearEndApproval => year_end::year_end_package_approve(
            &f.pool,
            &f.workspace,
            &YearEndPackageApproveInput {
                package_id: package.unwrap().id,
                idempotency_key: "new-year-end-approval".into(),
            },
        )
        .await
        .map(|_| ()),
    };
    assert_rejected(&f, before, files, result, cash).await;
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
    assert!(
        staged
            .iter()
            .any(|row| row.id == imported.first_staged_transaction_id),
        "blocked receipt remains available for review"
    );
}

macro_rules! method_cases {
    ($cash:ident, $missing:ident, $mutation:ident) => {
        #[tokio::test]
        async fn $cash() {
            rejects_new_mutation(Mutation::$mutation, true).await;
        }
        #[tokio::test]
        async fn $missing() {
            rejects_new_mutation(Mutation::$mutation, false).await;
        }
    };
}
method_cases!(
    cash_rejects_preflight,
    missing_method_rejects_preflight,
    Preflight
);
method_cases!(
    cash_rejects_new_issue,
    missing_method_rejects_new_issue,
    Issue
);
method_cases!(
    cash_rejects_new_credit,
    missing_method_rejects_new_credit,
    Credit
);
method_cases!(
    cash_rejects_staged_payment,
    missing_method_rejects_staged_payment,
    StagedPayment
);
method_cases!(
    cash_rejects_direct_payment,
    missing_method_rejects_direct_payment,
    DirectPayment
);
method_cases!(
    cash_rejects_ledger_invoice,
    missing_method_rejects_ledger_invoice,
    LedgerInvoice
);
method_cases!(
    cash_rejects_ledger_reversal,
    missing_method_rejects_ledger_reversal,
    LedgerReversal
);
method_cases!(
    cash_rejects_ledger_invoice_tx,
    missing_method_rejects_ledger_invoice_tx,
    LedgerInvoiceTx
);
method_cases!(
    cash_rejects_ledger_reversal_tx,
    missing_method_rejects_ledger_reversal_tx,
    LedgerReversalTx
);
method_cases!(
    cash_rejects_vat_approval,
    missing_method_rejects_vat_approval,
    VatApproval
);
method_cases!(
    cash_rejects_year_end_approval,
    missing_method_rejects_year_end_approval,
    YearEndApproval
);

#[derive(Clone, Copy)]
enum SaveRoute {
    Individual,
    Onboarding,
}
async fn save_method(f: &Fixture, route: SaveRoute, method: &str) -> Result<(), AppError> {
    match route {
        SaveRoute::Individual => {
            profiles::save_vat_profile(&f.pool, &f.workspace, &vat_profile(method))
                .await
                .map(|_| ())
        }
        SaveRoute::Onboarding => profiles::save_onboarding_profiles(
            &f.pool,
            &f.workspace,
            &OnboardingProfileSaveInput {
                // A refused composite save must not leak these unrelated edits.
                business: BusinessProfileSaveInput {
                    business_name: "Changed name".into(),
                    ..business()
                },
                tax: TaxProfileSaveInput {
                    expected_business_profit_minor: Some(2_000_000),
                    ..tax()
                },
                vat: vat_profile(method),
            },
        )
        .await
        .map(|_| ()),
    }
}

async fn rejects_posted_method_change(route: SaveRoute, from_cash: bool) {
    let f = Fixture::new().await;
    // An expense voucher is sufficient: the freeze must not only count invoices.
    let expense = f.expense().await;
    assert!(expense.voucher_id.is_some());
    if from_cash {
        f.legacy_cash().await;
    }
    let before = database_snapshot(&f.pool).await;
    let files = artifact_snapshot(f.dir.path());
    let result = save_method(
        &f,
        route,
        if from_cash {
            "invoice_method"
        } else {
            "cash_method"
        },
    )
    .await;
    // Method-transition errors need the same field, but are distinct from the
    // unsupported-posting message (they explain the post-posting freeze).
    let after = database_snapshot(&f.pool).await;
    let error = match result {
        Err(error) => error,
        Ok(()) => panic!("ordinary save unexpectedly changed method after a posted voucher (database unchanged: {})", before == after),
    };
    assert_method_error(&error, false);
    assert!(
        error.message.to_lowercase().contains("post"),
        "must explain the posting boundary: {error:?}"
    );
    assert_eq!(
        before, after,
        "refused profile save is atomic, including composite profiles/audit"
    );
    assert_eq!(files, artifact_snapshot(f.dir.path()));
}

#[tokio::test]
async fn individual_save_freezes_invoice_to_cash_after_any_posting() {
    rejects_posted_method_change(SaveRoute::Individual, false).await;
}
#[tokio::test]
async fn individual_save_freezes_legacy_cash_to_invoice_after_any_posting() {
    rejects_posted_method_change(SaveRoute::Individual, true).await;
}
#[tokio::test]
async fn onboarding_save_freezes_invoice_to_cash_after_any_posting() {
    rejects_posted_method_change(SaveRoute::Onboarding, false).await;
}
#[tokio::test]
async fn onboarding_save_freezes_legacy_cash_to_invoice_after_any_posting() {
    rejects_posted_method_change(SaveRoute::Onboarding, true).await;
}

#[tokio::test]
async fn both_save_routes_allow_selection_before_posting_and_same_method_after_posting() {
    for route in [SaveRoute::Individual, SaveRoute::Onboarding] {
        for method in ["invoice_method", "cash_method"] {
            let f = Fixture::new().await;
            for selection in ["cash_method", "invoice_method", method] {
                save_method(&f, route, selection)
                    .await
                    .expect("selection can change before any posting");
                assert_eq!(
                    profiles::get_vat_profile(&f.pool, &f.workspace)
                        .await
                        .unwrap()
                        .unwrap()
                        .accounting_method,
                    selection
                );
            }
            f.expense().await;
            save_method(&f, route, method)
                .await
                .expect("same-method saves remain available after posting");
            assert_eq!(
                profiles::get_vat_profile(&f.pool, &f.workspace)
                    .await
                    .unwrap()
                    .unwrap()
                    .accounting_method,
                method
            );
        }
    }
}

#[tokio::test]
async fn cash_preserves_drafts_evidence_csv_expenses_and_preparatory_packages() {
    let f = Fixture::new().await;
    save_method(&f, SaveRoute::Individual, "cash_method")
        .await
        .unwrap();
    let draft = f.draft().await;
    let edited = invoicing::update_draft(
        &f.pool,
        &f.workspace,
        &invoicing::InvoiceUpdateDraftInput {
            invoice_id: draft.id.clone(),
            due_date: Some("2026-04-01".into()),
            lines: vec![InvoiceLineInput {
                description: "Edited draft".into(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".into()),
            }],
        },
    )
    .await
    .expect("cash drafts remain editable");
    assert_eq!(edited.status, "draft");
    let document = f.evidence().await;
    documents::verify_retained_document(&f.pool, &f.workspace, &document.id)
        .await
        .unwrap();
    let imported = f.csv().await;
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
    assert_eq!(staged[0].id, imported.first_staged_transaction_id);
    assert_eq!(staged[0].amount_minor, 125_000);
    assert!(f.expense().await.voucher_id.is_some());
    assert_eq!(f.vat_draft().await.status, "draft");
    assert_eq!(
        fs::read(f.dir.path().join("documents").join(document.object_path)).unwrap(),
        b"%PDF-1.4 retained bank statement"
    );

    // Year-end creation already requires approved VAT. Prepare that under the
    // supported method, then prove legacy cash does not newly block drafting.
    let closing = Fixture::new().await;
    closing.issued("closing-history").await;
    closing.approve_vat().await;
    closing.legacy_cash().await;
    assert_eq!(closing.year_end_draft().await.status, "draft");
}

async fn positive_workflow_and_replays_preserve_history(cash: bool) {
    let f = Fixture::new().await;
    let credited_source = f.issued("issue-credit-source").await;
    let credit_request = credit_input(&credited_source.id, "credit");
    let credit = invoicing::credit_invoice(&f.pool, &f.workspace, &credit_request)
        .await
        .expect("invoice-method credit");
    let staged_invoice = f.issued("issue-staged").await;
    let imported = f.csv().await;
    let staged_request = staged_input(&staged_invoice.id, &imported.first_staged_transaction_id);
    let matched =
        reconciliation::reconciliation_match_create(&f.pool, &f.workspace, &staged_request)
            .await
            .expect("invoice-method staged payment");
    let direct_invoice = f.issued("issue-direct").await;
    let document = f.evidence().await;
    let direct_request = direct_input(&direct_invoice.id, &document.id);
    let recorded = reconciliation::invoice_payment_record(&f.pool, &f.workspace, &direct_request)
        .await
        .expect("invoice-method direct payment");
    let vat = f.approve_vat().await;
    let package = f.year_end_draft().await;
    let year_end_request = YearEndPackageApproveInput {
        package_id: package.id.clone(),
        idempotency_key: "year-end-approve".into(),
    };
    year_end::year_end_package_approve(&f.pool, &f.workspace, &year_end_request)
        .await
        .expect("invoice-method year-end approval");
    if cash {
        f.legacy_cash().await;
    } else {
        f.missing_method().await;
    }
    let before = database_snapshot(&f.pool).await;
    let files = artifact_snapshot(f.dir.path());

    for (invoice, key) in [
        (&staged_invoice, "issue-staged"),
        (&direct_invoice, "issue-direct"),
    ] {
        assert_eq!(
            invoicing::issue_invoice(&f.pool, &f.workspace, &f.issue_input(&invoice.id, key))
                .await
                .expect("issue replay after cash conversion")
                .id,
            invoice.id
        );
        let history = invoicing::get_invoice(&f.pool, &f.workspace, &invoice.id)
            .await
            .expect("historical invoice read");
        assert_eq!(history.voucher_id, invoice.voucher_id);
        assert_eq!(history.total_inc_vat_minor, 125_000);
        let voucher = ledger::voucher_get(
            &f.pool,
            &f.workspace,
            &ledger::VoucherGetInput {
                voucher_id: history.voucher_id.unwrap(),
            },
        )
        .await
        .expect("historical voucher read");
        assert!(!voucher.lines.is_empty());
    }
    assert_eq!(
        invoicing::credit_invoice(&f.pool, &f.workspace, &credit_request)
            .await
            .expect("credit replay")
            .id,
        credit.id
    );
    assert_eq!(
        invoicing::credit_invoice(
            &f.pool,
            &f.workspace,
            &credit_input(&credited_source.id, "credit-new-key")
        )
        .await
        .expect("existing credit with a new key is not a new posting")
        .id,
        credit.id
    );
    assert_eq!(
        reconciliation::reconciliation_match_create(&f.pool, &f.workspace, &staged_request)
            .await
            .expect("staged replay")
            .match_id,
        matched.match_id
    );
    assert_eq!(
        reconciliation::invoice_payment_record(&f.pool, &f.workspace, &direct_request)
            .await
            .expect("direct replay")
            .voucher_id,
        recorded.voucher_id
    );
    assert_eq!(
        vat::vat_return_approve(
            &f.pool,
            &f.workspace,
            &VatReturnApproveInput {
                reviewed_revision: vat.review_revision.clone().expect("review revision"),
                vat_return_id: vat.id.clone(),
                idempotency_key: "vat-approve".into()
            }
        )
        .await
        .expect("VAT approval replay")
        .id,
        vat.id
    );
    assert_eq!(
        year_end::year_end_package_approve(&f.pool, &f.workspace, &year_end_request)
            .await
            .expect("year-end approval replay")
            .status,
        "approved"
    );
    assert_eq!(
        before,
        database_snapshot(&f.pool).await,
        "completed same-key replays mutate nothing"
    );
    assert_eq!(
        vat::vat_return_approve(
            &f.pool,
            &f.workspace,
            &VatReturnApproveInput {
                reviewed_revision: vat.review_revision.clone().expect("review revision"),
                vat_return_id: vat.id.clone(),
                idempotency_key: "vat-approved-new-key".into()
            }
        )
        .await
        .expect("existing VAT approval with a new key")
        .status,
        "approved"
    );
    let binding = sqlx::query("SELECT id, status, payload_json FROM local_jobs WHERE workspace_id = ?1 AND job_type = 'vat_return_approve' AND idempotency_key = 'vat-approved-new-key'")
        .bind(&f.workspace).fetch_one(&f.pool).await.expect("fresh successful key must be bound");
    assert_eq!(binding.get::<String, _>("status"), "succeeded");
    let payload: serde_json::Value =
        serde_json::from_str(&binding.get::<String, _>("payload_json")).unwrap();
    assert_eq!(payload["vatReturnId"], vat.id);
    assert_eq!(
        payload["reviewedRevision"],
        vat.review_revision.clone().unwrap()
    );
    let binding_id: String = binding.get("id");
    let after_binding = database_snapshot(&f.pool).await;
    vat::vat_return_approve(
        &f.pool,
        &f.workspace,
        &VatReturnApproveInput {
            vat_return_id: vat.id.clone(),
            reviewed_revision: vat.review_revision.clone().expect("review revision"),
            idempotency_key: "vat-approved-new-key".into(),
        },
    )
    .await
    .expect("fresh key now replays without mutation");
    assert_eq!(after_binding, database_snapshot(&f.pool).await);
    assert_eq!(
        year_end::year_end_package_approve(
            &f.pool,
            &f.workspace,
            &YearEndPackageApproveInput {
                package_id: package.id.clone(),
                idempotency_key: "year-end-approved-new-key".into()
            }
        )
        .await
        .expect("existing year-end approval with a new key")
        .status,
        "approved"
    );
    assert_eq!(
        vat::vat_return_get(
            &f.pool,
            &f.workspace,
            &vat::VatReturnGetInput {
                vat_return_id: vat.id
            }
        )
        .await
        .expect("approved VAT history")
        .status,
        "approved"
    );
    assert_eq!(
        year_end::year_end_package_get(
            &f.pool,
            &f.workspace,
            &year_end::YearEndPackageGetInput {
                package_id: package.id
            }
        )
        .await
        .expect("approved year-end history")
        .status,
        "approved"
    );
    documents::verify_retained_document(&f.pool, &f.workspace, &document.id)
        .await
        .expect("retained historical evidence");
    let mut after = database_snapshot(&f.pool).await;
    let jobs = &mut after
        .iter_mut()
        .find(|(table, _)| table == "local_jobs")
        .unwrap()
        .1;
    let count = jobs.len();
    jobs.retain(|row| !row.starts_with(&format!("'{}'|", binding_id)));
    assert_eq!(
        jobs.len() + 1,
        count,
        "exclude exactly the named new replay binding"
    );
    assert_eq!(
        before, after,
        "all other jobs, proofs, ledger, audits and locks remain exact"
    );
    assert_eq!(files, artifact_snapshot(f.dir.path()));
}

#[tokio::test]
async fn invoice_method_positive_workflow_and_legacy_cash_replays_preserve_history() {
    positive_workflow_and_replays_preserve_history(true).await;
}

#[tokio::test]
async fn missing_method_does_not_block_existing_replays_or_history() {
    positive_workflow_and_replays_preserve_history(false).await;
}

#[tokio::test]
async fn already_issued_new_key_retry_under_cash_does_not_post_again_or_rewrite_history() {
    let f = Fixture::new().await;
    let issued = f.issued("original-issue").await;
    f.legacy_cash().await;
    let mut before = database_snapshot(&f.pool).await;
    let files = artifact_snapshot(f.dir.path());
    let original_jobs: Vec<(String, String)> =
        sqlx::query_as("SELECT id, payload_json FROM local_jobs ORDER BY id")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    let retry = invoicing::issue_invoice(
        &f.pool,
        &f.workspace,
        &f.issue_input(&issued.id, "new-key-no-new-posting"),
    )
    .await
    .expect("already-issued retry remains available");
    assert_eq!(retry.id, issued.id);
    assert_eq!(retry.voucher_id, issued.voucher_id);
    let mut after = database_snapshot(&f.pool).await;
    // This established retry path may cache the new key, but may not change
    // anything else or replace any previously stored replay payload.
    before.retain(|(table, _)| table != "local_jobs");
    after.retain(|(table, _)| table != "local_jobs");
    assert_eq!(before, after);
    for (id, payload) in original_jobs {
        let stored: String =
            sqlx::query_scalar("SELECT payload_json FROM local_jobs WHERE id = ?1")
                .bind(id)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        assert_eq!(stored, payload);
    }
    assert_eq!(files, artifact_snapshot(f.dir.path()));
}

#[tokio::test]
async fn invoice_posting_requires_validated_context_but_reversals_remain_available() {
    let f = Fixture::new().await;
    let invoice = f.draft().await;
    let error = ledger::post_invoice_voucher(
        &f.pool,
        &f.workspace,
        &f.fiscal_year(),
        &invoice.id,
        "2026-02-15",
        &buckets(),
    )
    .await
    .expect_err("method alone cannot authorize low-level invoice posting");
    assert_eq!(error.details.unwrap()[0].field.as_deref(), Some("eligibility"));
    let issued = f.issued("reviewed-low-level-source").await;
    let voucher = issued.voucher_id.unwrap();
    let credit = f.draft().await;
    ledger::post_reversal_voucher(
        &f.pool,
        &f.workspace,
        &f.fiscal_year(),
        &credit.id,
        &voucher,
        "2026-02-15",
        &buckets(),
    )
    .await
    .expect("invoice-method low-level reversal");
    let invoice_tx = f.draft().await;
    let credit_tx = f.draft().await;
    let mut tx = f.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let error = ledger::post_invoice_voucher_tx(
        &mut tx,
        &f.workspace,
        &f.fiscal_year(),
        &invoice_tx.id,
        "2026-02-15",
        &buckets(),
    )
    .await
    .expect_err("tx entrypoint also needs validated pending context");
    assert_eq!(error.details.unwrap()[0].field.as_deref(), Some("eligibility"));
    tx.rollback().await.unwrap();
    let issued_tx = f.issued("reviewed-low-level-tx-source").await;
    assert!(issued_tx.voucher_id.is_some());
    let mut tx = f.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    ledger::post_reversal_voucher_tx(
        &mut tx,
        &f.workspace,
        &f.fiscal_year(),
        &credit_tx.id,
        "2026-02-15",
        &buckets(),
    )
    .await
    .expect("invoice-method reversal tx");
    tx.commit().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vouchers WHERE status = 'posted'")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 4);
    assert_eq!(
        ledger::net_revenue_minor(&f.pool, &f.workspace)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ledger::net_output_vat_minor(&f.pool, &f.workspace)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn issue_waiting_for_writer_rechecks_committed_method_inside_transaction() {
    let f = Fixture::new().await;
    let draft = f.draft().await;
    // A separate real connection permits preflight reads while BEGIN IMMEDIATE
    // holds the writer lock. The mutation must not trust a pre-lock method read.
    let writer_pool = connect_workspace(&f.dir.path().join("workspace.sqlite"))
        .await
        .unwrap();
    let mut writer = writer_pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let mut before = database_snapshot(&f.pool).await;
    before.retain(|(table, _)| table != "vat_profiles");
    let files = artifact_snapshot(f.dir.path());
    let pool = f.pool.clone();
    let workspace = f.workspace.clone();
    let input = common::reviewed_input(&f.pool, &f.workspace, &f.issue_input(&draft.id, "racing-issue")).await.unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let mutation = tokio::spawn(async move {
        started_tx.send(()).unwrap();
        invoicing::issue_invoice(&pool, &workspace, &input).await
    });
    started_rx.await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(
        !mutation.is_finished(),
        "issue should be waiting behind the writer lock"
    );
    sqlx::query(
        "UPDATE vat_profiles SET accounting_method = 'cash_method' WHERE workspace_id = ?1",
    )
    .bind(&f.workspace)
    .execute(&mut *writer)
    .await
    .unwrap();
    writer.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), mutation)
        .await
        .expect("issue must finish after writer commits")
        .unwrap();
    let mut after = database_snapshot(&f.pool).await;
    after.retain(|(table, _)| table != "vat_profiles");
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("racing issue unexpectedly succeeded after cash method committed (other database content unchanged: {})", before == after),
    };
    assert_method_error(&error, true);
    assert_eq!(before, after);
    assert_eq!(files, artifact_snapshot(f.dir.path()));
    assert_eq!(
        profiles::get_vat_profile(&f.pool, &f.workspace)
            .await
            .unwrap()
            .unwrap()
            .accounting_method,
        "cash_method"
    );
}

// Dedicated additive cash identities use public services, never historical SQL
// seeding or changes to the old seven-fact invoice contract.
use invoicing::cash::{
    self, CashBusinessOriginFacts, CashIssueFacts, CashIssueInput, CashIssuePreflightInput,
};
fn cash_facts() -> CashIssueFacts {
    CashIssueFacts {
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
        short_year_turnover_adjustment_reviewed: None,
    }
}
fn cash_origin(start: &str) -> CashBusinessOriginFacts {
    CashBusinessOriginFacts {
        new_business_started_in_2026: Some(true),
        business_started_on: Some(start.into()),
        first_financial_year_ends_on: Some("2026-12-31".into()),
        zero_opening_balances_confirmed: Some(true),
        no_opening_unpaid_invoices_or_bills_confirmed: Some(true),
        no_prior_unrecorded_activity_confirmed: Some(true),
        outside_workspace_history_reviewed: Some(true),
    }
}
async fn cash_review(f: &Fixture) -> CashIssueInput {
    profiles::save_vat_profile(&f.pool, &f.workspace, &vat_profile("cash_method"))
        .await
        .unwrap();
    let draft = f.draft().await;
    let preflight = CashIssuePreflightInput {
        invoice_id: draft.id.clone(),
        issue_date: "2026-01-20".into(),
        facts: cash_facts(),
        origin_review: Some(cash_origin("2026-01-01")),
    };
    let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &preflight)
        .await
        .unwrap();
    CashIssueInput {
        invoice_id: draft.id,
        issue_date: preflight.issue_date,
        facts: preflight.facts,
        origin_review: preflight.origin_review,
        idempotency_key: "cash-first-identity".into(),
        review_fingerprint: review.assessment.review_fingerprint,
    }
}
#[tokio::test]
async fn cash_identity_freezes_both_serialized_profile_save_routes_without_voucher() {
    for route in [SaveRoute::Individual, SaveRoute::Onboarding] {
        let f = Fixture::new().await;
        let input = cash_review(&f).await;
        cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
            .await
            .unwrap();
        let before = database_snapshot(&f.pool).await;
        let error = save_method(&f, route, "invoice_method")
            .await
            .expect_err("cash identity freezes method before receipt voucher");
        assert_eq!(
            error.details.unwrap()[0].field.as_deref(),
            Some("accountingMethod")
        );
        assert_eq!(before, database_snapshot(&f.pool).await);
        save_method(&f, route, "cash_method").await.unwrap();
    }
}
#[tokio::test]
async fn cash_identity_cannot_use_legacy_settlement_or_credit_after_external_method_edit() {
    let f = Fixture::new().await;
    let input = cash_review(&f).await;
    cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    let document = f.evidence().await;
    f.csv().await;
    let staged = imports::staged_transactions_list(
        &f.pool,
        &f.workspace,
        &StagedTransactionsListInput {
            status: Some("staged".into()),
            limit: Some(100),
            before_id: None,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE vat_profiles SET accounting_method='invoice_method' WHERE workspace_id=?1")
        .bind(&f.workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    let before = database_snapshot(&f.pool).await;
    for error in [
        reconciliation::invoice_payment_record(
            &f.pool,
            &f.workspace,
            &direct_input(&input.invoice_id, &document.id),
        )
        .await
        .unwrap_err(),
        reconciliation::reconciliation_match_create(
            &f.pool,
            &f.workspace,
            &staged_input(&input.invoice_id, &staged[0].id),
        )
        .await
        .unwrap_err(),
        invoicing::credit_invoice(
            &f.pool,
            &f.workspace,
            &credit_input(&input.invoice_id, "cash-credit"),
        )
        .await
        .unwrap_err(),
    ] {
        assert_eq!(
            error.details.unwrap()[0].field.as_deref(),
            Some("accountingMethod")
        );
    }
    assert_eq!(before, database_snapshot(&f.pool).await);
}
#[tokio::test]
async fn cash_short_first_year_needs_adjustment_review_and_persists_real_start_atomically() {
    let f = Fixture::new().await;
    let mut input = cash_review(&f).await;
    input.origin_review = Some(cash_origin("2026-01-05"));
    let mut preflight = CashIssuePreflightInput {
        invoice_id: input.invoice_id.clone(),
        issue_date: input.issue_date.clone(),
        facts: input.facts.clone(),
        origin_review: input.origin_review.clone(),
    };
    let before = database_snapshot(&f.pool).await;
    assert!(
        cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &preflight)
            .await
            .is_err()
    );
    assert_eq!(before, database_snapshot(&f.pool).await);
    preflight.facts.short_year_turnover_adjustment_reviewed = Some(true);
    let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &preflight)
        .await
        .unwrap();
    assert_eq!(
        before,
        database_snapshot(&f.pool).await,
        "preflight must not configure first year"
    );
    input.facts = preflight.facts;
    input.review_fingerprint = review.assessment.review_fingerprint;
    cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    let start: String = sqlx::query_scalar("SELECT starts_on FROM fiscal_years WHERE id=?1")
        .bind(f.fiscal_year())
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(start, "2026-01-05");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vouchers WHERE workspace_id=?1")
        .bind(&f.workspace)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn cash_unknown_or_contradictory_facts_and_nonempty_origin_are_readonly_rejections() {
    let f = Fixture::new().await;
    let input = cash_review(&f).await;
    let base = serde_json::to_value(CashIssuePreflightInput {
        invoice_id: input.invoice_id.clone(),
        issue_date: input.issue_date.clone(),
        facts: input.facts.clone(),
        origin_review: input.origin_review.clone(),
    })
    .unwrap();
    let before = database_snapshot(&f.pool).await;
    for section in ["facts", "originReview"] {
        for (name, good) in base[section].as_object().unwrap() {
            if name == "shortYearTurnoverAdjustmentReviewed" {
                continue;
            }
            let mut bads = vec![serde_json::Value::Null];
            if let Some(b) = good.as_bool() {
                bads.push((!b).into());
            }
            for bad in bads {
                let mut candidate = base.clone();
                candidate[section][name] = bad;
                let candidate: CashIssuePreflightInput = serde_json::from_value(candidate).unwrap();
                assert!(
                    cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &candidate)
                        .await
                        .is_err(),
                    "{section}.{name}"
                );
                assert_eq!(before, database_snapshot(&f.pool).await);
            }
        }
    }
    f.expense().await;
    let before = database_snapshot(&f.pool).await;
    let candidate: CashIssuePreflightInput = serde_json::from_value(base).unwrap();
    assert!(
        cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &candidate)
            .await
            .is_err(),
        "posted expense prevents falsely claiming empty origin"
    );
    assert_eq!(before, database_snapshot(&f.pool).await);
}
#[tokio::test]
async fn cash_completed_replay_precedes_profile_rules_clock_and_rejects_changed_input() {
    let f = Fixture::new().await;
    let input = cash_review(&f).await;
    let issued = cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    sqlx::query("DELETE FROM vat_profiles WHERE workspace_id=?1")
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
    let before = database_snapshot(&f.pool).await;
    let replay = cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(issued).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    for field in ["issueDate", "facts", "reviewFingerprint", "invoiceId"] {
        let mut changed = serde_json::to_value(&input).unwrap();
        match field {
            "facts" => changed["facts"]["priorPaymentReceived"] = true.into(),
            "issueDate" => changed[field] = "2027-01-01".into(),
            _ => changed[field] = "changed".into(),
        };
        let changed: CashIssueInput = serde_json::from_value(changed).unwrap();
        assert!(cash::invoice_cash_issue(&f.pool, &f.workspace, &changed)
            .await
            .is_err());
    }
    let history = cash::invoice_cash_history_get(
        &f.pool,
        &f.workspace,
        &cash::CashHistoryGetInput {
            invoice_id: input.invoice_id.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        history.issue.assessment.review_fingerprint,
        input.review_fingerprint
    );
    assert_eq!(before, database_snapshot(&f.pool).await);
}
#[tokio::test]
async fn cash_writer_rechecks_committed_profile_and_reviewed_customer_before_any_write() {
    for edit in [
        "UPDATE vat_profiles SET accounting_method='invoice_method' WHERE workspace_id=?1",
        "UPDATE counterparties SET name='Changed customer' WHERE workspace_id=?1",
    ] {
        let f = Fixture::new().await;
        let input = cash_review(&f).await;
        let writer_pool = connect_workspace(&f.dir.path().join("workspace.sqlite"))
            .await
            .unwrap();
        let mut writer = writer_pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let pool = f.pool.clone();
        let workspace = f.workspace.clone();
        let task =
            tokio::spawn(async move { cash::invoice_cash_issue(&pool, &workspace, &input).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        sqlx::query(edit)
            .bind(&f.workspace)
            .execute(&mut *writer)
            .await
            .unwrap();
        writer.commit().await.unwrap();
        let before = database_snapshot(&writer_pool).await;
        assert!(tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        assert_eq!(before, database_snapshot(&writer_pool).await);
    }
}
#[tokio::test]
async fn cash_concurrent_duplicate_issue_is_one_identity_and_continuation_counts_unpaid_once() {
    let f = Fixture::new().await;
    let input = cash_review(&f).await;
    let pool2 = connect_workspace(&f.dir.path().join("workspace.sqlite"))
        .await
        .unwrap();
    let (one, two) = tokio::join!(
        cash::invoice_cash_issue(&f.pool, &f.workspace, &input),
        cash::invoice_cash_issue(&pool2, &f.workspace, &input)
    );
    assert_eq!(one.unwrap().invoice_number, two.unwrap().invoice_number);
    f.expense().await;
    let draft = f.draft().await;
    let review = cash::invoice_cash_issue_preflight(
        &f.pool,
        &f.workspace,
        &CashIssuePreflightInput {
            invoice_id: draft.id,
            issue_date: "2026-01-20".into(),
            facts: cash_facts(),
            origin_review: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(review.current_taxable_base_minor, 100_000);
    assert_eq!(review.projected_taxable_base_minor, 200_000);
    let (origins,issues,jobs):(i64,i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM cash_business_origins),(SELECT COUNT(*) FROM cash_invoice_issues),(SELECT COUNT(*) FROM local_jobs WHERE job_type='invoice_pdf_generate')").fetch_one(&f.pool).await.unwrap();
    assert_eq!((origins, issues, jobs), (1, 1, 1));
}

#[tokio::test]
async fn cash_first_issue_rolls_back_origin_year_identity_and_all_jobs_on_late_failure() {
    let f = Fixture::new().await;
    let mut input = cash_review(&f).await;
    input.origin_review = Some(cash_origin("2026-01-05"));
    input.facts.short_year_turnover_adjustment_reviewed = Some(true);
    let review = cash::invoice_cash_issue_preflight(
        &f.pool,
        &f.workspace,
        &CashIssuePreflightInput {
            invoice_id: input.invoice_id.clone(),
            issue_date: input.issue_date.clone(),
            facts: input.facts.clone(),
            origin_review: input.origin_review.clone(),
        },
    )
    .await
    .unwrap();
    input.review_fingerprint = review.assessment.review_fingerprint;
    sqlx::query("CREATE TRIGGER inject_cash_issue_audit_failure BEFORE INSERT ON audit_events WHEN NEW.action='invoice_issue' BEGIN SELECT RAISE(ABORT,'injected cash issue audit failure'); END;").execute(&f.pool).await.unwrap();
    let before = database_snapshot(&f.pool).await;
    assert!(cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
        .await
        .is_err());
    assert_eq!(
        before,
        database_snapshot(&f.pool).await,
        "including real fiscal-year start, origin, number, snapshot, proof and replay"
    );
    assert!(cash::cash_business_origin_get(&f.pool, &f.workspace)
        .await
        .unwrap()
        .is_none());
    assert!(cash::invoice_cash_history_get(
        &f.pool,
        &f.workspace,
        &cash::CashHistoryGetInput {
            invoice_id: input.invoice_id
        }
    )
    .await
    .unwrap()
    .is_none());
}
#[tokio::test]
async fn cash_current_rule_header_line_calendar_and_lock_changes_invalidate_review() {
    let edits=[
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.scope.annualTaxableBaseLimitMinor',200000000) WHERE rule_version_id='rv-2026-method-recognition-v2' AND family='cash_recognition'",
        "UPDATE tax_rules SET value_json=json_set(value_json,'$.schedules.annual_may_12.\"2026\"','2026-05-12') WHERE rule_version_id='rv-2026-method-recognition-v2' AND key='filing_deadline_schedules'",
        "UPDATE invoices SET total_vat_minor=1 WHERE workspace_id=?1",
        "UPDATE invoice_lines SET unit_price_minor=100001 WHERE invoice_id IN(SELECT id FROM invoices WHERE workspace_id=?1)",
        "UPDATE invoices SET due_date='2026-05-05' WHERE workspace_id=?1",
        "UPDATE sole_trader_profiles SET residency_country='NO' WHERE workspace_id=?1",
        "UPDATE fiscal_years SET status='closing' WHERE workspace_id=?1",
    ];
    for edit in edits {
        let f = Fixture::new().await;
        let input = cash_review(&f).await;
        sqlx::query(edit)
            .bind(&f.workspace)
            .execute(&f.pool)
            .await
            .unwrap();
        let before = database_snapshot(&f.pool).await;
        assert!(
            cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
                .await
                .is_err(),
            "{edit}"
        );
        assert_eq!(before, database_snapshot(&f.pool).await, "{edit}");
    }
}
#[tokio::test]
async fn cash_issue_keeps_receipt_service_closed_and_provenance_append_only() {
    let f = Fixture::new().await;
    let input = cash_review(&f).await;
    cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    let before = database_snapshot(&f.pool).await;
    for table in ["cash_business_origins", "cash_invoice_issues"] {
        assert!(sqlx::query(&format!(
            "UPDATE {table} SET facts_json='{{}}' WHERE workspace_id=?1"
        ))
        .bind(&f.workspace)
        .execute(&f.pool)
        .await
        .is_err());
        assert!(
            sqlx::query(&format!("DELETE FROM {table} WHERE workspace_id=?1"))
                .bind(&f.workspace)
                .execute(&f.pool)
                .await
                .is_err()
        );
    }
    let receipt=cash::CashReceiptRecordInput {invoice_id:input.invoice_id,document_id:"not-receipt-proof".into(),idempotency_key:"not-yet".into(),facts:serde_json::from_value(serde_json::json!({"actualReceivedAmountMinor":125000,"actualReceivedOn":"2026-02-15","currency":"SEK","receivedAlready":true,"bankReceiptIdentity":{"bankId":"bank","receivingAccountReference":"business-account","transactionReference":"actual-entry"},"sourceEntryLocator":"page 1","bankPdfMatchesReceiptConfirmed":true,"issuedCaseStillAppliesConfirmed":true,"noOtherReceiptsOrAdjustmentsConfirmed":true})).unwrap()};
    assert!(
        cash::invoice_cash_receipt_record(&f.pool, &f.workspace, &receipt)
            .await
            .is_err()
    );
    assert_eq!(before, database_snapshot(&f.pool).await);
}

#[tokio::test]
async fn cash_rechecks_fa_skatt_salary_posture_even_after_external_profile_edit() {
    let f = Fixture::new().await;
    let input = cash_review(&f).await;
    sqlx::query("UPDATE tax_profiles SET tax_status='fa_skatt',expected_salary_income_minor=0 WHERE workspace_id=?1")
        .bind(&f.workspace).execute(&f.pool).await.unwrap();
    let before = database_snapshot(&f.pool).await;
    let candidate = CashIssuePreflightInput {invoice_id: input.invoice_id.clone(),issue_date:input.issue_date.clone(),facts:input.facts.clone(),origin_review:input.origin_review.clone()};
    assert!(cash::invoice_cash_issue_preflight(&f.pool,&f.workspace,&candidate).await.is_err(), "stored FA-skatt posture must remain valid, not merely retain a status string");
    assert_eq!(before,database_snapshot(&f.pool).await);
}

#[tokio::test]
async fn cash_immutable_boundary_blocks_vat_approval_after_external_profile_drift() {
    let f = Fixture::new().await;
    let input = cash_review(&f).await;
    cash::invoice_cash_issue(&f.pool, &f.workspace, &input)
        .await
        .unwrap();
    let draft = vat::vat_return_draft_create(
        &f.pool,
        &f.workspace,
        &VatReturnDraftCreateInput {
            period_key: "2026".into(),
            idempotency_key: "cash-provisional-vat".into(),
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE vat_profiles SET accounting_method='invoice_method' WHERE workspace_id=?1")
        .bind(&f.workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    let before = database_snapshot(&f.pool).await;
    let result = vat::vat_return_approve(
        &f.pool,
        &f.workspace,
        &VatReturnApproveInput {
            reviewed_revision: draft.review_revision.clone().expect("review revision"),
            vat_return_id: draft.id,
            idempotency_key: "cash-must-not-approve".into(),
        },
    )
    .await;
    let error = result.expect_err(
        "an externally edited profile cannot promote cash books to validated VAT approval",
    );
    assert_method_error(&error, true);
    assert_eq!(database_snapshot(&f.pool).await, before);
}

#[tokio::test]
async fn cash_immutable_boundary_rejects_expense_before_real_first_year_start() {
    let f = Fixture::new().await;
    let mut input = cash_review(&f).await;
    input.origin_review = Some(cash_origin("2026-01-05"));
    input.facts.short_year_turnover_adjustment_reviewed = Some(true);
    let review = cash::invoice_cash_issue_preflight(&f.pool, &f.workspace, &CashIssuePreflightInput {
        invoice_id: input.invoice_id.clone(), issue_date: input.issue_date.clone(),
        facts: input.facts.clone(), origin_review: input.origin_review.clone(),
    }).await.unwrap();
    input.review_fingerprint = review.assessment.review_fingerprint;
    cash::invoice_cash_issue(&f.pool, &f.workspace, &input).await.unwrap();
    let before = database_snapshot(&f.pool).await;
    let result = expenses::expense_post(&f.pool, &f.workspace, &ExpensePostInput {
        amount_minor_ex_vat: 10_000, vat_rate: 0.25,
        expense_account_number: "5610".into(), payment_account_number: "1930".into(),
        document_id: None, no_document_reason: Some("Disposable boundary regression".into()),
        staged_transaction_id: None, idempotency_key: "expense-outside-real-first-year".into(),
        date: Some("2026-01-02".into()),
    }).await;
    assert!(result.is_err(), "a January calendar bucket must not permit posting before the actual January 5 business/fiscal-year start");
    assert_eq!(database_snapshot(&f.pool).await, before);
}
