//! Interim safety contract: selection is retained, but only invoice_method may
//! authorize new affected postings/approvals. Legacy books are simulated with SQL,
//! never by treating an ordinary post-posting profile save as a migration.
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
            invoice_id: invoice.into(),
            idempotency_key: key.into(),
            issue_date: Some("2026-01-20".into()),
        }
    }

    async fn issued(&self, key: &str) -> invoicing::InvoiceSummary {
        let draft = self.draft().await;
        invoicing::issue_invoice(
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
        vat::vat_return_approve(
            &f.pool,
            &f.workspace,
            &VatReturnApproveInput {
                vat_return_id: vat.id.clone(),
                idempotency_key: "vat-approved-new-key".into()
            }
        )
        .await
        .expect("existing VAT approval with a new key")
        .status,
        "approved"
    );
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
    assert_eq!(
        before,
        database_snapshot(&f.pool).await,
        "replays/history cannot rewrite jobs, audits, postings or locks"
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
async fn invoice_method_authorizes_both_low_level_ledger_entrypoints_and_tx_variants() {
    let f = Fixture::new().await;
    let invoice = f.draft().await;
    let voucher = ledger::post_invoice_voucher(
        &f.pool,
        &f.workspace,
        &f.fiscal_year(),
        &invoice.id,
        "2026-02-15",
        &buckets(),
    )
    .await
    .expect("invoice-method low-level invoice");
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
    ledger::post_invoice_voucher_tx(
        &mut tx,
        &f.workspace,
        &f.fiscal_year(),
        &invoice_tx.id,
        "2026-02-15",
        &buckets(),
    )
    .await
    .expect("invoice-method invoice tx");
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
    let input = f.issue_input(&draft.id, "racing-issue");
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
