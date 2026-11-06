use super::*;
use tempfile::tempdir;

const PASSPHRASE: &str = "test-passphrase-12";

fn write_json_manifest(root: &Path, value: &mut serde_json::Value) {
    let body = serde_json::json!({
        "version": value["version"],
        "workspaceId": value["workspaceId"],
        "workspaceName": value["workspaceName"],
        "createdAt": value["createdAt"],
        "entries": value["entries"],
    });
    value["manifestSha256"] = hash_string(&serde_json::to_string(&body).unwrap()).into();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
}

async fn existing_workspaces(root: &Path) -> Vec<BackupManifestEntry> {
    staged_fixture(&root.join("existing")).await;
    fs::write(
        root.join("existing/documents/keep.pdf"),
        b"existing evidence",
    )
    .unwrap();
    collect_backup_entries(root).unwrap()
}

async fn assert_rejected_unchanged(root: &Path, restores: &Path, expected: &str) {
    let before = collect_backup_entries(restores).unwrap();
    let source_before = collect_backup_entries(root).unwrap();
    let error = restore_from_staged_backup(root, restores)
        .await
        .expect_err(expected);
    assert!(
        error.message.contains(expected),
        "expected {expected}: {}",
        error.message
    );
    assert_eq!(
        collect_backup_entries(restores).unwrap(),
        before,
        "failed restore changed existing workspaces or left staging"
    );
    assert_eq!(
        collect_backup_entries(root).unwrap(),
        source_before,
        "verification changed snapshot files"
    );
}

fn encrypted_package(root: &Path, path: &Path) {
    let tar = crypto::create_tar_archive(root).unwrap();
    fs::write(path, crypto::encrypt_bytes(PASSPHRASE, &tar).unwrap()).unwrap();
}

async fn add_evidence(root: &Path, manifest: &mut BackupManifest) -> String {
    let pool = connect_workspace(&root.join("workspace.sqlite"))
        .await
        .unwrap();
    let document = crate::documents::store_document_bytes(
        &pool,
        "source",
        b"%PDF-1.4 retained receipt",
        "receipt.pdf",
        "application/pdf",
    )
    .await
    .unwrap();
    crate::db::wal_checkpoint_truncate(&pool).await.unwrap();
    pool.close().await;
    manifest.entries = collect_backup_entries(root).unwrap();
    document.object_path
}

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
async fn malformed_manifest_matrix_never_changes_existing_workspaces() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("package");
    let mut manifest = staged_fixture(&root).await;
    write_manifest(&root, &mut manifest);
    let valid = serde_json::to_value(&manifest).unwrap();
    let restores = dir.path().join("restores");
    existing_workspaces(&restores).await;
    let db_index = manifest
        .entries
        .iter()
        .position(|entry| entry.relative_path == "workspace.sqlite")
        .unwrap();
    let dir_index = manifest
        .entries
        .iter()
        .position(|entry| entry.relative_path == "documents/")
        .unwrap();
    let mut cases = Vec::new();
    for field in [
        "version",
        "workspaceId",
        "workspaceName",
        "createdAt",
        "entries",
        "manifestSha256",
    ] {
        let mut value = valid.clone();
        value.as_object_mut().unwrap().remove(field);
        cases.push((format!("missing {field}"), value));
    }
    for (field, wrong) in [
        ("version", serde_json::json!("1")),
        ("workspaceId", serde_json::json!(42)),
        ("workspaceName", serde_json::json!(null)),
        ("createdAt", serde_json::json!([])),
        ("entries", serde_json::json!({})),
        ("manifestSha256", serde_json::json!(true)),
    ] {
        let mut value = valid.clone();
        value[field] = wrong;
        cases.push((format!("wrong type {field}"), value));
    }
    for field in ["relativePath", "sha256", "bytes", "entryType"] {
        let mut value = valid.clone();
        value["entries"][db_index]
            .as_object_mut()
            .unwrap()
            .remove(field);
        cases.push((format!("missing entry {field}"), value));
    }
    for (field, wrong) in [
        ("bytes", serde_json::json!("0")),
        ("bytes", serde_json::json!(-1)),
        ("bytes", serde_json::json!(1.5)),
        ("sha256", serde_json::json!(null)),
        ("relativePath", serde_json::json!(false)),
        ("entryType", serde_json::json!([])),
    ] {
        let mut value = valid.clone();
        value["entries"][db_index][field] = wrong;
        cases.push((format!("wrong entry type {field}"), value));
    }
    let mut value = valid.clone();
    value["unknown"] = true.into();
    cases.push(("unknown manifest field".into(), value));
    let mut value = valid.clone();
    value["entries"][db_index]["unknown"] = true.into();
    cases.push(("unknown entry field".into(), value));
    for (label, mut value) in cases {
        // Type-invalid input still has a self-consistent hash wherever possible.
        if label.contains("manifestSha256") {
            fs::write(
                root.join("manifest.json"),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
        } else {
            write_json_manifest(&root, &mut value);
        }
        assert_rejected_unchanged(&root, &restores, "schema").await;
        assert!(root.join("manifest.json").exists(), "{label}");
    }

    for (field, wrong, expected) in [
        ("version", serde_json::json!(999), "version"),
        ("workspaceId", serde_json::json!(""), "metadata"),
        ("workspaceName", serde_json::json!(" "), "metadata"),
        ("createdAt", serde_json::json!("yesterday"), "metadata"),
        ("workspaceId", serde_json::json!("different"), "identity"),
        ("workspaceName", serde_json::json!("Different"), "identity"),
    ] {
        let mut value = valid.clone();
        value[field] = wrong;
        write_json_manifest(&root, &mut value);
        assert_rejected_unchanged(&root, &restores, expected).await;
    }
    for (field, wrong, expected) in [
        ("entryType", serde_json::json!("symlink"), "entry type"),
        ("sha256", serde_json::json!("invalid"), "file entry"),
        ("sha256", serde_json::json!("A".repeat(64)), "file entry"),
        (
            "sha256",
            serde_json::json!("0".repeat(64)),
            "match manifest",
        ),
        ("bytes", serde_json::json!(0), "match manifest"),
    ] {
        let mut value = valid.clone();
        value["entries"][db_index][field] = wrong;
        write_json_manifest(&root, &mut value);
        assert_rejected_unchanged(&root, &restores, expected).await;
    }
    for (field, wrong) in [
        ("bytes", serde_json::json!(1)),
        ("sha256", serde_json::json!("0".repeat(64))),
    ] {
        let mut value = valid.clone();
        value["entries"][dir_index][field] = wrong;
        write_json_manifest(&root, &mut value);
        assert_rejected_unchanged(&root, &restores, "directory entry").await;
    }
    let mut missing_db = valid.clone();
    missing_db["entries"]
        .as_array_mut()
        .unwrap()
        .remove(db_index);
    write_json_manifest(&root, &mut missing_db);
    assert_rejected_unchanged(&root, &restores, "missing workspace.sqlite").await;
    write_manifest(&root, &mut manifest);
    let mut bad_hash = serde_json::to_value(&manifest).unwrap();
    bad_hash["manifestSha256"] = "0".repeat(64).into();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&bad_hash).unwrap(),
    )
    .unwrap();
    assert_rejected_unchanged(&root, &restores, "hash mismatch").await;
    fs::write(root.join("manifest.json"), b"{broken json").unwrap();
    assert_rejected_unchanged(&root, &restores, "schema").await;
    let raw = serde_json::to_string(&manifest)
        .unwrap()
        .replacen("{", "{\"version\":1,", 1);
    fs::write(root.join("manifest.json"), raw).unwrap();
    assert_rejected_unchanged(&root, &restores, "schema").await;
}

#[tokio::test]
async fn confined_unique_manifest_paths_are_required() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("package");
    let mut manifest = staged_fixture(&root).await;
    let restores = dir.path().join("restores");
    existing_workspaces(&restores).await;
    let db_index = manifest
        .entries
        .iter()
        .position(|entry| entry.relative_path == "workspace.sqlite")
        .unwrap();
    for path in [
        "/tmp/outside",
        "../outside",
        "documents/../../outside",
        "C:/outside",
        "C:outside",
        "\\\\server\\outside",
        "documents\\outside",
        "",
        ".",
        "./workspace.sqlite",
        "documents//file",
        "documents/./file",
        "documents/a/../file",
        "documents/file\0",
    ] {
        let mut invalid = manifest.clone();
        invalid.entries[db_index].relative_path = path.into();
        write_manifest(&root, &mut invalid);
        assert_rejected_unchanged(&root, &restores, "path is invalid").await;
    }
    for entry in [
        manifest.entries[db_index].clone(),
        BackupManifestEntry {
            relative_path: "documents".into(),
            sha256: "0".repeat(64),
            bytes: 0,
            entry_type: "file".into(),
        },
    ] {
        let mut invalid = manifest.clone();
        invalid.entries.push(entry);
        write_manifest(&root, &mut invalid);
        assert_rejected_unchanged(&root, &restores, "duplicate").await;
    }
    // Dots inside an ordinary filename are valid, unlike parent components.
    fs::write(root.join("documents/receipt..pdf"), b"receipt").unwrap();
    manifest.entries = collect_backup_entries(&root).unwrap();
    write_manifest(&root, &mut manifest);
    verify::staged_package(&root).await.unwrap();
}

#[tokio::test]
async fn retained_evidence_is_reconciled_even_when_archive_and_manifest_agree() {
    for failure in [
        "missing file",
        "omitted both",
        "missing document root",
        "corrupt bytes",
        "rehashed corrupt bytes",
        "unsafe database path",
        "missing reference",
    ] {
        let dir = tempdir().unwrap();
        let root = dir.path().join("package");
        let mut manifest = staged_fixture(&root).await;
        let object_path = add_evidence(&root, &mut manifest).await;
        let object = root.join("documents").join(&object_path);
        match failure {
            "missing file" | "omitted both" => fs::remove_file(&object).unwrap(),
            "missing document root" => fs::remove_dir_all(root.join("documents")).unwrap(),
            "corrupt bytes" | "rehashed corrupt bytes" => {
                fs::write(&object, b"corrupted receipt").unwrap()
            }
            "unsafe database path" | "missing reference" => {
                let pool = connect_workspace(&root.join("workspace.sqlite"))
                    .await
                    .unwrap();
                if failure == "unsafe database path" {
                    sqlx::query("UPDATE documents SET object_path = '../outside'")
                        .execute(&pool)
                        .await
                        .unwrap();
                } else {
                    sqlx::query("PRAGMA foreign_keys = OFF")
                        .execute(&pool)
                        .await
                        .unwrap();
                    sqlx::query("INSERT INTO vouchers (id, workspace_id, status, source_type, document_id) VALUES ('broken', 'source', 'draft', 'manual', 'missing-document')")
                        .execute(&pool).await.unwrap();
                }
                crate::db::wal_checkpoint_truncate(&pool).await.unwrap();
                pool.close().await;
            }
            _ => unreachable!(),
        }
        let expected = match failure {
            "missing file" | "corrupt bytes" => "match manifest",
            "unsafe database path" => "path is invalid",
            "missing reference" => "integrity",
            _ => "Retained",
        };
        if !matches!(failure, "missing file" | "corrupt bytes") {
            manifest.entries = collect_backup_entries(&root).unwrap();
        }
        write_manifest(&root, &mut manifest);
        let restores = dir.path().join("restores");
        let before = existing_workspaces(&restores).await;
        assert_rejected_unchanged(&root, &restores, expected).await;
        let archive = dir.path().join("invalid.skatbackup");
        encrypted_package(&root, &archive);
        let original = fs::read(&archive).unwrap();
        assert!(
            verify::artifact(&archive, PASSPHRASE, &manifest)
                .await
                .is_err(),
            "artifact verifier: {failure}"
        );
        let error = restore_backup_package(
            &BackupRestoreInput {
                backup_path: archive.to_string_lossy().into(),
                confirm_overwrite: true,
                passphrase: PASSPHRASE.into(),
            },
            &restores,
        )
        .await
        .expect_err(failure);
        assert!(
            error.message.contains(expected),
            "{failure}: {}",
            error.message
        );
        assert_eq!(collect_backup_entries(&restores).unwrap(), before);
        assert_eq!(fs::read(&archive).unwrap(), original);
    }
}

#[tokio::test]
async fn database_table_metadata_and_complete_inventory_are_checked() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("package");
    let mut manifest = staged_fixture(&root).await;
    let pool = connect_workspace(&root.join("workspace.sqlite"))
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rule_versions")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    manifest.entries.push(BackupManifestEntry {
        relative_path: "rule_versions/".into(),
        sha256: String::new(),
        bytes: count as u64,
        entry_type: "database_table".into(),
    });
    write_manifest(&root, &mut manifest);
    verify::staged_package(&root).await.unwrap();
    let restores = dir.path().join("restores");
    existing_workspaces(&restores).await;
    for (path, count, hash, expected) in [
        ("other_table/", count as u64, "", "database-table"),
        ("rule_versions/", 0, "", "database-table"),
        ("rule_versions/", count as u64 + 1, "", "row count"),
        ("rule_versions/", count as u64, "bad", "database-table"),
    ] {
        let mut invalid = manifest.clone();
        let entry = invalid.entries.last_mut().unwrap();
        entry.relative_path = path.into();
        entry.bytes = count;
        entry.sha256 = hash.into();
        write_manifest(&root, &mut invalid);
        assert_rejected_unchanged(&root, &restores, expected).await;
    }
    write_manifest(&root, &mut manifest);
    fs::write(
        root.join("documents/undeclared.pdf"),
        b"undeclared evidence",
    )
    .unwrap();
    assert_rejected_unchanged(&root, &restores, "match manifest").await;
    fs::remove_file(root.join("documents/undeclared.pdf")).unwrap();
    fs::write(root.join("workspace.sqlite"), b"not a database").unwrap();
    manifest.entries = collect_backup_entries(&root).unwrap();
    write_manifest(&root, &mut manifest);
    let before = collect_backup_entries(&restores).unwrap();
    assert!(restore_from_staged_backup(&root, &restores).await.is_err());
    assert_eq!(collect_backup_entries(&restores).unwrap(), before);
}

#[tokio::test]
async fn creation_rejects_missing_or_corrupt_retained_evidence_and_cleans_private_staging() {
    for corrupt in [false, true] {
        let dir = tempdir().unwrap();
        let root = dir.path().join("source");
        let mut manifest = staged_fixture(&root).await;
        let object_path = add_evidence(&root, &mut manifest).await;
        let object = root.join("documents").join(object_path);
        if corrupt {
            fs::write(&object, b"corrupt retained evidence").unwrap();
        } else {
            fs::remove_file(&object).unwrap();
        }
        let documents_before = collect_backup_entries(&root.join("documents")).unwrap();
        let db = root.join("workspace.sqlite");
        let pool = connect_workspace(&db).await.unwrap();
        let final_path = dir.path().join("rejected.skatbackup");
        let error = create_backup_package(
            &pool,
            "source",
            &root,
            &db,
            dir.path(),
            PASSPHRASE,
            Some(final_path.to_str().unwrap()),
        )
        .await
        .expect_err("incomplete evidence cannot become a successful backup");
        assert!(error.message.contains("Retained"), "{}", error.message);
        let (status, payload): (String, String) = sqlx::query_as("SELECT status, payload_json FROM local_jobs WHERE job_type = 'workspace_backup_create'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status, "failed");
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let staging_root = Path::new(payload["stagingRoot"].as_str().unwrap());
        assert!(
            !staging_root.exists(),
            "failed package left plaintext or encrypted staging"
        );
        assert!(!final_path.exists());
        let successes: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE action = 'workspace_backup_create'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(successes, 0);
        assert_eq!(
            collect_backup_entries(&root.join("documents")).unwrap(),
            documents_before
        );
        pool.close().await;
    }
}

#[tokio::test]
async fn recovery_does_not_adopt_a_published_package_with_evidence_omitted_from_both_inventories() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("source");
    let mut manifest = staged_fixture(&root).await;
    let object_path = add_evidence(&root, &mut manifest).await;
    let db = root.join("workspace.sqlite");
    let pool = connect_workspace(&db).await.unwrap();
    let BackupCreateClaim::Proceed(lease) =
        claim_backup_create(&pool, "source", "recovery", BACKUP_CREATE_JOB_TYPE)
            .await
            .unwrap()
    else {
        panic!("claim");
    };
    let final_path = dir.path().join("interrupted.skatbackup");
    let staged = stage_backup_package_with_claim(
        &pool,
        "source",
        "recovery",
        BACKUP_CREATE_JOB_TYPE,
        &root,
        &db,
        &final_path,
        PASSPHRASE,
        &lease,
    )
    .await
    .unwrap();
    record_backup_staging(
        &pool,
        "source",
        "recovery",
        BACKUP_CREATE_JOB_TYPE,
        &lease,
        &staged,
    )
    .await
    .unwrap();
    publish_staged_backup(
        &pool,
        "source",
        "recovery",
        BACKUP_CREATE_JOB_TYPE,
        &lease,
        &staged,
    )
    .await
    .unwrap();
    let incomplete = dir.path().join("incomplete");
    verify::extract_artifact(&final_path, PASSPHRASE, &incomplete).unwrap();
    fs::remove_file(incomplete.join("documents").join(&object_path)).unwrap();
    let mut incomplete_manifest = staged.summary.manifest.clone();
    incomplete_manifest.entries = collect_backup_entries(&incomplete).unwrap();
    write_manifest(&incomplete, &mut incomplete_manifest);
    encrypted_package(&incomplete, &final_path);
    // Simulate an older producer that recorded the same incomplete manifest as its package.
    sqlx::query("UPDATE local_jobs SET payload_json = json_set(payload_json, '$.summary.manifest', json(?1)) WHERE job_type = ?2")
        .bind(serde_json::to_string(&incomplete_manifest).unwrap()).bind(BACKUP_CREATE_JOB_TYPE).execute(&pool).await.unwrap();
    let published_bytes = fs::read(&final_path).unwrap();
    let documents_before = collect_backup_entries(&root.join("documents")).unwrap();
    assert!(recover_backup_create(
        &pool,
        "source",
        "recovery",
        BACKUP_CREATE_JOB_TYPE,
        &lease,
        PASSPHRASE
    )
    .await
    .is_err());
    assert_eq!(
        fs::read(&final_path).unwrap(),
        published_bytes,
        "recovery must not clobber the preexisting publication"
    );
    assert_eq!(
        collect_backup_entries(&root.join("documents")).unwrap(),
        documents_before
    );
    assert!(!staged.staging_path.parent().unwrap().exists());
    let status: String = sqlx::query_scalar("SELECT status FROM local_jobs WHERE job_type = ?1")
        .bind(BACKUP_CREATE_JOB_TYPE)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "running",
        "failed verification must not finalize success"
    );
    let cached = check_idempotency(&pool, "source", "recovery", BACKUP_CREATE_JOB_TYPE)
        .await
        .unwrap();
    assert!(cached.is_none());
    pool.close().await;
}

#[tokio::test]
async fn late_restore_failure_removes_unpublished_staging_and_preserves_existing_books() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("package");
    let mut manifest = staged_fixture(&root).await;
    let pool = connect_workspace(&root.join("workspace.sqlite"))
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER reject_restore_audit BEFORE INSERT ON audit_events WHEN NEW.action = 'workspace_backup_restore' BEGIN SELECT RAISE(ABORT, 'injected restore failure'); END")
        .execute(&pool).await.unwrap();
    crate::db::wal_checkpoint_truncate(&pool).await.unwrap();
    pool.close().await;
    manifest.entries = collect_backup_entries(&root).unwrap();
    write_manifest(&root, &mut manifest);
    verify::staged_package(&root)
        .await
        .expect("valid before mutation-stage failure");
    let restores = dir.path().join("restores");
    let before = existing_workspaces(&restores).await;
    assert!(restore_from_staged_backup(&root, &restores).await.is_err());
    assert_eq!(collect_backup_entries(&restores).unwrap(), before);
    assert_eq!(verify::staged_package(&root).await.unwrap(), manifest);
}

#[tokio::test]
async fn encrypted_restore_preserves_retained_bytes_identity_and_physical_paths() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("package");
    let mut manifest = staged_fixture(&root).await;
    let object_path = add_evidence(&root, &mut manifest).await;
    write_manifest(&root, &mut manifest);
    let archive = dir.path().join("valid.skatbackup");
    encrypted_package(&root, &archive);
    let restores = dir.path().join("restores");
    let existing_before = existing_workspaces(&restores).await;
    let mut input = BackupRestoreInput {
        backup_path: archive.to_string_lossy().into(),
        confirm_overwrite: false,
        passphrase: PASSPHRASE.into(),
    };
    assert!(restore_backup_package(&input, &restores).await.is_err());
    assert_eq!(collect_backup_entries(&restores).unwrap(), existing_before);
    let existing_files = collect_backup_entries(&restores.join("existing")).unwrap();
    input.confirm_overwrite = true;
    let summary = restore_backup_package(&input, &restores).await.unwrap();
    assert_eq!(summary.workspace_id, "source");
    assert_eq!(summary.workspace_name, "Source");
    let restored = Path::new(&summary.database_path).parent().unwrap();
    assert_ne!(restored, restores.join("existing"));
    assert_eq!(
        collect_backup_entries(&restores.join("existing")).unwrap(),
        existing_files
    );
    let pool = connect_workspace(Path::new(&summary.database_path))
        .await
        .unwrap();
    let (database_path, documents_path, exports_path): (String, String, String) = sqlx::query_as(
        "SELECT database_path, documents_path, exports_path FROM workspaces WHERE id = 'source'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(database_path, summary.database_path);
    assert_eq!(Path::new(&documents_path), restored.join("documents"));
    assert_eq!(Path::new(&exports_path), restored.join("exports"));
    let document_id: String = sqlx::query_scalar("SELECT id FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    crate::documents::verify_retained_document(&pool, "source", &document_id)
        .await
        .unwrap();
    assert_eq!(
        fs::read(restored.join("documents").join(&object_path)).unwrap(),
        b"%PDF-1.4 retained receipt"
    );
    pool.close().await;
}

#[tokio::test]
async fn creation_over_restore_entry_limit_fails_without_publication_and_cleans_staging() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("source");
    staged_fixture(&root).await;
    for i in 0..crypto::MAX_TAR_ENTRIES {
        fs::create_dir(root.join("exports").join(format!("entry-{i}"))).unwrap();
    }
    let db = root.join("workspace.sqlite");
    let pool = connect_workspace(&db).await.unwrap();
    let final_path = dir.path().join("oversized.skatbackup");
    let error = create_backup_package(
        &pool,
        "source",
        &root,
        &db,
        dir.path(),
        PASSPHRASE,
        Some(final_path.to_str().unwrap()),
    )
    .await
    .expect_err("too many entries cannot produce a successful backup");
    assert!(error.message.contains("too many"), "{}", error.message);
    assert!(!final_path.exists());
    let (status, payload): (String, String) = sqlx::query_as(
        "SELECT status, payload_json FROM local_jobs WHERE job_type = 'workspace_backup_create'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "failed");
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert!(!Path::new(payload["stagingRoot"].as_str().unwrap()).exists());
    assert_eq!(
        fs::read_dir(root.join("exports")).unwrap().count(),
        crypto::MAX_TAR_ENTRIES
    );
    pool.close().await;
}

#[tokio::test]
async fn oversized_encrypted_artifacts_are_refused_before_reading_or_decrypting() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("source");
    let manifest = staged_fixture(&root).await;
    let archive = dir.path().join("oversized.skatbackup");
    let file = fs::File::create(&archive).unwrap();
    file.set_len(crypto::MAX_ENCRYPTED_BACKUP_BYTES + 1)
        .unwrap();
    drop(file);
    let restores = dir.path().join("restores");
    let before = existing_workspaces(&restores).await;
    let error = restore_backup_package(
        &BackupRestoreInput {
            backup_path: archive.to_string_lossy().into(),
            confirm_overwrite: true,
            passphrase: PASSPHRASE.into(),
        },
        &restores,
    )
    .await
    .expect_err("encrypted-size bound precedes decryption");
    assert!(error.message.contains("maximum allowed size"));
    let error = verify::artifact(&archive, PASSPHRASE, &manifest)
        .await
        .unwrap_err();
    assert!(error.message.contains("maximum allowed size"));
    assert_eq!(collect_backup_entries(&restores).unwrap(), before);
    assert_eq!(
        fs::metadata(&archive).unwrap().len(),
        crypto::MAX_ENCRYPTED_BACKUP_BYTES + 1
    );
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
