use oppenbokforing_desktop_lib::db::connect_workspace;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn local_jobs_rejects_duplicate_idempotency_key() {
    let dir = tempdir().expect("tempdir");
    let database_path = dir.path().join("workspace.sqlite");
    let pool = connect_workspace(&database_path).await.expect("connect");
    let workspace_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind("Idempotency constraint workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(dir.path().join("documents").to_string_lossy().to_string())
    .bind(dir.path().join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace");

    let first = sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&workspace_id)
    .bind("sie_export_create")
    .bind(r#"{"summary":{"exportPath":"sie/test.se"}}"#)
    .bind("shared-export-key")
    .execute(&pool)
    .await;
    assert!(first.is_ok());

    let duplicate = sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json, idempotency_key)
        VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&workspace_id)
    .bind("sie_export_create")
    .bind(r#"{"summary":{"exportPath":"sie/other.se"}}"#)
    .bind("shared-export-key")
    .execute(&pool)
    .await;
    assert!(duplicate.is_err());
    assert!(
        duplicate
            .expect_err("duplicate insert")
            .to_string()
            .contains("UNIQUE constraint failed")
    );
}
