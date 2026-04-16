use anyhow::Result;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub async fn add_todo(pool: &SqlitePool, project_id: &str, title: &str) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO todos (id, project_id, title, status, priority, created_at)
        VALUES (?, ?, ?, 'todo', 0, ?)
        "#,
    )
    .bind(id)
    .bind(project_id)
    .bind(title)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_todos(
    pool: &SqlitePool,
    project_id: Option<&str>,
) -> Result<Vec<core::Todo>> {
    let rows = if let Some(pid) = project_id {
        sqlx::query("SELECT * FROM todos WHERE project_id = ?")
            .bind(pid)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query("SELECT * FROM todos")
            .fetch_all(pool)
            .await?
    };

    let todos = rows
        .into_iter()
        .map(|r| core::Todo {
            id: r.get("id"),
            project_id: r.get("project_id"),
            title: r.get("title"),
            status: r.get("status"),
            priority: r.get("priority"),
            created_at: r.get("created_at"),
        })
        .collect();

    Ok(todos)
}

pub async fn mark_done(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("UPDATE todos SET status = 'done' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_todo(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM todos WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_pending_todos(pool: &SqlitePool) -> Result<Vec<core::Todo>> {
    let rows = sqlx::query("SELECT * FROM todos WHERE status != 'done'")
        .fetch_all(pool)
        .await?;

    let todos = rows
        .into_iter()
        .map(|r| core::Todo {
            id: r.get("id"),
            project_id: r.get("project_id"),
            title: r.get("title"),
            status: r.get("status"),
            priority: r.get("priority"),
            created_at: r.get("created_at"),
        })
        .collect();

    Ok(todos)
}

pub async fn load_todo_context(pool: &SqlitePool, project_id: &str) -> Result<String> {
    let todos = list_todos(pool, Some(project_id)).await?;

    let mut text = String::new();

    for t in todos {
        if t.status != "done" {
            text.push_str(&format!("- {}\n", t.title));
        }
    }

    Ok(text)
}
