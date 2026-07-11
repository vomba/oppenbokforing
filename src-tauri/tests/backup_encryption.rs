use oppenbokforing_desktop_lib::{
    backup::{self, backup_plaintext_is_sqlite, BackupRestoreInput},
    db::connect_workspace,
    profiles::{self, TaxProfileSaveInput, VatProfileSaveInput},
};
use tempfile::tempdir;
use uuid::Uuid;

const PASSPHRASE: &str = "portable-passphrase";

#[tokio::test]
async fn encrypted_backup_rejects_wrong_passphrase() {
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
    .bind("Encryption workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "fa_skatt".to_string(),
            expected_business_profit_minor: Some(1_000_000),
            expected_salary_income_minor: Some(2_000_000),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("tax");

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("vat");

    let backup = backup::create_backup_package(
        &pool,
        &workspace_id,
        &data_dir,
        &database_path,
        &data_dir.join("exports"),
        PASSPHRASE,
        None,
    )
    .await
    .expect("backup");

    let bytes = std::fs::read(&backup.backup_path).expect("read backup");
    assert!(!backup_plaintext_is_sqlite(&bytes));

    let restore_root = dir.path().join("restored");
    std::fs::create_dir_all(&restore_root).expect("restore root");

    let err = backup::restore_backup_package(
        &BackupRestoreInput {
            backup_path: backup.backup_path.clone(),
            confirm_overwrite: true,
            passphrase: "wrong-passphrase".to_string(),
        },
        &restore_root,
    )
    .await
    .expect_err("wrong passphrase");

    assert!(err.message.contains("passphrase"));
}

#[tokio::test]
async fn encrypted_backup_round_trip_preserves_profiles() {
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
    .bind("Round trip workspace")
    .bind(database_path.to_string_lossy().to_string())
    .bind(data_dir.join("documents").to_string_lossy().to_string())
    .bind(data_dir.join("exports").to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("workspace row");

    profiles::save_tax_profile(
        &pool,
        &workspace_id,
        &TaxProfileSaveInput {
            tax_status: "fa_skatt".to_string(),
            expected_business_profit_minor: Some(1_000_000),
            expected_salary_income_minor: Some(2_000_000),
            active_rule_year: Some(2026),
        },
    )
    .await
    .expect("tax");

    profiles::save_vat_profile(
        &pool,
        &workspace_id,
        &VatProfileSaveInput {
            vat_status: "registered".to_string(),
            reporting_period: "quarterly".to_string(),
            accounting_method: "invoice_method".to_string(),
            voluntary_registration_date: None,
        },
    )
    .await
    .expect("vat");

    let backup = backup::create_backup_package(
        &pool,
        &workspace_id,
        &data_dir,
        &database_path,
        &data_dir.join("exports"),
        PASSPHRASE,
        None,
    )
    .await
    .expect("backup");

    let restore_root = dir.path().join("restored");
    std::fs::create_dir_all(&restore_root).expect("restore root");

    let restored = backup::restore_backup_package(
        &BackupRestoreInput {
            backup_path: backup.backup_path,
            confirm_overwrite: true,
            passphrase: PASSPHRASE.to_string(),
        },
        &restore_root,
    )
    .await
    .expect("restore");

    let restored_pool = connect_workspace(std::path::Path::new(&restored.database_path))
        .await
        .expect("restored pool");
    let preserved = backup::profiles_preserved_after_restore(&restored_pool, &restored.workspace_id)
        .await
        .expect("profiles");
    assert!(preserved);
}
