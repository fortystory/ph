use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::fs::data_dir;

pub fn init_vec_extension() {
    unsafe {
        sqlite3_auto_extension(Some(sqlite_vec::sqlite3_vec_init));
    }
}

// FFI binding for sqlite3_auto_extension - avoids depending on rusqlite or libsqlite3-sys directly
// This function registers an extension so it's available in all subsequent sqlite3 connections
#[allow(non_camel_case_types)]
type sqlite3_init_fn = Option<unsafe extern "C" fn()>;

extern "C" {
    fn sqlite3_auto_extension(x: sqlite3_init_fn);
}

pub async fn init_db() -> anyhow::Result<SqlitePool> {
    init_vec_extension();

    std::fs::create_dir_all(data_dir())?;

    let db_path = data_dir().join("todos.db");

    let options = SqliteConnectOptions::from_str("")?
        .filename(&db_path)
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(options).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS todos (
            id TEXT PRIMARY KEY,
            project_id TEXT,
            title TEXT,
            status TEXT,
            priority INTEGER,
            created_at TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query("ALTER TABLE todos ADD COLUMN completed_at TEXT")
        .execute(&pool)
        .await
        .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS todo_stage_logs (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            priority INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            stage TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS knowledge_chunks (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            content TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS knowledge_files (
            project_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            mtime TEXT NOT NULL,
            PRIMARY KEY (project_id, file_path)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // vec0 virtual table - only create if it doesn't exist
    // Using raw SQL since CREATE VIRTUAL TABLE doesn't support IF NOT EXISTS in some sqlite-vec versions
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_vec USING vec0(embedding float[384])",
    )
    .execute(&pool)
    .await
    .ok();

    Ok(pool)
}
