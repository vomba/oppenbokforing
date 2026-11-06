use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    ledger::{self, VatBucket},
    profiles::{self, VatProfileSaveInput},
    vat::{
        self, VatReturnApproveInput, VatReturnDraftCreateInput, VatReturnGetInput,
        VatReturnSummary, VatReturnTraceInput,
    },
    workspace::ensure_workspace_ready,
};
use sqlx::SqlitePool;
use tempfile::{tempdir, TempDir};
use uuid::Uuid;

async fn setup() -> (TempDir, SqlitePool, String) {
    let dir = tempdir().unwrap();
    let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .unwrap();
    let workspace = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO workspaces (id,name,database_path,documents_path,exports_path) VALUES (?1,'VAT review',?2,?3,?4)")
        .bind(&workspace).bind(dir.path().join("workspace.sqlite").to_string_lossy().as_ref())
        .bind(dir.path().join("documents").to_string_lossy().as_ref())
        .bind(dir.path().join("exports").to_string_lossy().as_ref())
        .execute(&pool).await.unwrap();
    ensure_workspace_ready(&pool, &workspace).await.unwrap();
    profiles::save_vat_profile(
        &pool,
        &workspace,
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
    (dir, pool, workspace)
}

async fn draft(pool: &SqlitePool, workspace: &str, key: &str) -> VatReturnSummary {
    vat::vat_return_draft_create(
        pool,
        workspace,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".into(),
            idempotency_key: key.into(),
        },
    )
    .await
    .unwrap()
}

fn approval(draft: &VatReturnSummary, key: &str) -> VatReturnApproveInput {
    VatReturnApproveInput {
        vat_return_id: draft.id.clone(),
        reviewed_revision: draft.review_revision.clone().unwrap(),
        idempotency_key: key.into(),
    }
}

fn buckets() -> Vec<VatBucket> {
    vec![VatBucket {
        vat_code: "VAT25".into(),
        ex_vat_minor: 10_000,
        vat_minor: 2_500,
    }]
}

async fn post(
    pool: &SqlitePool,
    workspace: &str,
    source: &str,
) -> Result<String, oppenbokforing_desktop_lib::error::AppError> {
    ledger::post_invoice_voucher(
        pool,
        workspace,
        &format!("fy-{workspace}-2026"),
        source,
        "2026-02-01",
        &buckets(),
    )
    .await
}

// Include row identities, timestamps, locks, audit and replay records, not just amounts.
async fn approval_state(pool: &SqlitePool) -> Vec<String> {
    let mut state = Vec::new();
    for query in [
        "SELECT COALESCE(json_group_array(json_object('id',id,'period',fiscal_period_id,'status',status,'rule',rule_version_id,'approved',approved_at,'export',export_path,'revision',review_revision,'snapshot',review_snapshot_json,'snapshotHash',review_snapshot_hash)), '[]') FROM (SELECT * FROM vat_returns ORDER BY id)",
        "SELECT COALESCE(json_group_array(json_object('id',id,'return',vat_return_id,'box',box_code,'amount',amount_minor,'hash',source_query_hash)), '[]') FROM (SELECT * FROM vat_return_boxes ORDER BY id)",
        "SELECT COALESCE(json_group_array(json_object('id',id,'status',status)), '[]') FROM (SELECT * FROM fiscal_periods ORDER BY id)",
        "SELECT COALESCE(json_group_array(json_object('id',id,'metadata',metadata_json)), '[]') FROM (SELECT * FROM audit_events WHERE action = 'vat_return_approved' ORDER BY id)",
        "SELECT COALESCE(json_group_array(json_object('id',id,'payload',payload_json,'key',idempotency_key)), '[]') FROM (SELECT * FROM local_jobs WHERE job_type = 'vat_return_approve' ORDER BY id)",
    ] {
        state.push(sqlx::query_scalar(query).fetch_one(pool).await.unwrap());
    }
    state
}

async fn assert_stale_unchanged(pool: &SqlitePool, workspace: &str, input: &VatReturnApproveInput) {
    let before = approval_state(pool).await;
    let error = vat::vat_return_approve(pool, workspace, input)
        .await
        .expect_err("must require fresh review");
    assert_eq!(error.code, "validation_error");
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
    assert_eq!(
        before,
        approval_state(pool).await,
        "stale rejection must not mutate approval state"
    );
}

#[tokio::test]
async fn unchanged_approval_preserves_exact_reviewed_rows_and_trace_and_binds_replay() {
    let (_dir, pool, workspace) = setup().await;
    post(&pool, &workspace, "sale").await.unwrap();
    let reviewed = draft(&pool, &workspace, "draft").await;
    let trace = vat::vat_return_trace(
        &pool,
        &workspace,
        &VatReturnTraceInput {
            vat_return_id: reviewed.id.clone(),
        },
    )
    .await
    .unwrap();
    let rows_before = approval_state(&pool).await[1].clone();
    let input = approval(&reviewed, "approve");
    let approved = vat::vat_return_approve(&pool, &workspace, &input)
        .await
        .unwrap();
    assert_eq!(approved.status, "approved");
    assert_eq!(approved.review_revision, reviewed.review_revision);
    assert_eq!(
        serde_json::to_value(&approved.boxes).unwrap(),
        serde_json::to_value(&reviewed.boxes).unwrap()
    );
    assert_eq!(rows_before, approval_state(&pool).await[1]);
    assert_eq!(approved.box49_amount_minor, 2_500);
    // Retries refer to the successful historical intent, not today's profile/rule state.
    sqlx::query(
        "UPDATE vat_profiles SET accounting_method = 'cash_method' WHERE workspace_id = ?1",
    )
    .bind(&workspace)
    .execute(&pool)
    .await
    .unwrap();
    let before = approval_state(&pool).await;
    let replay = vat::vat_return_approve(&pool, &workspace, &input)
        .await
        .unwrap();
    assert_eq!(replay.review_revision, reviewed.review_revision);
    assert_eq!(before, approval_state(&pool).await);
    let retained_trace = vat::vat_return_trace(
        &pool,
        &workspace,
        &VatReturnTraceInput {
            vat_return_id: reviewed.id.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(trace).unwrap(),
        serde_json::to_value(retained_trace).unwrap()
    );
    let mut mismatch = input.clone();
    mismatch.reviewed_revision = Uuid::new_v4().to_string();
    assert!(vat::vat_return_approve(&pool, &workspace, &mismatch)
        .await
        .is_err());
    mismatch.idempotency_key = "different-key".into();
    assert!(vat::vat_return_approve(&pool, &workspace, &mismatch)
        .await
        .is_err());
    mismatch.vat_return_id = Uuid::new_v4().to_string();
    mismatch.idempotency_key = input.idempotency_key;
    assert!(vat::vat_return_approve(&pool, &workspace, &mismatch)
        .await
        .is_err());
    assert_eq!(before, approval_state(&pool).await);
    assert!(vat::vat_return_draft_create(
        &pool,
        &workspace,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".into(),
            idempotency_key: "overwrite-approved".into(),
        }
    )
    .await
    .is_err());
}

#[tokio::test]
async fn fresh_key_on_an_already_approved_return_is_bound_without_reapproving_history() {
    let (_dir, pool, workspace) = setup().await;
    let reviewed = draft(&pool, &workspace, "q1").await;
    vat::vat_return_approve(&pool, &workspace, &approval(&reviewed, "first-key"))
        .await
        .unwrap();
    let other = vat::vat_return_draft_create(
        &pool,
        &workspace,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q2".into(),
            idempotency_key: "q2".into(),
        },
    )
    .await
    .unwrap();
    let before = approval_state(&pool).await;
    let fresh_input = approval(&reviewed, "fresh-key");
    let repeated = vat::vat_return_approve(&pool, &workspace, &fresh_input)
        .await
        .unwrap();
    assert_eq!(repeated.review_revision, reviewed.review_revision);
    let after = approval_state(&pool).await;
    assert_eq!(
        &before[..4],
        &after[..4],
        "fresh replay binding must not change figures, status, locks or approval audit"
    );
    let binding: String = sqlx::query_scalar("SELECT payload_json FROM local_jobs WHERE job_type = 'vat_return_approve' AND idempotency_key = 'fresh-key'")
        .fetch_one(&pool).await.expect("every successful key is bound");
    let binding: serde_json::Value = serde_json::from_str(&binding).unwrap();
    assert_eq!(binding["vatReturnId"], reviewed.id);
    assert_eq!(
        binding["reviewedRevision"],
        reviewed.review_revision.unwrap()
    );
    vat::vat_return_approve(&pool, &workspace, &fresh_input)
        .await
        .unwrap();
    assert_eq!(
        after,
        approval_state(&pool).await,
        "same-key replay adds no records"
    );
    let error = vat::vat_return_approve(&pool, &workspace, &approval(&other, "fresh-key"))
        .await
        .expect_err("fresh key is reserved to the first return/revision");
    assert_eq!(error.code, "validation_error");
    assert_eq!(after, approval_state(&pool).await);
}

#[tokio::test]
async fn late_posting_requires_regeneration_and_old_review_cannot_approve_new_draft() {
    let (_dir, pool, workspace) = setup().await;
    let scenario =
        oppenbokforing_desktop_lib::state::load_golden_scenario("vat-reviewed-snapshot").unwrap();
    let old = draft(&pool, &workspace, "old").await;
    assert_eq!(
        Some(old.box49_amount_minor),
        scenario.expected["originalBox49Minor"].as_i64()
    );
    let old_trace = vat::vat_return_trace(
        &pool,
        &workspace,
        &VatReturnTraceInput {
            vat_return_id: old.id.clone(),
        },
    )
    .await
    .unwrap();
    post(&pool, &workspace, "late-sale").await.unwrap();
    assert_stale_unchanged(&pool, &workspace, &approval(&old, "approve-old")).await;
    // The displayed trace remains the reviewed snapshot, even before regeneration.
    assert_eq!(
        serde_json::to_value(old_trace).unwrap(),
        serde_json::to_value(
            vat::vat_return_trace(
                &pool,
                &workspace,
                &VatReturnTraceInput {
                    vat_return_id: old.id.clone()
                }
            )
            .await
            .unwrap()
        )
        .unwrap()
    );
    let fresh = draft(&pool, &workspace, "fresh").await;
    assert_eq!(fresh.id, old.id);
    assert_ne!(fresh.review_revision, old.review_revision);
    assert_eq!(
        Some(fresh.box49_amount_minor),
        scenario.expected["regeneratedBox49Minor"].as_i64()
    );
    assert_stale_unchanged(&pool, &workspace, &approval(&old, "approve-old")).await;
    let approved = vat::vat_return_approve(&pool, &workspace, &approval(&fresh, "approve-fresh"))
        .await
        .unwrap();
    assert_eq!(approved.box49_amount_minor, fresh.box49_amount_minor);
}

#[tokio::test]
async fn equal_totals_do_not_hide_added_offsetting_source_records() {
    let (_dir, pool, workspace) = setup().await;
    post(&pool, &workspace, "original-sale").await.unwrap();
    let reviewed = draft(&pool, &workspace, "before").await;
    post(&pool, &workspace, "offset-sale").await.unwrap();
    ledger::post_reversal_voucher(
        &pool,
        &workspace,
        &format!("fy-{workspace}-2026"),
        "credit",
        "offset-sale",
        "2026-02-01",
        &buckets(),
    )
    .await
    .unwrap();
    assert_stale_unchanged(&pool, &workspace, &approval(&reviewed, "stale")).await;
    let fresh = draft(&pool, &workspace, "after").await;
    let amounts = |r: &VatReturnSummary| {
        r.boxes
            .iter()
            .map(|b| (b.box_code.clone(), b.amount_minor))
            .collect::<Vec<_>>()
    };
    assert_eq!(amounts(&reviewed), amounts(&fresh));
    assert_ne!(reviewed.review_revision, fresh.review_revision);
    vat::vat_return_approve(&pool, &workspace, &approval(&fresh, "fresh"))
        .await
        .unwrap();
}

#[tokio::test]
async fn same_amount_new_line_identity_invalidates_even_an_unmapped_account() {
    let (_dir, pool, workspace) = setup().await;
    let voucher = post(&pool, &workspace, "sale").await.unwrap();
    let reviewed = draft(&pool, &workspace, "before").await;
    // Existing DB sealing permits insertion into posted vouchers. An equal debit/credit
    // pair has no VAT effect but must change the reviewed source identity.
    let account: String =
        sqlx::query_scalar("SELECT id FROM accounts WHERE workspace_id = ?1 AND number = '1510'")
            .bind(&workspace)
            .fetch_one(&pool)
            .await
            .unwrap();
    for (debit, credit) in [(100_i64, 0_i64), (0, 100)] {
        sqlx::query("INSERT INTO journal_lines (id,voucher_id,account_id,debit_minor,credit_minor) VALUES (?1,?2,?3,?4,?5)")
            .bind(Uuid::new_v4().to_string()).bind(&voucher).bind(&account).bind(debit).bind(credit)
            .execute(&pool).await.unwrap();
    }
    assert_stale_unchanged(&pool, &workspace, &approval(&reviewed, "stale-lines")).await;
}

#[tokio::test]
async fn semantic_profile_and_rule_changes_invalidate_but_profile_timestamps_do_not() {
    for mutation in [
        "UPDATE vat_profiles SET accounting_method = 'cash_method'",
        "UPDATE vat_profiles SET vat_status = 'voluntary_registered'",
        "UPDATE vat_profiles SET vat_filing_deadline_regime = NULL",
        "UPDATE workspaces SET active_rule_year = 2025",
        "INSERT INTO tax_profiles (id, workspace_id, tax_status) SELECT 'new-tax', id, 'f_skatt' FROM workspaces",
        "INSERT INTO sole_trader_profiles (id, workspace_id, business_name, owner_name) SELECT 'new-business', id, 'Firma', 'Owner' FROM workspaces",
        "UPDATE rule_versions SET status = 'retired' WHERE tax_year = 2026 AND status = 'active'",
        "UPDATE rule_versions SET checksum = 'changed' WHERE tax_year = 2026 AND status = 'active'",
        "UPDATE tax_rules SET value_json = '123456' WHERE id = (SELECT id FROM tax_rules WHERE rule_version_id = 'rv-2026-active' LIMIT 1)",
    ] {
        let (_dir, pool, workspace) = setup().await;
        let reviewed = draft(&pool, &workspace, "before").await;
        let result = sqlx::query(mutation).execute(&pool).await.unwrap();
        assert!(result.rows_affected() > 0, "mutation exercised: {mutation}");
        assert_stale_unchanged(&pool, &workspace, &approval(&reviewed, "stale-config")).await;
    }
    let (_dir, pool, workspace) = setup().await;
    let reviewed = draft(&pool, &workspace, "before").await;
    sqlx::query("UPDATE vat_profiles SET updated_at = '2099-01-01'")
        .execute(&pool)
        .await
        .unwrap();
    vat::vat_return_approve(&pool, &workspace, &approval(&reviewed, "unchanged"))
        .await
        .unwrap();
}

#[tokio::test]
async fn unchanged_regeneration_replaces_revision_not_review_and_replay_cannot_regenerate() {
    let (_dir, pool, workspace) = setup().await;
    let old = draft(&pool, &workspace, "old").await;
    let replay = draft(&pool, &workspace, "old").await;
    assert_eq!(old.review_revision, replay.review_revision);
    let new = draft(&pool, &workspace, "new").await;
    assert_eq!(
        serde_json::to_value(&old.boxes).unwrap(),
        serde_json::to_value(&new.boxes).unwrap()
    );
    assert_ne!(old.review_revision, new.review_revision);
    assert_stale_unchanged(&pool, &workspace, &approval(&old, "old-approve")).await;
    // An old generation retry must not claim a different revision was its own result.
    let retry = vat::vat_return_draft_create(
        &pool,
        &workspace,
        &VatReturnDraftCreateInput {
            period_key: "2026-Q1".into(),
            idempotency_key: "old".into(),
        },
    )
    .await;
    let superseded =
        retry.expect_err("superseded requests must be distinguishable from uncertain failures");
    assert_eq!(superseded.code, "vat_draft_superseded");
    let current = vat::vat_return_get(
        &pool,
        &workspace,
        &VatReturnGetInput {
            vat_return_id: old.id,
        },
    )
    .await
    .unwrap();
    assert_eq!(current.review_revision, new.review_revision);
    vat::vat_return_approve(&pool, &workspace, &approval(&new, "fresh-approve"))
        .await
        .unwrap();
}

#[tokio::test]
async fn missing_unsupported_or_changed_stored_snapshot_cannot_be_adopted_during_approval() {
    for mutation in [
        "UPDATE vat_returns SET review_snapshot_json = NULL, review_snapshot_hash = NULL",
        "UPDATE vat_returns SET review_snapshot_json = json_set(review_snapshot_json, '$.version', 999)",
        "UPDATE vat_returns SET review_snapshot_hash = 'invalid'",
        "UPDATE vat_return_boxes SET amount_minor = amount_minor + 1 WHERE box_code = '49'",
        "UPDATE vat_returns SET review_revision = NULL, review_snapshot_json = NULL, review_snapshot_hash = NULL",
    ] {
        let (_dir, pool, workspace) = setup().await;
        let reviewed = draft(&pool, &workspace, "review").await;
        sqlx::query(mutation).execute(&pool).await.unwrap();
        assert_stale_unchanged(&pool, &workspace, &approval(&reviewed, "reject")).await;
    }
}

#[tokio::test]
async fn workspace_scoping_cannot_approve_another_workspaces_revision() {
    let (_dir, pool, workspace) = setup().await;
    let reviewed = draft(&pool, &workspace, "review").await;
    let before = approval_state(&pool).await;
    assert!(vat::vat_return_approve(
        &pool,
        "another-workspace",
        &approval(&reviewed, "cross-workspace")
    )
    .await
    .is_err());
    assert_eq!(before, approval_state(&pool).await);
}

#[tokio::test]
async fn concurrent_posting_and_approval_cannot_both_commit_against_old_review() {
    for _ in 0..6 {
        let (dir, pool, workspace) = setup().await;
        let other = connect_workspace(&dir.path().join("workspace.sqlite"))
            .await
            .unwrap();
        let reviewed = draft(&pool, &workspace, "review").await;
        let input = approval(&reviewed, "approve");
        let (approval_result, posting_result) = tokio::join!(
            vat::vat_return_approve(&pool, &workspace, &input),
            post(&other, &workspace, "racing-sale"),
        );
        assert!(
            !(approval_result.is_ok() && posting_result.is_ok()),
            "no changed source may commit under an old approval"
        );
        if let Ok(approved) = approval_result {
            assert_eq!(approved.box49_amount_minor, 0);
            assert_eq!(approved.review_revision, reviewed.review_revision);
            assert_eq!(
                approval_state(&pool).await[1],
                approval_state(&other).await[1]
            );
        } else {
            assert!(
                posting_result.is_ok(),
                "posting should win if approval is stale"
            );
            let current = vat::vat_return_get(
                &pool,
                &workspace,
                &VatReturnGetInput {
                    vat_return_id: reviewed.id,
                },
            )
            .await
            .unwrap();
            assert_eq!(current.status, "draft");
            assert_eq!(current.box49_amount_minor, 0);
        }
    }
}

#[tokio::test]
async fn posting_commit_while_approval_waits_requires_fresh_review() {
    let (dir, pool, workspace) = setup().await;
    let writer = connect_workspace(&dir.path().join("workspace.sqlite"))
        .await
        .unwrap();
    let reviewed = draft(&pool, &workspace, "review").await;
    let before = approval_state(&pool).await;
    let mut posting = writer.begin_with("BEGIN IMMEDIATE").await.unwrap();
    ledger::post_invoice_voucher_tx(
        &mut posting,
        &workspace,
        &format!("fy-{workspace}-2026"),
        "committing-sale",
        "2026-02-01",
        &buckets(),
    )
    .await
    .unwrap();
    let approval_pool = pool.clone();
    let approval_workspace = workspace.clone();
    let input = approval(&reviewed, "queued-approve");
    let (started, ready) = tokio::sync::oneshot::channel();
    let pending = tokio::spawn(async move {
        started.send(()).unwrap();
        vat::vat_return_approve(&approval_pool, &approval_workspace, &input).await
    });
    ready.await.unwrap();
    assert!(
        !pending.is_finished(),
        "approval cannot finish while another writer owns the transaction"
    );
    posting.commit().await.unwrap();
    let rejected = pending
        .await
        .unwrap()
        .expect_err("approval must observe the just-committed source");
    assert_eq!(rejected.code, "validation_error");
    assert!(rejected.message.contains("Regenerate"));
    assert_eq!(before, approval_state(&pool).await);
    let fresh = draft(&pool, &workspace, "fresh").await;
    assert_eq!(fresh.box49_amount_minor, 2_500);
    vat::vat_return_approve(&pool, &workspace, &approval(&fresh, "fresh-approve"))
        .await
        .unwrap();
}

#[tokio::test]
async fn approval_failure_rolls_back_status_lock_audit_and_replay_together() {
    for trigger in [
        "CREATE TRIGGER reject_approval_audit BEFORE INSERT ON audit_events WHEN NEW.action = 'vat_return_approved' BEGIN SELECT RAISE(ABORT, 'test audit failure'); END",
        "CREATE TRIGGER reject_approval_replay BEFORE INSERT ON local_jobs WHEN NEW.job_type = 'vat_return_approve' BEGIN SELECT RAISE(ABORT, 'test replay failure'); END",
    ] {
        let (_dir, pool, workspace) = setup().await;
        post(&pool, &workspace, "sale").await.unwrap();
        let reviewed = draft(&pool, &workspace, "review").await;
        sqlx::query(trigger).execute(&pool).await.unwrap();
        let before = approval_state(&pool).await;
        let result = vat::vat_return_approve(&pool, &workspace, &approval(&reviewed, "approve")).await;
        assert!(result.is_err(), "injected failure must abort the approval transaction");
        assert_eq!(before, approval_state(&pool).await);
    }
}

#[tokio::test]
async fn posted_vouchers_without_lines_are_part_of_the_review_identity() {
    let (_dir, pool, workspace) = setup().await;
    let reviewed = draft(&pool, &workspace, "review").await;
    sqlx::query("INSERT INTO vouchers(id,workspace_id,fiscal_year_id,status,source_type,posted_at,accounting_date) VALUES ('empty-voucher',?1,?2,'posted','legacy','2026-02-01','2026-02-01')")
        .bind(&workspace).bind(format!("fy-{workspace}-2026")).execute(&pool).await.unwrap();
    assert_stale_unchanged(&pool, &workspace, &approval(&reviewed, "approve")).await;
}

#[tokio::test]
async fn concurrent_regeneration_and_approval_never_approve_an_unreviewed_revision() {
    for _ in 0..6 {
        let (dir, pool, workspace) = setup().await;
        let other = connect_workspace(&dir.path().join("workspace.sqlite"))
            .await
            .unwrap();
        let reviewed = draft(&pool, &workspace, "review").await;
        let approve = approval(&reviewed, "approve");
        let regenerate = VatReturnDraftCreateInput {
            period_key: "2026-Q1".into(),
            idempotency_key: "regenerate".into(),
        };
        let (approved, regenerated) = tokio::join!(
            vat::vat_return_approve(&pool, &workspace, &approve),
            vat::vat_return_draft_create(&other, &workspace, &regenerate),
        );
        assert_ne!(
            approved.is_ok(),
            regenerated.is_ok(),
            "exactly one competing operation commits"
        );
        let current = vat::vat_return_get(
            &pool,
            &workspace,
            &VatReturnGetInput {
                vat_return_id: reviewed.id,
            },
        )
        .await
        .unwrap();
        if approved.is_ok() {
            assert_eq!(current.status, "approved");
            assert_eq!(current.review_revision, reviewed.review_revision);
        } else {
            assert_eq!(current.status, "draft");
            assert_ne!(current.review_revision, reviewed.review_revision);
            assert_stale_unchanged(&pool, &workspace, &approve).await;
        }
    }
}
