use oppenbokforing_desktop_lib::{
    backup::{self, BackupCreateInput},
    db::{connect_workspace, open_existing_workspace},
};
use tempfile::tempdir;
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
        &data_dir.join("exports"),
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
