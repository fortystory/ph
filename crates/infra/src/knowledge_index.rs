use anyhow::Result;
use chrono::Utc;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, DeletePointsBuilder, Distance, PointStruct,
    SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::Qdrant;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::chunk::{chunk_markdown, content_hash};
use crate::embed::Embedder;
use crate::fs::knowledge_dir;

fn collect_files(base: &std::path::Path) -> Result<Vec<(std::path::PathBuf, String)>> {
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, base: &std::path::Path, files: &mut Vec<(std::path::PathBuf, String)>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let rel = path.strip_prefix(base)?.to_string_lossy().to_string();
                files.push((path, rel));
            } else if path.is_dir() {
                walk(&path, base, files)?;
            }
        }
        Ok(())
    }
    walk(base, base, &mut files)?;
    Ok(files)
}

const EMBEDDING_DIM: u64 = 384;

fn qdrant_client() -> Result<Qdrant> {
    let url = std::env::var("QDRANT_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:6334".to_string());
    eprintln!("[qdrant] connecting to {}", url);
    let client = Qdrant::from_url(&url)
        .skip_compatibility_check()
        .timeout(std::time::Duration::from_secs(60))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;
    eprintln!("[qdrant] client built ok");
    Ok(client)
}

fn collection_name(project_id: &str) -> String {
    format!("ph-knowledge-{}", project_id)
}

async fn ensure_collection(
    client: &Qdrant,
    collection: &str,
) -> Result<()> {
    eprintln!("[qdrant] checking collection '{}'", collection);
    let exists = client.collection_exists(collection).await?;
    eprintln!("[qdrant] collection exists = {}", exists);
    if !exists {
        eprintln!("[qdrant] creating collection '{}'", collection);
        client
            .create_collection(
                CreateCollectionBuilder::new(collection)
                    .vectors_config(VectorParamsBuilder::new(
                        EMBEDDING_DIM,
                        Distance::Cosine,
                    )),
            )
            .await?;
        eprintln!("[qdrant] collection created ok");
    }
    Ok(())
}

pub async fn is_stale(pool: &SqlitePool, project_id: &str) -> Result<bool> {
    let base = knowledge_dir().join(project_id);
    if !base.exists() {
        return Ok(false);
    }

    let files = collect_files(&base)?;

    for (path, rel) in &files {
        let mtime = std::fs::metadata(path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .to_string();

        let stored: Option<String> = sqlx::query_scalar(
            "SELECT mtime FROM knowledge_files WHERE project_id = ? AND file_path = ?",
        )
        .bind(project_id)
        .bind(rel)
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
        eprintln!("[qdrant] knowledge dir not found for {}", project_id);
        return Ok(());
    }

    eprintln!("[qdrant] update_index start for project_id={}", project_id);
    let client = qdrant_client()?;
    let collection = collection_name(project_id);
    ensure_collection(&client, &collection).await?;

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
    eprintln!("[qdrant] indexed files count={}", indexed.len());

    // Collect all current files and their mtimes
    let mut current_files: Vec<(String, String)> = Vec::new();
    for (path, rel) in collect_files(&base)? {
        let mtime = std::fs::metadata(&path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .to_string();
        current_files.push((rel, mtime));
    }
    eprintln!("[qdrant] current files count={}", current_files.len());

    // Delete removed files from index
    for (indexed_path, _) in &indexed {
        if !current_files.iter().any(|(p, _)| p == indexed_path) {
            eprintln!("[qdrant] deleting removed file '{}'", indexed_path);
            delete_file_chunks(pool, &client, &collection, project_id, indexed_path).await?;
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

        eprintln!("[qdrant] updating file '{}'", rel);
        let full_path = knowledge_dir().join(project_id).join(rel);
        let content = std::fs::read_to_string(&full_path)?;
        let hash = content_hash(&content);

        // Delete old chunks for this file from Qdrant and SQLite
        delete_file_chunks(pool, &client, &collection, project_id, rel).await?;

        // Chunk the file
        let chunks = chunk_markdown(&content, rel);
        if chunks.is_empty() {
            eprintln!("[qdrant] no chunks for '{}'", rel);
            continue;
        }
        eprintln!("[qdrant] chunks count={} for '{}'", chunks.len(), rel);

        // Generate embeddings
        eprintln!("[qdrant] generating embeddings...");
        let texts: Vec<String> = chunks.iter().map(|(_, c)| c.clone()).collect();
        let embeddings = embedder.embed(&texts)?;
        eprintln!("[qdrant] embeddings generated count={}", embeddings.len());

        // Insert chunks into SQLite metadata and build Qdrant points
        let now = Utc::now().to_rfc3339();
        let mut points: Vec<PointStruct> = Vec::new();

        for (chunk_idx, ((_chunk_id, chunk_content), embedding)) in
            chunks.iter().zip(embeddings.iter()).enumerate()
        {
            let id = Uuid::new_v4().to_string();

            sqlx::query(
                "INSERT INTO knowledge_chunks (id, project_id, file_path, chunk_index, content, content_hash, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(project_id)
            .bind(rel)
            .bind(chunk_idx as i32)
            .bind(chunk_content)
            .bind(&hash)
            .bind(&now)
            .execute(pool)
            .await?;

            let payload: std::collections::HashMap<String, serde_json::Value> =
                [
                    ("project_id".to_string(), serde_json::Value::String(project_id.to_string())),
                    ("file_path".to_string(), serde_json::Value::String(rel.to_string())),
                    ("content_hash".to_string(), serde_json::Value::String(hash.clone())),
                    ("chunk_index".to_string(), serde_json::Value::Number((chunk_idx as i64).into())),
                    ("content".to_string(), serde_json::Value::String(chunk_content.clone())),
                    ("updated_at".to_string(), serde_json::Value::String(now.clone())),
                ]
                .into_iter()
                .collect();

            points.push(PointStruct::new(
                id,
                embedding.clone(),
                payload,
            ));
        }

        // Upsert all points for this file into Qdrant
        if !points.is_empty() {
            eprintln!("[qdrant] upserting {} points for '{}'", points.len(), rel);
            client
                .upsert_points(
                    UpsertPointsBuilder::new(collection.clone(), points),
                )
                .await?;
            eprintln!("[qdrant] upsert ok for '{}'", rel);
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

    eprintln!("[qdrant] update_index done for project_id={}", project_id);
    Ok(())
}

pub async fn search(
    _pool: &SqlitePool,
    project_id: &str,
    query_embedding: Vec<f32>,
    top_k: i64,
) -> Result<Vec<core::KnowledgeChunk>> {
    eprintln!("[qdrant] search start project_id={} top_k={}", project_id, top_k);
    let client = qdrant_client()?;
    let collection = collection_name(project_id);

    eprintln!("[qdrant] searching collection '{}'", collection);
    let result = client
        .search_points(
            SearchPointsBuilder::new(
                collection,
                query_embedding,
                top_k as u64,
            )
            .with_payload(true),
        )
        .await?;
    eprintln!("[qdrant] search returned {} results", result.result.len());

    let mut chunks = Vec::new();
    for scored_point in result.result {
        let payload = scored_point.payload;
        let get_str = |key: &str| {
            payload
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default()
        };
        let get_i32 = |key: &str| {
            payload
                .get(key)
                .and_then(|v| v.as_integer())
                .unwrap_or(0) as i32
        };

        chunks.push(core::KnowledgeChunk {
            id: scored_point.id.map(|pid| format!("{:?}", pid)).unwrap_or_default(),
            project_id: get_str("project_id"),
            file_path: get_str("file_path"),
            chunk_index: get_i32("chunk_index"),
            content: get_str("content"),
            content_hash: get_str("content_hash"),
            updated_at: get_str("updated_at"),
        });
    }

    Ok(chunks)
}

pub async fn clear_index(pool: &SqlitePool, project_id: &str) -> Result<()> {
    let client = qdrant_client()?;
    let collection = collection_name(project_id);

    // Delete the collection from Qdrant (ignore errors if it doesn't exist)
    let _ = client.delete_collection(&collection).await;

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
    client: &Qdrant,
    collection: &str,
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

    if !ids.is_empty() {
        eprintln!("[qdrant] deleting {} old points for '{}'", ids.len(), file_path);
        client
            .delete_points(
                DeletePointsBuilder::new(collection.to_string())
                    .points(ids),
            )
            .await?;
        eprintln!("[qdrant] delete_points ok");
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
