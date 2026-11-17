//! Independent native invoice-v1 history regressions across the real historical schema21 -> current23
//! upgrade, cold reopen and encrypted backup. No current issuance generates history,
//! and none of these assertions establishes new cash-case accounting eligibility.
#[path = "cash_history_upgrade/fixture.rs"]
mod fixture;

use fixture::{history, rows, sha256, sorted, History, NativeFixture, FINGERPRINT, PDF_HASH};
use oppenbokforing_desktop_lib::{
    backup::{self, BackupRestoreInput},
    db::open_existing_workspace,
    documents, invoicing,
    invoicing::{InvoiceIssueInput, InvoiceIssueValidationGetInput},
};
use serde_json::Value;
use sqlx::SqlitePool;
use std::path::Path;
use tempfile::tempdir;

const RECOVERY_KEY: &str = "native-v1-already-issued-recovery";
const PASSPHRASE: &str = "native-history-encrypted-regression-only";
const OTHER_WORKSPACE: &str = "unrelated-workspace-cannot-read-native-history";

async fn assert_retained_proof_and_pdf(fixture: &NativeFixture, pool: &SqlitePool, root: &Path) {
    let raw = &fixture.archive["tables"]["invoice_issue_validations"][0];
    assert_eq!(
        sha256(raw["context_json"].as_str().unwrap().as_bytes()),
        FINGERPRINT
    );
    assert_eq!(
        sha256(include_bytes!(
            "../../fixtures/accounting-validation/2026-domestic-service25-exact.json"
        )),
        "cb00f554e7ee3697b8461309e7184fd88b3a0aa70300f3f27ead9f6ca98b8594",
        "published v1 manifest must retain its raw identity"
    );
    assert_eq!(
        rows(pool, "invoice_issue_validations", "").await,
        vec![raw.clone()]
    );
    let input = InvoiceIssueValidationGetInput {
        invoice_id: fixture.invoice.clone(),
    };
    let proof = invoicing::invoice_issue_validation_get(pool, &fixture.workspace, &input)
        .await
        .expect("historical v1 decoder must accept the genuine f9b6933 context")
        .expect("retained native provenance");
    let proof = serde_json::to_value(proof).unwrap();
    assert_eq!(proof["invoiceId"], fixture.invoice);
    assert_eq!(
        proof["facts"],
        serde_json::from_str::<Value>(raw["facts_json"].as_str().unwrap()).unwrap()
    );
    assert_eq!(
        proof["assessment"],
        serde_json::from_str::<Value>(raw["assessment_json"].as_str().unwrap()).unwrap()
    );
    assert_eq!(proof["assessment"]["reviewFingerprint"], FINGERPRINT);
    assert_eq!(proof["recordedAt"], raw["recorded_at"]);
    assert!(
        invoicing::invoice_issue_validation_get(pool, OTHER_WORKSPACE, &input)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        invoicing::get_invoice(pool, OTHER_WORKSPACE, &fixture.invoice)
            .await
            .is_err()
    );

    let document = documents::verify_retained_document(pool, &fixture.workspace, &fixture.document)
        .await
        .expect("retained PDF bytes and workspace must verify");
    assert_eq!(document.content_sha256, PDF_HASH);
    let actual_pdf = std::fs::read(root.join("documents").join(&document.object_path)).unwrap();
    assert_eq!(actual_pdf.len(), 2328);
    assert_eq!(sha256(&actual_pdf), PDF_HASH);
    assert_eq!(actual_pdf, fixture.pdf, "never regenerate the native PDF");
    assert!(
        documents::verify_retained_document(pool, OTHER_WORKSPACE, &fixture.document)
            .await
            .is_err()
    );
    let invoice = invoicing::get_invoice(pool, &fixture.workspace, &fixture.invoice)
        .await
        .unwrap();
    assert_eq!(
        invoice.pdf_document_id.as_deref(),
        Some(fixture.document.as_str())
    );
    assert_eq!(invoice.invoice_number.as_deref(), Some("2026-0001"));
    assert_eq!(invoice.total_inc_vat_minor, 250_000);
}

fn archived_issue(fixture: &NativeFixture) -> Value {
    let job = fixture.archive["tables"]["local_jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["job_type"] == "invoice_issue")
        .unwrap();
    serde_json::from_str(job["payload_json"].as_str().unwrap()).unwrap()
}

fn issue_input(fixture: &NativeFixture, key: &str) -> InvoiceIssueInput {
    // Deliberately omit all live eligibility/fingerprint inputs and use an unrelated
    // date. Completed recovery must not reevaluate the retired case as a new issue.
    InvoiceIssueInput {
        invoice_id: fixture.invoice.clone(),
        idempotency_key: key.into(),
        issue_date: Some("2027-01-02".into()),
        eligibility_facts: None,
        review_fingerprint: None,
    }
}

async fn assert_cached_replay(fixture: &NativeFixture, pool: &SqlitePool) {
    let before = history(pool, &fixture.archive).await;
    let retained = archived_issue(fixture);
    // This genuine cached response predates PDF completion, so null must remain
    // null even though get_invoice now returns the actual retained PDF identity.
    assert!(retained["invoice"]["pdfDocumentId"].is_null());
    let key = retained["idempotencyKey"].as_str().unwrap();
    let replay = invoicing::issue_invoice(pool, &fixture.workspace, &issue_input(fixture, key))
        .await
        .expect("completed native replay must precede live v2 eligibility");
    assert_eq!(serde_json::to_value(replay).unwrap(), retained["invoice"]);
    let scoped = invoicing::issue_invoice(pool, OTHER_WORKSPACE, &issue_input(fixture, key)).await;
    assert!(scoped.is_err(), "native replay is workspace-scoped");
    assert_eq!(
        history(pool, &fixture.archive).await,
        before,
        "cached replay/foreign workspace read must not mutate any native row"
    );
}

async fn assert_already_issued_recovery(fixture: &NativeFixture, pool: &SqlitePool) {
    let before = history(pool, &fixture.archive).await;
    let current = invoicing::get_invoice(pool, &fixture.workspace, &fixture.invoice)
        .await
        .unwrap();
    let recovered = invoicing::issue_invoice(
        pool,
        &fixture.workspace,
        &issue_input(fixture, RECOVERY_KEY),
    )
    .await
    .expect("already-issued recovery must not reinterpret a native v1 invoice");
    assert_eq!(
        serde_json::to_value(recovered).unwrap(),
        serde_json::to_value(&current).unwrap()
    );
    let after = history(pool, &fixture.archive).await;
    let mut expected = before.clone();
    let extra = after["local_jobs"]
        .iter()
        .filter(|row| !before["local_jobs"].contains(row))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        extra.len(),
        1,
        "only a completed recovery replay record may be appended"
    );
    assert_eq!(extra[0]["workspace_id"], fixture.workspace);
    assert_eq!(extra[0]["job_type"], "invoice_issue");
    assert_eq!(extra[0]["status"], "succeeded");
    assert_eq!(extra[0]["idempotency_key"], RECOVERY_KEY);
    let payload: Value = serde_json::from_str(extra[0]["payload_json"].as_str().unwrap()).unwrap();
    assert_eq!(payload["invoice"], serde_json::to_value(&current).unwrap());
    expected.get_mut("local_jobs").unwrap().extend(extra);
    expected.insert("local_jobs".into(), sorted(expected["local_jobs"].clone()));
    assert_eq!(
        after, expected,
        "recovery creates no new vouchers, journals, audit, PDF, or provenance"
    );
    let again = invoicing::issue_invoice(
        pool,
        &fixture.workspace,
        &issue_input(fixture, RECOVERY_KEY),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(again).unwrap(),
        serde_json::to_value(current).unwrap()
    );
    assert_eq!(
        history(pool, &fixture.archive).await,
        after,
        "recovery replay is exactly once"
    );
}

#[tokio::test]
async fn genuine_native_v1_history_survives_real_schema21_to_23_upgrade() {
    let mut fixture = NativeFixture::schema_21().await;
    assert_retained_proof_and_pdf(&fixture, &fixture.pool, fixture.dir.path()).await;
    fixture.upgrade().await;
    assert_retained_proof_and_pdf(&fixture, &fixture.pool, fixture.dir.path()).await;
    assert_cached_replay(&fixture, &fixture.pool).await;
    assert_already_issued_recovery(&fixture, &fixture.pool).await;
    fixture.assert_schema_23(&fixture.pool).await;
    fixture.pool.close().await;
}

// Backup service writes its own job/audit, and restore explicitly remaps workspace
// paths/updated_at and appends a restore audit. Compare all historical rows exactly
// while allowing ONLY those named service-owned effects, never accounting changes.
async fn assert_backup_history(
    fixture: &NativeFixture,
    pool: &SqlitePool,
    baseline: &History,
    restored_database: Option<&Path>,
) {
    let mut actual = history(pool, &fixture.archive).await;
    let mut expected = baseline.clone();
    let backup_jobs = actual["local_jobs"]
        .iter()
        .filter(|row| !baseline["local_jobs"].contains(row))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(backup_jobs.len(), 1);
    assert_eq!(backup_jobs[0]["job_type"], "workspace_backup_create");
    assert_eq!(backup_jobs[0]["workspace_id"], fixture.workspace);
    expected.get_mut("local_jobs").unwrap().extend(backup_jobs);
    expected.insert("local_jobs".into(), sorted(expected["local_jobs"].clone()));
    let audits = actual["audit_events"]
        .iter()
        .filter(|row| !baseline["audit_events"].contains(row))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        audits.len(),
        1,
        "only service-specific backup/restore audit, never new invoice accounting"
    );
    assert_eq!(audits[0]["workspace_id"], fixture.workspace);
    assert_eq!(audits[0]["resource_type"], "backup");
    assert_eq!(
        audits[0]["action"],
        if restored_database.is_some() {
            "workspace_backup_restore"
        } else {
            "workspace_backup_create"
        }
    );
    expected.get_mut("audit_events").unwrap().extend(audits);
    expected.insert(
        "audit_events".into(),
        sorted(expected["audit_events"].clone()),
    );
    if let Some(database) = restored_database {
        let root = database.parent().unwrap();
        let row = &mut expected.get_mut("workspaces").unwrap()[0];
        row["database_path"] = database.to_string_lossy().as_ref().into();
        row["documents_path"] = root.join("documents").to_string_lossy().as_ref().into();
        row["exports_path"] = root.join("exports").to_string_lossy().as_ref().into();
        assert!(actual["workspaces"][0]["updated_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        row["updated_at"] = actual["workspaces"][0]["updated_at"].clone();
    }
    assert_eq!(actual, expected, "backup/restore preserves every native and recovery row outside documented backup bookkeeping");
    // Keep these central no-new-accounting expectations visible even if the broader
    // history list changes later.
    for table in [
        "vouchers",
        "journal_lines",
        "invoice_issue_validations",
        "invoice_issue_snapshots",
    ] {
        assert_eq!(actual.remove(table).unwrap(), baseline[table]);
    }
}

#[tokio::test]
async fn upgraded_native_v1_survives_cold_reopen_encrypted_restore_and_scoped_replay() {
    let mut fixture = NativeFixture::schema_21().await;
    fixture.upgrade().await;
    assert_already_issued_recovery(&fixture, &fixture.pool).await;
    let recovered_history = history(&fixture.pool, &fixture.archive).await;
    fixture.pool.close().await;
    fixture.pool = open_existing_workspace(&fixture.database).await.unwrap();
    assert_eq!(
        history(&fixture.pool, &fixture.archive).await,
        recovered_history,
        "cold reopen preserves all raw rows"
    );
    fixture.assert_schema_23(&fixture.pool).await;
    assert_retained_proof_and_pdf(&fixture, &fixture.pool, fixture.dir.path()).await;
    assert_cached_replay(&fixture, &fixture.pool).await;
    let recovered =
        invoicing::check_issue_idempotency(&fixture.pool, &fixture.workspace, RECOVERY_KEY)
            .await
            .unwrap()
            .unwrap();

    let backup_dir = tempdir().unwrap();
    let package = backup::create_backup_package(
        &fixture.pool,
        &fixture.workspace,
        fixture.dir.path(),
        &fixture.database,
        backup_dir.path(),
        PASSPHRASE,
        None,
    )
    .await
    .unwrap();
    let encrypted = std::fs::read(&package.backup_path).unwrap();
    assert!(!backup::backup_plaintext_is_sqlite(&encrypted));
    assert!(!encrypted
        .windows(FINGERPRINT.len())
        .any(|part| part == FINGERPRINT.as_bytes()));
    let object_path = fixture.archive["tables"]["documents"][0]["object_path"]
        .as_str()
        .unwrap();
    let entry = package
        .manifest
        .entries
        .iter()
        .find(|entry| entry.relative_path == format!("documents/{object_path}"))
        .expect("retained original PDF is included in encrypted manifest");
    assert_eq!(entry.sha256, PDF_HASH);
    assert_eq!(entry.bytes, 2328);
    assert_backup_history(&fixture, &fixture.pool, &recovered_history, None).await;

    let restore_dir = tempdir().unwrap();
    let restored = backup::restore_backup_package(
        &BackupRestoreInput {
            backup_path: package.backup_path,
            confirm_overwrite: true,
            passphrase: PASSPHRASE.into(),
        },
        restore_dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        restored.workspace_id, fixture.workspace,
        "logical workspace identity is retained"
    );
    let restored_database = Path::new(&restored.database_path);
    assert_ne!(restored_database, fixture.database);
    assert!(restored_database.starts_with(restore_dir.path()));
    let restored_pool = open_existing_workspace(restored_database).await.unwrap();
    fixture.assert_schema_23(&restored_pool).await;
    assert_backup_history(
        &fixture,
        &restored_pool,
        &recovered_history,
        Some(restored_database),
    )
    .await;
    assert_retained_proof_and_pdf(
        &fixture,
        &restored_pool,
        restored_database.parent().unwrap(),
    )
    .await;
    assert_cached_replay(&fixture, &restored_pool).await;
    let before_replay = history(&restored_pool, &fixture.archive).await;
    let recovery = invoicing::issue_invoice(
        &restored_pool,
        &fixture.workspace,
        &issue_input(&fixture, RECOVERY_KEY),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(recovery).unwrap(),
        serde_json::to_value(recovered).unwrap()
    );
    assert_eq!(
        history(&restored_pool, &fixture.archive).await,
        before_replay,
        "restored recovery replay adds no journal/audit/provenance"
    );
    // Restore/replay must not modify the still-open source workspace either.
    assert_backup_history(&fixture, &fixture.pool, &recovered_history, None).await;
    restored_pool.close().await;
    fixture.pool.close().await;
}

#[tokio::test]
async fn native_backup_rejects_missing_or_corrupt_retained_pdf_without_fallback() {
    for corrupt in [false, true] {
        let mut fixture = NativeFixture::schema_21().await;
        fixture.upgrade().await;
        let before = history(&fixture.pool, &fixture.archive).await;
        let object_path = fixture.archive["tables"]["documents"][0]["object_path"]
            .as_str()
            .unwrap();
        let source = fixture.dir.path().join("documents").join(object_path);
        // Damage only this test's owned copy, never the archived bytes or native UAT.
        if corrupt {
            std::fs::write(&source, b"corrupt retained PDF").unwrap();
        } else {
            std::fs::remove_file(&source).unwrap();
        }
        let backup_dir = tempdir().unwrap();
        let result = backup::create_backup_package(
            &fixture.pool,
            &fixture.workspace,
            fixture.dir.path(),
            &fixture.database,
            backup_dir.path(),
            PASSPHRASE,
            None,
        )
        .await;
        assert!(
            result.is_err(),
            "backup must reject {} retained PDF, not fall back to archived bytes/paths",
            if corrupt { "corrupt" } else { "missing" }
        );
        if corrupt {
            assert_eq!(std::fs::read(&source).unwrap(), b"corrupt retained PDF");
        } else {
            assert!(
                !source.exists(),
                "backup must not regenerate historical PDFs"
            );
        }
        let after = history(&fixture.pool, &fixture.archive).await;
        for (table, expected) in &before {
            // Failed backup bookkeeping is service-owned; every other raw row stays exact.
            if table != "local_jobs" && table != "audit_events" {
                assert_eq!(&after[table], expected, "failed backup changed {table}");
            }
        }
        fixture.assert_schema_23(&fixture.pool).await;
        fixture.pool.close().await;
    }
}
