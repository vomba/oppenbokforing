use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    profiles::{self, VatProfileSaveInput},
    vat::{self, VatReturnApproveInput, VatReturnDraftCreateInput, VatReturnGetInput},
    workspace::ensure_workspace_ready,
};
use sqlx::{migrate::Migrator, sqlite::SqliteConnectOptions, SqlitePool};
use std::borrow::Cow;
use tempfile::tempdir;

#[tokio::test]
async fn forward_migration_preserves_seeded_history_and_legacy_drafts_require_fresh_review() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite");
    let prior = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    let migrations = sqlx::migrate!("./migrations");
    let previous = Migrator {
        migrations: Cow::Owned(
            migrations
                .iter()
                .filter(|m| m.version < 202609180004)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    previous.run(&prior).await.unwrap();
    sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES ('w','legacy',?1,'documents','exports')")
        .bind(path.to_string_lossy().as_ref()).execute(&prior).await.unwrap();
    sqlx::query("INSERT INTO fiscal_years(id,workspace_id,starts_on,ends_on,status) VALUES ('fy-w-2026','w','2026-01-01','2026-12-31','open')")
        .execute(&prior).await.unwrap();
    for (period, start, end, status, return_id, return_status) in [
        (
            "2026-Q1",
            "2026-01-01",
            "2026-03-31",
            "open",
            "legacy-draft",
            "draft",
        ),
        (
            "2026-Q2",
            "2026-04-01",
            "2026-06-30",
            "locked",
            "legacy-approved",
            "approved",
        ),
    ] {
        let period_id = if period == "2026-Q1" {
            "legacy-period-q1".to_string()
        } else {
            format!("fp-w-{period}")
        };
        sqlx::query("INSERT INTO fiscal_periods(id,workspace_id,fiscal_year_id,period_key,starts_on,ends_on,status) VALUES (?1,'w','fy-w-2026',?2,?3,?4,?5)")
            .bind(&period_id).bind(period).bind(start).bind(end).bind(status).execute(&prior).await.unwrap();
        sqlx::query("INSERT INTO vat_returns(id,workspace_id,fiscal_period_id,status,rule_version_id,approved_at) VALUES (?1,'w',?2,?3,'rv-2026-active',?4)")
            .bind(return_id).bind(&period_id).bind(return_status).bind(if return_status == "approved" { Some("2026-07-01") } else { None })
            .execute(&prior).await.unwrap();
        sqlx::query("INSERT INTO vat_return_boxes(id,vat_return_id,box_code,amount_minor,source_query_hash) VALUES (?1,?1,'49',1234,'legacy-query')")
            .bind(return_id).execute(&prior).await.unwrap();
    }
    sqlx::query("INSERT INTO audit_events(id,workspace_id,action,resource_type,resource_id,metadata_json) VALUES ('legacy-audit','w','vat_return_approved','vat_return','legacy-approved','{\"legacy\":true}')")
        .execute(&prior).await.unwrap();
    sqlx::query("INSERT INTO local_jobs(id,workspace_id,job_type,status,payload_json,idempotency_key) VALUES ('legacy-replay','w','vat_return_approve','succeeded','{\"vatReturnId\":\"legacy-approved\"}','legacy-key')")
        .execute(&prior).await.unwrap();
    prior.close().await;

    let pool = connect_workspace(&path).await.unwrap();
    let approved = vat::vat_return_get(
        &pool,
        "w",
        &VatReturnGetInput {
            vat_return_id: "legacy-approved".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(approved.status, "approved");
    assert_eq!(approved.box49_amount_minor, 1234);
    assert_eq!(approved.review_revision, None);
    let before: (String, String, String) = sqlx::query_as(
        "SELECT (SELECT metadata_json FROM audit_events WHERE id='legacy-audit'), (SELECT payload_json FROM local_jobs WHERE id='legacy-replay'), (SELECT status FROM fiscal_periods WHERE id='fp-w-2026-Q2')"
    ).fetch_one(&pool).await.unwrap();
    let error = vat::vat_return_approve(
        &pool,
        "w",
        &VatReturnApproveInput {
            vat_return_id: "legacy-draft".into(),
            reviewed_revision: "legacy-no-snapshot".into(),
            idempotency_key: "attempt".into(),
        },
    )
    .await
    .expect_err("legacy drafts cannot be approved");
    assert!(
        error.message.to_lowercase().contains("regenerat"),
        "{}",
        error.message
    );
    assert!(
        error.message.to_lowercase().contains("review"),
        "{}",
        error.message
    );
    let legacy = vat::vat_return_get(
        &pool,
        "w",
        &VatReturnGetInput {
            vat_return_id: "legacy-draft".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(legacy.status, "draft");
    assert_eq!(legacy.box49_amount_minor, 1234);
    assert_eq!(legacy.review_revision, None);
    let counts: (i64, i64, String) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM audit_events), (SELECT COUNT(*) FROM local_jobs), (SELECT status FROM fiscal_periods WHERE id='legacy-period-q1')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, "open".into()));
    let after: (String, String, String) = sqlx::query_as(
        "SELECT (SELECT metadata_json FROM audit_events WHERE id='legacy-audit'), (SELECT payload_json FROM local_jobs WHERE id='legacy-replay'), (SELECT status FROM fiscal_periods WHERE id='fp-w-2026-Q2')"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);

    ensure_workspace_ready(&pool, "w").await.unwrap();
    profiles::save_vat_profile(
        &pool,
        "w",
        &VatProfileSaveInput {
            vat_status: "registered".into(),
            reporting_period: "quarterly".into(),
            accounting_method: "invoice_method".into(),
            voluntary_registration_date: None,
            vat_filing_deadline_regime: Some("quarterly_12".into()),
        },
    )
    .await
    .unwrap();
    let regenerated = vat::vat_return_draft_create(
        &pool,
        "w",
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".into(),
            idempotency_key: "regenerate".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(regenerated.id, "legacy-draft");
    assert!(regenerated.review_revision.is_some());
    vat::vat_return_approve(
        &pool,
        "w",
        &VatReturnApproveInput {
            vat_return_id: regenerated.id,
            reviewed_revision: regenerated.review_revision.unwrap(),
            idempotency_key: "fresh-review".into(),
        },
    )
    .await
    .unwrap();
    let history = vat::vat_return_get(
        &pool,
        "w",
        &VatReturnGetInput {
            vat_return_id: "legacy-approved".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(approved).unwrap(),
        serde_json::to_value(history).unwrap()
    );
}
