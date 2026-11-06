//! Preparation readiness is not cash recognition or unpaid-closing validation.
use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    profiles::{self, VatProfileSaveInput},
    workspace::{bootstrap_accounting_defaults, ensure_workspace_ready},
    year_end::{year_end_readiness_get, YearEndReadinessInput},
};
use sqlx::SqlitePool;
use tempfile::{tempdir, TempDir};
use uuid::Uuid;

struct Fixture {
    _dir: TempDir,
    pool: SqlitePool,
    workspace: String,
}

impl Fixture {
    async fn new(method: &str) -> Self {
        let dir = tempdir().unwrap();
        let database = dir.path().join("workspace.sqlite");
        let pool = connect_workspace(&database).await.unwrap();
        let workspace = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES(?1,'Cash closing readiness',?2,?3,?4)")
            .bind(&workspace).bind(database.to_string_lossy().as_ref())
            .bind(dir.path().join("documents").to_string_lossy().as_ref())
            .bind(dir.path().join("exports").to_string_lossy().as_ref())
            .execute(&pool).await.unwrap();
        ensure_workspace_ready(&pool, &workspace).await.unwrap();
        bootstrap_accounting_defaults(&pool, &workspace, 2026)
            .await
            .unwrap();
        profiles::save_vat_profile(
            &pool,
            &workspace,
            &VatProfileSaveInput {
                vat_status: "registered".into(),
                reporting_period: "yearly".into(),
                accounting_method: method.into(),
                voluntary_registration_date: None,
                vat_filing_deadline_regime: Some("annual_may_12".into()),
            },
        )
        .await
        .unwrap();
        Self {
            _dir: dir,
            pool,
            workspace,
        }
    }

    async fn mutation_counts(&self) -> Vec<i64> {
        let mut counts = Vec::new();
        for table in [
            "vouchers",
            "journal_lines",
            "audit_events",
            "local_jobs",
            "fiscal_years",
            "fiscal_periods",
            "vat_returns",
            "year_end_packages",
        ] {
            counts.push(
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                    .fetch_one(&self.pool)
                    .await
                    .unwrap(),
            );
        }
        counts
    }
}

#[tokio::test]
async fn even_empty_cash_books_report_unvalidated_closing_without_writes() {
    let fixture = Fixture::new("cash_method").await;
    let before = fixture.mutation_counts().await;
    let readiness = year_end_readiness_get(
        &fixture.pool,
        &fixture.workspace,
        &YearEndReadinessInput { fiscal_year: 2026 },
    )
    .await
    .unwrap();
    let cash = readiness.items.iter().find(|item| item.code == "cash_preparation_unvalidated")
        .expect("cash books must explicitly disclose unsupported preparation even with zero balances or receipts");
    assert!(!cash.satisfied);
    assert!(!readiness.ready_to_approve);
    assert_eq!(
        fixture.mutation_counts().await,
        before,
        "readiness never creates recognition, filing or closing state"
    );
}

#[tokio::test]
async fn invoice_method_books_do_not_acquire_a_cash_preparation_blocker() {
    let fixture = Fixture::new("invoice_method").await;
    let before = fixture.mutation_counts().await;
    let readiness = year_end_readiness_get(
        &fixture.pool,
        &fixture.workspace,
        &YearEndReadinessInput { fiscal_year: 2026 },
    )
    .await
    .unwrap();
    assert!(!readiness
        .items
        .iter()
        .any(|item| item.code == "cash_preparation_unvalidated"));
    assert_eq!(fixture.mutation_counts().await, before);
}
