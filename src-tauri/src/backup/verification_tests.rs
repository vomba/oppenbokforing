use super::*;
use tempfile::tempdir;

async fn staged_fixture(root: &Path) -> BackupManifest {
    fs::create_dir_all(root.join("documents")).unwrap();
    fs::create_dir_all(root.join("exports")).unwrap();
    let db = root.join("workspace.sqlite");
    let pool = connect_workspace(&db).await.unwrap();
    sqlx::query("INSERT INTO workspaces (id, name, database_path, documents_path, exports_path) VALUES ('source', 'Source', ?1, ?2, ?3)")
        .bind(db.to_string_lossy().as_ref())
        .bind(root.join("documents").to_string_lossy().as_ref())
        .bind(root.join("exports").to_string_lossy().as_ref())
        .execute(&pool).await.unwrap();
    crate::db::wal_checkpoint_truncate(&pool).await.unwrap();
    pool.close().await;
    BackupManifest {
        version: 1,
        workspace_id: "source".into(),
        workspace_name: "Source".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        entries: collect_backup_entries(root).unwrap(),
        manifest_sha256: String::new(),
    }
}

fn write_manifest(root: &Path, manifest: &mut BackupManifest) {
    let body = serde_json::json!({
        "version": manifest.version,
        "workspaceId": manifest.workspace_id,
        "workspaceName": manifest.workspace_name,
        "createdAt": manifest.created_at,
        "entries": manifest.entries,
    });
    manifest.manifest_sha256 = hash_string(&serde_json::to_string(&body).unwrap());
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn unsupported_manifest_version_is_rejected_before_restore_publication() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("package");
    let mut manifest = staged_fixture(&root).await;
    manifest.version = 999;
    write_manifest(&root, &mut manifest);
    let restores = dir.path().join("restores");
    let error = restore_from_staged_backup(&root, &restores)
        .await
        .expect_err("unsupported version");
    assert!(error.message.contains("version"), "{}", error.message);
    assert!(!restores.exists());
}
