use oppenbokforing_desktop_lib::{
    backup::{self, BackupCreateInput, BackupManifest, BackupSummary},
    db::{connect_workspace, open_existing_workspace},
};
use sqlx::SqlitePool;
use tempfile::{tempdir, TempDir};
use uuid::Uuid;

#[tokio::test]
async fn backup_create_is_idempotent() {
    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let data_dir = dir.path().join(&workspace_id);
    std::fs::create_dir_all(data_dir.join("documents")).expect("documents");
    std::fs::create_dir_all(data_dir.join("exports")).expect("exports");
    let database_path = data_dir.join("workspace.sqlite");
    let pool = connect_workspace(&database_path).await.expect("connect");

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("Idempotency workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    let idempotency_key = "backup-attempt-1";
    assert!(
        backup::check_idempotency(&pool, &workspace_id, idempotency_key, "workspace_backup_create")
            .await
            .expect("check")
            .is_none()
    );

    let first = backup::create_backup_package(
        &pool,
        &workspace_id,
        &data_dir,
        &database_path,
        &dir.path().join("backup-destination"),
        "test-passphrase-12",
        None,
    )
    .await
    .expect("first backup");

    backup::record_idempotent_job(
        &pool,
        &workspace_id,
        idempotency_key,
        "workspace_backup_create",
        &first,
    )
    .await
    .expect("record");

    let cached = backup::check_idempotency(
        &pool,
        &workspace_id,
        idempotency_key,
        "workspace_backup_create",
    )
    .await
    .expect("cached")
    .expect("cached summary");

    assert_eq!(cached.backup_path, first.backup_path);
    assert_eq!(cached.manifest.manifest_sha256, first.manifest.manifest_sha256);
    let backup::BackupCreateClaim::Cached(claimed_cached) = backup::claim_backup_create(
        &pool,
        &workspace_id,
        idempotency_key,
        "workspace_backup_create",
    )
    .await
    .expect("cached claim")
    else {
        panic!("completed backup must be returned from the idempotency cache");
    };
    assert_eq!(claimed_cached.backup_path, first.backup_path);

    assert!(backup::idempotent_backup_matches_request(&cached, Some(&first.backup_path)));
    assert!(!backup::idempotent_backup_matches_request(
        &cached,
        Some("/tmp/other-backup.skatbackup"),
    ));

    let _input = BackupCreateInput {
        idempotency_key: idempotency_key.to_string(),
        destination_path: None,
        backup_file_path: None,
        passphrase: "test-passphrase-12".to_string(),
    };
}

async fn backup_test_workspace() -> (TempDir, SqlitePool, String) {
    let dir = tempdir().expect("tempdir");
    let workspace_id = Uuid::new_v4().to_string();
    let database_path = dir.path().join("workspace.sqlite");
    let pool = connect_workspace(&database_path).await.expect("connect");
    let data_dir = dir.path().join(&workspace_id);
    let documents_dir = data_dir.join("documents");
    let exports_dir = data_dir.join("exports");
    std::fs::create_dir_all(&documents_dir).expect("documents");
    std::fs::create_dir_all(&exports_dir).expect("exports");
    sqlx::query(
        "INSERT INTO workspaces (id, name, database_path, documents_path, exports_path) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&workspace_id)
    .bind("Recovery workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(documents_dir.to_string_lossy().to_string())
    .bind(exports_dir.to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace");
    (dir, pool, workspace_id)
}

fn completed_backup_summary() -> BackupSummary {
    BackupSummary {
        backup_path: "/tmp/backup.skatbackup".to_string(),
        manifest: BackupManifest {
            version: 1,
            workspace_id: "workspace".to_string(),
            workspace_name: "Workspace".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            entries: vec![],
            manifest_sha256: "manifest".to_string(),
        },
    }
}

#[tokio::test]
async fn expired_backup_claim_replaces_owner_and_rejects_old_finalizer() {
    let (_dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let backup::BackupCreateClaim::Proceed(old_lease) =
        backup::claim_backup_create(&pool, &workspace_id, "expired-claim", job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    sqlx::query(
        "UPDATE local_jobs SET payload_json = json_set(payload_json, '$.lease.expiresAt', datetime('now', '-1 second')) WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind("expired-claim")
    .execute(&pool)
    .await
    .expect("expire lease");

    let backup::BackupCreateClaim::Proceed(new_lease) =
        backup::claim_backup_create(&pool, &workspace_id, "expired-claim", job_type)
            .await
            .expect("reclaim expired claim")
    else {
        panic!("expired claim must be reclaimed");
    };
    assert!(
        backup::renew_backup_create_lease(
            &pool,
            &workspace_id,
            "expired-claim",
            job_type,
            &old_lease,
        )
        .await
        .is_err(),
        "the replaced lease token cannot renew the current claim"
    );
    let summary = completed_backup_summary();
    assert!(
        backup::finalize_backup_create(
            &pool,
            &workspace_id,
            "expired-claim",
            job_type,
            &old_lease,
            &summary,
        )
        .await
        .is_err(),
        "a replaced owner cannot finalize the new claim"
    );

    backup::finalize_backup_create(
        &pool,
        &workspace_id,
        "expired-claim",
        job_type,
        &new_lease,
        &summary,
    )
    .await
    .expect("current owner finalizes");
    assert_eq!(
        backup::check_idempotency(&pool, &workspace_id, "expired-claim", job_type)
            .await
            .expect("cached")
            .expect("summary")
            .backup_path,
        summary.backup_path
    );
}

#[tokio::test]
async fn renewed_backup_claim_cannot_be_reclaimed_from_an_old_updated_timestamp() {
    let (_dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(&pool, &workspace_id, "renewed-claim", job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };

    backup::renew_backup_create_lease(
        &pool,
        &workspace_id,
        "renewed-claim",
        job_type,
        &lease,
    )
    .await
    .expect("renew lease");
    sqlx::query(
        "UPDATE local_jobs SET updated_at = datetime('now', '-6 minutes') WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind("renewed-claim")
    .execute(&pool)
    .await
    .expect("age row without expiring lease");

    assert!(
        backup::claim_backup_create(&pool, &workspace_id, "renewed-claim", job_type)
            .await
            .is_err(),
        "the lease, not updated_at, governs runner ownership"
    );
}

#[tokio::test]
async fn only_current_backup_owner_can_mark_a_claim_failed() {
    let (_dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let backup::BackupCreateClaim::Proceed(old_lease) =
        backup::claim_backup_create(&pool, &workspace_id, "failed-claim", job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    sqlx::query(
        "UPDATE local_jobs SET payload_json = json_set(payload_json, '$.lease.expiresAt', datetime('now', '-1 second')) WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind("failed-claim")
    .execute(&pool)
    .await
    .expect("expire lease");
    let backup::BackupCreateClaim::Proceed(new_lease) =
        backup::claim_backup_create(&pool, &workspace_id, "failed-claim", job_type)
            .await
            .expect("reclaim")
    else {
        panic!("expired claim must be reclaimed");
    };

    assert!(
        backup::fail_backup_create_claim(
            &pool,
            &workspace_id,
            "failed-claim",
            job_type,
            &old_lease,
        )
        .await
        .is_err(),
        "a replaced owner cannot fail the current claim"
    );
    backup::fail_backup_create_claim(
        &pool,
        &workspace_id,
        "failed-claim",
        job_type,
        &new_lease,
    )
    .await
    .expect("current owner fails its claim");
    let status: String = sqlx::query_scalar(
        "SELECT status FROM local_jobs WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind("failed-claim")
    .fetch_one(&pool)
    .await
    .expect("failed status");
    let last_error: Option<String> = sqlx::query_scalar(
        "SELECT last_error FROM local_jobs WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind("failed-claim")
    .fetch_one(&pool)
    .await
    .expect("failure reason");
    assert_eq!(status, "failed");
    assert_eq!(last_error.as_deref(), Some("Backup creation failed"));
    assert!(matches!(
        backup::claim_backup_create(&pool, &workspace_id, "failed-claim", job_type)
            .await
            .expect("failed claim is reclaimable"),
        backup::BackupCreateClaim::Proceed(_)
    ));
}

#[tokio::test]
async fn package_failure_marks_backup_claim_reclaimable_for_retry() {
    let (dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "package-failure";
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };

    backup::create_backup_package(
        &pool,
        &workspace_id,
        &dir.path().join(&workspace_id),
        &dir.path().join("workspace.sqlite"),
        &dir.path().join("backup-destination"),
        "short",
        None,
    )
    .await
    .expect_err("short passphrase must fail package creation");
    backup::fail_backup_create_claim(&pool, &workspace_id, idempotency_key, job_type, &lease)
        .await
        .expect("failed package claim is marked reclaimable");

    assert!(matches!(
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("retry claim"),
        backup::BackupCreateClaim::Proceed(_)
    ));
}

#[tokio::test]
async fn failed_finalization_leaves_written_artifact_uncached_and_never_overwrites_it() {
    let (dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "finalize-failure";
    let backup_path = dir.path().join("backups").join("finalize-failure.skatbackup");
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    let summary = backup::create_backup_package(
        &pool,
        &workspace_id,
        &dir.path().join(&workspace_id),
        &dir.path().join("workspace.sqlite"),
        &dir.path().join("backup-destination"),
        "test-passphrase-12",
        Some(backup_path.to_string_lossy().as_ref()),
    )
    .await
    .expect("package creation");
    assert_eq!(summary.backup_path, backup_path.to_string_lossy());
    assert!(backup_path.exists(), "package artifact was written");

    sqlx::query(
        "UPDATE local_jobs SET payload_json = json_set(payload_json, '$.lease.expiresAt', datetime('now', '-1 second')) WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind(idempotency_key)
    .execute(&pool)
    .await
    .expect("expire lease before finalization");
    assert!(
        backup::finalize_backup_create(
            &pool,
            &workspace_id,
            idempotency_key,
            job_type,
            &lease,
            &summary,
        )
        .await
        .is_err(),
        "expired owner cannot cache a written package"
    );
    assert!(
        backup::check_idempotency(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("idempotency check")
            .is_none(),
        "a failed finalization must not falsely cache its artifact"
    );
    assert!(matches!(
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("safe retry claim"),
        backup::BackupCreateClaim::Proceed(_)
    ));
    assert!(
        backup::create_backup_package(
            &pool,
            &workspace_id,
            &dir.path().join(&workspace_id),
            &dir.path().join("workspace.sqlite"),
            &dir.path().join("backup-destination"),
            "test-passphrase-12",
            Some(backup_path.to_string_lossy().as_ref()),
        )
        .await
        .is_err(),
        "a retry must not overwrite the artifact from failed finalization"
    );
}


#[tokio::test]
async fn lost_backup_lease_cannot_publish_its_staged_artifact() {
    let (dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "lost-lease-publish";
    let final_path = dir.path().join("lost-lease-publish.skatbackup");
    let backup::BackupCreateClaim::Proceed(old_lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    let staged = backup::stage_backup_package(
        &pool,
        &workspace_id,
        &dir.path().join(&workspace_id),
        &dir.path().join("workspace.sqlite"),
        final_path.as_path(),
        "test-passphrase-12",
        &old_lease,
    )
    .await
    .expect("stage package");
    backup::record_backup_staging(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &old_lease,
        &staged,
    )
    .await
    .expect("record staging");
    sqlx::query(
        "UPDATE local_jobs SET payload_json = json_set(payload_json, '$.lease.expiresAt', datetime('now', '-1 second')) WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind(idempotency_key)
    .execute(&pool)
    .await
    .expect("expire lease");
    let backup::BackupCreateClaim::Proceed(_) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("reclaim")
    else {
        panic!("expired claim must be reclaimed");
    };

    assert!(
        backup::publish_staged_backup(
            &pool,
            &workspace_id,
            idempotency_key,
            job_type,
            &old_lease,
            &staged,
        )
        .await
        .is_err(),
        "lost owner must be fenced before publication"
    );
    assert!(!final_path.exists(), "lost owner never publishes a final path");
    assert!(staged.staging_path.exists(), "only the owner may clean its staging path");
}

#[tokio::test]
async fn retry_cleans_only_interrupted_owned_staging() {
    let (dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "interrupted-staging";
    let final_path = dir.path().join("interrupted-staging.skatbackup");
    let backup::BackupCreateClaim::Proceed(old_lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    let staged = backup::stage_backup_package(
        &pool,
        &workspace_id,
        &dir.path().join(&workspace_id),
        &dir.path().join("workspace.sqlite"),
        final_path.as_path(),
        "test-passphrase-12",
        &old_lease,
    )
    .await
    .expect("stage package");
    backup::record_backup_staging(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &old_lease,
        &staged,
    )
    .await
    .expect("record staging");
    backup::fail_backup_create_claim(&pool, &workspace_id, idempotency_key, job_type, &old_lease)
        .await
        .expect("fail interrupted claim");
    let backup::BackupCreateClaim::Proceed(retry_lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("retry claim")
    else {
        panic!("failed claim must be reclaimed");
    };

    assert!(
        backup::recover_backup_create(
            &pool,
            &workspace_id,
            idempotency_key,
            job_type,
            &retry_lease,
            "test-passphrase-12",
        )
        .await
        .expect("recover interrupted staging")
        .is_none()
    );
    assert!(!staged.staging_path.exists(), "retry removes only its recorded staging path");
    assert!(!final_path.exists(), "interrupted staging never creates a final path");
}

#[tokio::test]
async fn retry_adopts_verified_promoted_artifact_after_finalization_failure() {
    let (dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "promoted-retry";
    let final_path = dir.path().join("promoted-retry.skatbackup");
    let backup::BackupCreateClaim::Proceed(old_lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    let staged = backup::stage_backup_package(
        &pool,
        &workspace_id,
        &dir.path().join(&workspace_id),
        &dir.path().join("workspace.sqlite"),
        final_path.as_path(),
        "test-passphrase-12",
        &old_lease,
    )
    .await
    .expect("stage package");
    backup::record_backup_staging(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &old_lease,
        &staged,
    )
    .await
    .expect("record staging");
    backup::publish_staged_backup(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &old_lease,
        &staged,
    )
    .await
    .expect("publish");
    assert!(
        !staged.staging_path.exists(),
        "successful publication removes the private encrypted staging package"
    );
    assert!(
        !staged
            .staging_path
            .parent()
            .expect("private staging root")
            .exists(),
        "successful publication removes the private staging root"
    );
    sqlx::query(
        "UPDATE local_jobs SET payload_json = json_set(payload_json, '$.lease.expiresAt', datetime('now', '-1 second')) WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind(idempotency_key)
    .execute(&pool)
    .await
    .expect("force finalization failure");
    assert!(
        backup::finalize_backup_create(
            &pool,
            &workspace_id,
            idempotency_key,
            job_type,
            &old_lease,
            &staged.summary,
        )
        .await
        .is_err()
    );
    let backup::BackupCreateClaim::Proceed(retry_lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("retry claim")
    else {
        panic!("expired claim must be reclaimed");
    };
    let adopted = backup::recover_backup_create(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &retry_lease,
        "test-passphrase-12",
    )
    .await
    .expect("verify promoted artifact")
    .expect("matching final artifact is adopted");
    assert_eq!(adopted.backup_path, final_path.to_string_lossy());
    backup::finalize_backup_create(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &retry_lease,
        &adopted,
    )
    .await
    .expect("finalize adopted artifact");
    assert_eq!(
        backup::check_idempotency(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("check cache")
            .expect("cached artifact")
            .backup_path,
        final_path.to_string_lossy()
    );
}

#[tokio::test]
async fn open_existing_workspace_rejects_unregistered_database() {
    let dir = tempdir().expect("tempdir");
    let database_path = dir.path().join("foreign.sqlite");
    let pool = connect_workspace(&database_path).await.expect("connect");
    drop(pool);

    let error = open_existing_workspace(&database_path)
        .await
        .expect_err("should reject empty migrated database without workspace row");

    assert!(error.message.contains("not registered"));
}

#[tokio::test]
async fn backup_staging_is_private_and_removed_after_discard() {
    let (dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "private-staging";
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    let destination = dir.path().join("user-selected-destination");
    std::fs::create_dir_all(&destination).expect("destination");
    let staged = backup::stage_backup_package(
        &pool,
        &workspace_id,
        &dir.path().join(&workspace_id),
        &dir.path().join("workspace.sqlite"),
        &destination.join("private.skatbackup"),
        "test-passphrase-12",
        &lease,
    )
    .await
    .expect("stage package");

    let staging_root = staged.staging_path.parent().expect("private staging root");
    assert!(
        !staged.staging_path.starts_with(&destination),
        "encrypted staging is not created beside the user destination"
    );
    assert!(
        !staged.staging_path.starts_with(dir.path().join(&workspace_id)),
        "cleartext staging is not created in the workspace"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        assert_eq!(
            std::fs::metadata(staging_root).expect("staging metadata").mode() & 0o077,
            0,
            "private staging root is owner-only"
        );
    }

    backup::discard_staged_backup(&staged, &lease).expect("discard staging");
    assert!(
        !staging_root.exists(),
        "discard removes the private staging root as well as its package"
    );
}

#[tokio::test]
async fn backup_staging_root_is_recorded_before_any_build_output() {
    let (_dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "pre-build-staging-root";
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };

    let staging_path = backup::prepare_backup_staging(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &lease,
    )
    .await
    .expect("record private staging root");
    let payload_json: String = sqlx::query_scalar(
        "SELECT payload_json FROM local_jobs WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind(idempotency_key)
    .fetch_one(&pool)
    .await
    .expect("payload");
    let payload: serde_json::Value =
        serde_json::from_str(&payload_json).expect("payload json");
    let staging_path_text = staging_path.to_string_lossy().to_string();
    let staging_root_text = staging_path
        .parent()
        .expect("staging root")
        .to_string_lossy()
        .to_string();

    assert_eq!(
        payload["stagingPath"].as_str(),
        Some(staging_path_text.as_str()),
        "the active lease records the private staging artifact before a build directory can be created"
    );
    assert_eq!(
        payload["stagingRoot"].as_str(),
        Some(staging_root_text.as_str()),
        "the active lease records the private staging root before cleartext can be written"
    );
    assert!(
        !staging_path.parent().expect("staging root").join("build").exists(),
        "recording the root does not write cleartext build output"
    );

    backup::discard_recorded_backup_staging(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &lease,
    )
    .await
    .expect("discard recorded root");
}

#[tokio::test]
async fn reclaim_removes_expired_prebuild_staging_tree() {
    let (_dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "expired-prebuild-staging";
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    let staging_path = backup::prepare_backup_staging(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &lease,
    )
    .await
    .expect("record staging");
    let staging_root = staging_path.parent().expect("staging root").to_path_buf();
    std::fs::create_dir(staging_root.join("build")).expect("build");
    std::fs::write(staging_root.join("build").join("workspace.sqlite"), b"cleartext")
        .expect("cleartext residue");
    sqlx::query(
        "UPDATE local_jobs SET payload_json = json_set(payload_json, '$.lease.expiresAt', datetime('now', '-1 second')) WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind(idempotency_key)
    .execute(&pool)
    .await
    .expect("expire lease");

    let backup::BackupCreateClaim::Proceed(_) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("reclaim")
    else {
        panic!("expired claim must proceed");
    };
    assert!(
        !staging_root.exists(),
        "reclaim removes the recorded expired cleartext staging tree"
    );
}

#[tokio::test]
async fn startup_cleanup_preserves_live_staging_lease() {
    let (_dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "live-prebuild-staging";
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    let staging_path = backup::prepare_backup_staging(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &lease,
    )
    .await
    .expect("record staging");
    let staging_root = staging_path.parent().expect("staging root").to_path_buf();

    backup::cleanup_stale_backup_staging_for_workspace(&pool)
        .await
        .expect("startup cleanup");

    assert!(
        staging_root.exists(),
        "startup cleanup must never remove staging held by a live lease"
    );
    backup::discard_recorded_backup_staging(
        &pool,
        &workspace_id,
        idempotency_key,
        job_type,
        &lease,
    )
    .await
    .expect("discard recorded root");
}

#[tokio::test]
async fn cleanup_refuses_malicious_out_of_root_staging_path() {
    let (dir, pool, workspace_id) = backup_test_workspace().await;
    let job_type = "workspace_backup_create";
    let idempotency_key = "malicious-staging-path";
    let outside_path = dir.path().join("outside").join("package.skatbackup");
    std::fs::create_dir_all(outside_path.parent().expect("outside parent")).expect("outside");
    std::fs::write(&outside_path, b"do not remove").expect("outside artifact");
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(&pool, &workspace_id, idempotency_key, job_type)
            .await
            .expect("claim")
    else {
        panic!("new claim must proceed");
    };
    sqlx::query(
        "UPDATE local_jobs SET status = 'failed', payload_json = json_set(payload_json, '$.stagingPath', ?4, '$.stagingRoot', ?5) WHERE workspace_id = ?1 AND job_type = ?2 AND idempotency_key = ?3",
    )
    .bind(&workspace_id)
    .bind(job_type)
    .bind(idempotency_key)
    .bind(outside_path.to_string_lossy().to_string())
    .bind(outside_path.parent().expect("outside parent").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("inject malicious metadata");
    drop(lease);

    assert!(
        backup::cleanup_stale_backup_staging_for_workspace(&pool)
            .await
            .is_err(),
        "cleanup must reject metadata which does not name a private temp root"
    );
    assert!(outside_path.exists(), "cleanup must not remove out-of-root data");
}

#[tokio::test]
async fn backup_rejects_destinations_inside_workspace_sources() {
    let (dir, pool, workspace_id) = backup_test_workspace().await;
    let data_dir = dir.path().join(&workspace_id);
    let backup::BackupCreateClaim::Proceed(lease) =
        backup::claim_backup_create(
            &pool,
            &workspace_id,
            "overlapping-destination",
            "workspace_backup_create",
        )
        .await
        .expect("claim")
    else {
        panic!("new claim must proceed");
    };

    for destination in [
        data_dir.join("workspace-copy.skatbackup"),
        data_dir.join("documents/evidence-copy.skatbackup"),
        data_dir.join("exports/export-copy.skatbackup"),
    ] {
        let error = backup::stage_backup_package(
            &pool,
            &workspace_id,
            &data_dir,
            &dir.path().join("workspace.sqlite"),
            &destination,
            "test-passphrase-12",
            &lease,
        )
        .await
        .expect_err("overlapping backup destination must be rejected");

        assert_eq!(error.code, "validation_error");
    }
}
