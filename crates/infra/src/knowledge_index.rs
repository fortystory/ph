use anyhow::Result;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::chunk::{chunk_markdown, content_hash};
use crate::embed::Embedder;
use crate::fs::knowledge_dir;

pub async fn is_stale(pool: &SqlitePool, project_id: &str) -> Result<bool> {
    let base = knowledge_dir().join(project_id);
    if !base.exists() {
        return Ok(false);
    }

    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let rel = path
            .strip_prefix(&base)?
            .to_string_lossy()
            .to_string();

        let mtime = std::fs::metadata(&path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .to_string();

        let stored: Option<String> = sqlx::query_scalar(
            "SELECT mtime FROM knowledge_files WHERE project_id = ? AND file_path = ?",
        )
        .bind(project_id)
        .bind(&rel)
        .fetch_optional(pool)
        .await?;

        match stored {
            Some(existing) if existing == mtime => continue,
            _ => return Ok(true),
        }
    }

    // Check for deleted files
    let indexed_files: Vec<String> = sqlx::query_scalar(
        "SELECT file_path FROM knowledge_files WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    for file in &indexed_files {
        let full_path = knowledge_dir().join(project_id).join(file);
        if !full_path.exists() {
            return Ok(true);
        }
    }

    Ok(false)
}

pub async fn update_index(
    pool: &SqlitePool,
    project_id: &str,
    embedder: &Embedder,
) -> Result<()> {
    let base = knowledge_dir().join(project_id);
    if !base.exists() {
        return Ok(());
    }

    // Get current indexed files and their mtimes
    let indexed: Vec<(String, String)> = sqlx::query(
        "SELECT file_path, mtime FROM knowledge_files WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.get("file_path"), r.get("mtime")))
    .collect();

    // Collect all current files and their mtimes
    let mut current_files: Vec<(String, String)> = Vec::new();
    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(&base)?
            .to_string_lossy()
            .to_string();
        let mtime = std::fs::metadata(&path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .to_string();
        current_files.push((rel, mtime));
    }

    // Delete removed files from index
    for (indexed_path, _) in &indexed {
        if !current_files.iter().any(|(p, _)| p == indexed_path) {
            delete_file_chunks(pool, project_id, indexed_path).await?;
            sqlx::query(
                "DELETE FROM knowledge_files WHERE project_id = ? AND file_path = ?",
            )
            .bind(project_id)
            .bind(indexed_path)
            .execute(pool)
            .await?;
        }
    }

    // Update changed files
    for (rel, mtime) in &current_files {
        let needs_update = !indexed.iter().any(|(p, m)| p == rel && m == mtime);
        if !needs_update {
            continue;
        }

        let full_path = knowledge_dir().join(project_id).join(rel);
        let content = std::fs::read_to_string(&full_path)?;
        let hash = content_hash(&content);

        // Delete old chunks for this file
        delete_file_chunks(pool, project_id, rel).await?;

        // Chunk the file
        let chunks = chunk_markdown(&content, rel);
        if chunks.is_empty() {
            continue;
        }

        // Generate embeddings
        let texts: Vec<String> = chunks.iter().map(|(_, c)| c.clone()).collect();
        let embeddings = embedder.embed(&texts)?;

        // Insert chunks and vectors
        let now = Utc::now().to_rfc3339();
        for ((_chunk_id, chunk_content), embedding) in chunks.iter().zip(embeddings.iter()) {
            let id = Uuid::new_v4().to_string();

            sqlx::query(
                "INSERT INTO knowledge_chunks (id, project_id, file_path, chunk_index, content, content_hash, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(project_id)
            .bind(rel)
            .bind(0i32)
            .bind(chunk_content)
            .bind(&hash)
            .bind(&now)
            .execute(pool)
            .await?;

            let raw_bytes = f32_vec_to_bytes(embedding);
            sqlx::query(
                "INSERT INTO knowledge_vec(rowid, embedding) VALUES (?, ?)",
            )
            .bind(sqlite_rowid(&id))
            .bind(raw_bytes)
            .execute(pool)
            .await?;
        }

        // Update file mtime record
        sqlx::query(
            "INSERT OR REPLACE INTO knowledge_files (project_id, file_path, mtime) VALUES (?, ?, ?)",
        )
        .bind(project_id)
        .bind(rel)
        .bind(mtime)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn search(
    pool: &SqlitePool,
    project_id: &str,
    query_embedding: Vec<f32>,
    top_k: i64,
) -> Result<Vec<core::KnowledgeChunk>> {
    let raw_bytes = f32_vec_to_bytes(&query_embedding);

    // Step 1: KNN search on vec table to get top-k rowids
    let vec_rows = sqlx::query(
        "SELECT rowid, distance FROM knowledge_vec WHERE embedding MATCH ? ORDER BY distance LIMIT ?",
    )
    .bind(&raw_bytes)
    .bind(top_k)
    .fetch_all(pool)
    .await?;

    if vec_rows.is_empty() {
        return Ok(vec![]);
    }

    // Step 2: Load full chunk metadata
    let mut chunks = Vec::new();
    for row in vec_rows {
        let target_rowid: i64 = row.get("rowid");
        // Find the chunk whose UUID hash matches this rowid
        let all_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM knowledge_chunks WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        for id in all_ids {
            if sqlite_rowid(&id) == target_rowid {
                let r = sqlx::query(
                    "SELECT id, project_id, file_path, chunk_index, content, content_hash, updated_at FROM knowledge_chunks WHERE id = ?",
                )
                .bind(&id)
                .fetch_one(pool)
                .await?;

                chunks.push(core::KnowledgeChunk {
                    id: r.get("id"),
                    project_id: r.get("project_id"),
                    file_path: r.get("file_path"),
                    chunk_index: r.get("chunk_index"),
                    content: r.get("content"),
                    content_hash: r.get("content_hash"),
                    updated_at: r.get("updated_at"),
                });
                break;
            }
        }
    }

    Ok(chunks)
}

pub async fn clear_index(pool: &SqlitePool, project_id: &str) -> Result<()> {
    // Get all chunk IDs for this project to delete their vectors
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM knowledge_chunks WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    for id in &ids {
        sqlx::query("DELETE FROM knowledge_vec WHERE rowid = ?")
            .bind(sqlite_rowid(id))
            .execute(pool)
            .await?;
    }

    sqlx::query("DELETE FROM knowledge_chunks WHERE project_id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM knowledge_files WHERE project_id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;

    Ok(())
}

async fn delete_file_chunks(
    pool: &SqlitePool,
    project_id: &str,
    file_path: &str,
) -> Result<()> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM knowledge_chunks WHERE project_id = ? AND file_path = ?",
    )
    .bind(project_id)
    .bind(file_path)
    .fetch_all(pool)
    .await?;

    for id in &ids {
        sqlx::query("DELETE FROM knowledge_vec WHERE rowid = ?")
            .bind(sqlite_rowid(id))
            .execute(pool)
            .await?;
    }

    sqlx::query(
        "DELETE FROM knowledge_chunks WHERE project_id = ? AND file_path = ?",
    )
    .bind(project_id)
    .bind(file_path)
    .execute(pool)
    .await?;

    Ok(())
}

fn f32_vec_to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn sqlite_rowid(id: &str) -> i64 {
    // Use a deterministic hash of the UUID as an i64 rowid
    // This ensures vec0 rowid matches knowledge_chunks.id via a lookup
    let mut hash = 0i64;
    for byte in id.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as i64);
    }
    hash.abs().max(1)
}
