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
async fn hash_valid_backup_with_stale_secondary_index_is_rejected() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("package");
    let mut manifest = staged_fixture(&root).await;
    let db = root.join("workspace.sqlite");
    let pool = connect_workspace(&db).await.unwrap();
    for (id, number) in [("account-A", "1930"), ("account-B", "1940")] {
        sqlx::query("INSERT INTO accounts (id, workspace_id, number, name, account_type, normal_balance) VALUES (?1, 'source', ?2, 'Bank', 'asset', 'debit')")
            .bind(id).bind(number).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO vouchers (id, workspace_id, status, source_type) VALUES ('voucher', 'source', 'draft', 'manual')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO journal_lines (id, voucher_id, account_id, debit_minor) VALUES ('line', 'voucher', 'account-A', 100)")
        .execute(&pool).await.unwrap();
    let page: i64 = sqlx::query_scalar(
        "SELECT rootpage FROM sqlite_master WHERE name = 'idx_journal_lines_account'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&pool)
        .await
        .unwrap();
    crate::db::wal_checkpoint_truncate(&pool).await.unwrap();
    pool.close().await;

    // Corrupt only the index key, leaving table records and the B-tree structure intact.
    let mut bytes = fs::read(&db).unwrap();
    let start = ((page - 1) * page_size) as usize;
    let index_page = &mut bytes[start..start + page_size as usize];
    let offsets: Vec<_> = index_page
        .windows(9)
        .enumerate()
        .filter_map(|(offset, key)| (key == b"account-A").then_some(offset))
        .collect();
    assert_eq!(offsets.len(), 1);
    index_page[offsets[0]..offsets[0] + 9].copy_from_slice(b"account-B");
    fs::write(&db, bytes).unwrap();

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db)
                .read_only(true)
                .immutable(true),
        )
        .await
        .unwrap();
    let quick: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(quick, "ok", "the old gate must miss this corruption");
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_optional(&pool)
        .await
        .unwrap()
        .is_none());
    let full: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(full.contains("idx_journal_lines_account"), "{full}");
    let indexed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines INDEXED BY idx_journal_lines_account WHERE account_id = 'account-A'")
        .fetch_one(&pool).await.unwrap();
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_lines NOT INDEXED WHERE account_id = 'account-A'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((indexed, table), (0, 1));
    pool.close().await;

    manifest.entries = collect_backup_entries(&root).unwrap();
    write_manifest(&root, &mut manifest);
    let restores = dir.path().join("restores");
    let error = restore_from_staged_backup(&root, &restores)
        .await
        .expect_err("hash-valid index corruption must not restore");
    assert!(error.message.contains("integrity"), "{}", error.message);
    assert!(!restores.exists());
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
