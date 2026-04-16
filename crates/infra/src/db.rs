use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::fs::data_dir;

pub async fn init_db() -> anyhow::Result<SqlitePool> {
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

    Ok(pool)
}
