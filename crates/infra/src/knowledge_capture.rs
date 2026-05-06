use anyhow::Result;
use sqlx::SqlitePool;
use std::path::Path;
use uuid::Uuid;

use crate::fs::knowledge_dir;

/// Write a markdown file to the knowledge directory for a project.
/// `subdir` is one of: "conversations", "commits", "live"
/// Returns the relative path of the written file.
pub fn write_knowledge_file(
    project_id: &str,
    subdir: &str,
    filename: &str,
    content: &str,
) -> Result<String> {
    let dir = knowledge_dir().join(project_id).join(subdir);
    std::fs::create_dir_all(&dir)?;

    let safe_name = sanitize_filename(filename);
    let path = dir.join(&safe_name);
    std::fs::write(&path, content)?;

    Ok(format!("{}/{}", subdir, safe_name))
}

/// Parse a Claude Code session JSONL file.
/// Returns a list of (role, content) pairs for user and assistant messages.
pub fn parse_session(path: &Path) -> Result<Vec<(String, String)>> {
    let content = std::fs::read_to_string(path)?;
    let mut messages = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // Claude Code JSONL format: each line is a JSON object
        // Look for message entries with role and content
        let role = val.get("role").and_then(|v| v.as_str());
        let msg_type = val.get("type").and_then(|v| v.as_str());

        match (role, msg_type) {
            (Some("user"), _) | (_, Some("human")) => {
                if let Some(text) = extract_text_content(&val) {
                    if !text.is_empty() && text.len() > 10 {
                        messages.push(("user".to_string(), text));
                    }
                }
            }
            (Some("assistant"), _) | (_, Some("assistant")) => {
                if let Some(text) = extract_text_content(&val) {
                    if !text.is_empty() && text.len() > 10 {
                        messages.push(("assistant".to_string(), text));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(messages)
}

fn extract_text_content(val: &serde_json::Value) -> Option<String> {
    // Try content as string
    if let Some(s) = val.get("content").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }

    // Try content as array of content blocks
    if let Some(arr) = val.get("content").and_then(|v| v.as_array()) {
        let mut text = String::new();
        for block in arr {
            if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
            }
        }
        if !text.is_empty() {
            return Some(text);
        }
    }

    // Try message.content (nested format)
    if let Some(msg) = val.get("message") {
        return extract_text_content(msg);
    }

    None
}

fn sanitize_filename(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // Collapse multiple dashes
    while result.contains("--") {
        result = result.replace("--", "-");
    }

    // Trim and limit length
    result = result.trim_matches('-').to_string();
    if result.len() > 120 {
        result.truncate(120);
        result = result.trim_matches('-').to_string();
    }

    if result.is_empty() {
        result = "untitled".to_string();
    }

    if !result.ends_with(".md") {
        result.push_str(".md");
    }

    result
}

/// Check if a source has already been synced.
pub async fn is_source_synced(
    pool: &SqlitePool,
    project_id: &str,
    source_type: &str,
    source_id: &str,
) -> Result<bool> {
    let row: Option<String> = sqlx::query_scalar(
        "SELECT id FROM knowledge_sync_log WHERE project_id = ? AND source_type = ? AND source_id = ?",
    )
    .bind(project_id)
    .bind(source_type)
    .bind(source_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.is_some())
}

/// Record a sync log entry.
pub async fn record_sync(
    pool: &SqlitePool,
    project_id: &str,
    source_type: &str,
    source_id: &str,
    file_path: &str,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT OR IGNORE INTO knowledge_sync_log (id, project_id, source_type, source_id, file_path, synced_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(project_id)
    .bind(source_type)
    .bind(source_id)
    .bind(file_path)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Capture knowledge in real-time (from MCP tool).
pub async fn capture_knowledge(
    pool: &SqlitePool,
    project_id: &str,
    title: &str,
    content: &str,
    category: &str,
) -> Result<String> {
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d-%H%M%S").to_string();
    let safe_title = sanitize_filename(title);
    let filename = format!("{}-{}", timestamp, safe_title);

    let full_content = format!("## {} [{}]\n\n{}", title, category, content);

    let file_path = write_knowledge_file(project_id, "live", &filename, &full_content)?;

    record_sync(pool, project_id, "live", &filename, &file_path).await?;

    Ok(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello world"), "hello-world.md");
        assert_eq!(sanitize_filename("test.md"), "test.md");
        assert_eq!(
            sanitize_filename("special!@#chars"),
            "special-chars.md"
        );
    }
}
