use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};

use crate::error::AppError;

async fn connect_pool(database_path: &Path, create_if_missing: bool) -> Result<SqlitePool, AppError> {
    let connect_options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(create_if_missing);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(connect_options)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA journal_mode = WAL;")
        .execute(&pool)
        .await?;

    Ok(pool)
}

async fn run_migrations(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Opens or creates a workspace database and applies migrations.
pub async fn connect_workspace(database_path: &Path) -> Result<SqlitePool, AppError> {
    let pool = connect_pool(database_path, true).await?;
    run_migrations(&pool).await?;
    Ok(pool)
}

/// Opens an existing workspace without creating a new database file.
/// Validates the file is a registered ÖppenBokföring workspace before migrating.
pub async fn open_existing_workspace(database_path: &Path) -> Result<SqlitePool, AppError> {
    if !database_path.exists() {
        return Err(AppError::validation(
            "Workspace database not found",
            "databasePath",
        ));
    }

    let pool = connect_pool(database_path, false).await?;

    let table_exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1 FROM sqlite_master
        WHERE type = 'table' AND name = 'workspaces'
        LIMIT 1
        "#,
    )
    .fetch_optional(&pool)
    .await?;

    if table_exists.is_none() {
        return Err(AppError::validation(
            "Not a valid ÖppenBokföring workspace database",
            "databasePath",
        ));
    }

    let database_path_value = database_path.to_string_lossy().to_string();
    let row = sqlx::query(
        r#"
        SELECT id FROM workspaces WHERE database_path = ?1 LIMIT 1
        "#,
    )
    .bind(&database_path_value)
    .fetch_optional(&pool)
    .await?;

    if row.is_none() {
        return Err(AppError::validation(
            "Workspace not registered in database",
            "databasePath",
        ));
    }

    run_migrations(&pool).await?;
    Ok(pool)
}

/// Asserts a WAL checkpoint completed successfully before copying the database file.
pub async fn wal_checkpoint_truncate(pool: &SqlitePool) -> Result<(), AppError> {
    let row = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE);")
        .fetch_one(pool)
        .await?;

    let busy: i64 = row.try_get(0).unwrap_or(1);
    let log: i64 = row.try_get(1).unwrap_or(0);
    let checkpointed: i64 = row.try_get(2).unwrap_or(0);
    if busy != 0 {
        return Err(AppError::storage(
            "WAL checkpoint blocked by active transaction; retry backup when workspace is idle",
        ));
    }
    if log > 0 && checkpointed < log {
        return Err(AppError::storage(
            "WAL checkpoint incomplete; retry backup when workspace is idle",
        ));
    }
    Ok(())
}

/// Creates a consistent on-disk SQLite snapshot using `VACUUM INTO`.
pub async fn vacuum_database_into(pool: &SqlitePool, destination: &Path) -> Result<(), AppError> {
    if destination.exists() {
        std::fs::remove_file(destination).map_err(AppError::from)?;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::from)?;
    }
    let escaped = destination.to_string_lossy().replace('\'', "''");
    let sql = format!("VACUUM INTO '{escaped}'");
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}
