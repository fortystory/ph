use anyhow::Result;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub async fn add_todo(pool: &SqlitePool, project_id: &str, title: &str) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    insert_todo(pool, &id, project_id, title, "todo", 0, &now).await
}

pub async fn insert_todo(
    pool: &SqlitePool,
    id: &str,
    project_id: &str,
    title: &str,
    status: &str,
    priority: i32,
    created_at: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO todos (id, project_id, title, status, priority, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id)
    .bind(project_id)
    .bind(title)
    .bind(status)
    .bind(priority)
    .bind(created_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_todos_by_ids(
    pool: &SqlitePool,
    ids: &[String],
    title: &str,
    status: &str,
    priority: i32,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "UPDATE todos SET title = ?, status = ?, priority = ? WHERE id IN ({})",
        placeholders
    );
    let mut query = sqlx::query(&sql).bind(title).bind(status).bind(priority);
    for id in ids {
        query = query.bind(id);
    }
    query.execute(pool).await?;
    Ok(())
}

pub async fn delete_todos_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM todos WHERE id IN ({})", placeholders);
    let mut query = sqlx::query(&sql);
    for id in ids {
        query = query.bind(id);
    }
    query.execute(pool).await?;
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
            completed_at: r.get("completed_at"),
        })
        .collect();

    Ok(todos)
}

pub async fn mark_done_with_time(pool: &SqlitePool, id: &str, completed_at: &str) -> Result<()> {
    sqlx::query("UPDATE todos SET status = 'done', completed_at = ? WHERE id = ?")
        .bind(completed_at)
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

pub async fn delete_todos_by_group(
    pool: &SqlitePool,
    title: &str,
    priority: i32,
    created_at: &str,
    project_id: &str,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM todos WHERE title = ? AND priority = ? AND created_at = ? AND project_id = ?"
    )
    .bind(title)
    .bind(priority)
    .bind(created_at)
    .bind(project_id)
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
            completed_at: r.get("completed_at"),
        })
        .collect();

    Ok(todos)
}

pub async fn insert_stage_log(
    pool: &SqlitePool,
    id: &str,
    title: &str,
    priority: i32,
    created_at: &str,
    stage: &str,
    started_at: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO todo_stage_logs (id, title, priority, created_at, stage, started_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id)
    .bind(title)
    .bind(priority)
    .bind(created_at)
    .bind(stage)
    .bind(started_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn end_stage_log(pool: &SqlitePool, id: &str, ended_at: &str) -> Result<()> {
    sqlx::query("UPDATE todo_stage_logs SET ended_at = ? WHERE id = ?")
        .bind(ended_at)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_stage_logs(
    pool: &SqlitePool,
    title: &str,
    priority: i32,
    created_at: &str,
) -> Result<Vec<core::TodoStageLog>> {
    let rows = sqlx::query(
        "SELECT * FROM todo_stage_logs WHERE title = ? AND priority = ? AND created_at = ? ORDER BY started_at"
    )
    .bind(title)
    .bind(priority)
    .bind(created_at)
    .fetch_all(pool)
    .await?;

    let logs = rows
        .into_iter()
        .map(|r| core::TodoStageLog {
            id: r.get("id"),
            title: r.get("title"),
            priority: r.get("priority"),
            created_at: r.get("created_at"),
            stage: r.get("stage"),
            started_at: r.get("started_at"),
            ended_at: r.get("ended_at"),
        })
        .collect();

    Ok(logs)
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
