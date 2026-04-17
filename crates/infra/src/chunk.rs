use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Split markdown content into chunks by `##` headings.
/// Returns (chunk_id, chunk_content) pairs.
pub fn chunk_markdown(content: &str, file_path: &str) -> Vec<(String, String)> {
    let mut chunks: Vec<(String, String)> = Vec::new();
    let mut current_heading: Option<&str> = None;
    let mut current_body: Vec<&str> = Vec::new();
    let mut chunk_index: i32 = 0;

    for line in content.lines() {
        if line.starts_with("## ") {
            // Flush previous chunk
            if let Some(heading) = current_heading {
                let chunk_id = format!("{}#{}", file_path, chunk_index);
                let chunk_content = format!("{}\n{}", heading, current_body.join("\n"));
                chunks.push((chunk_id, chunk_content.trim().to_string()));
                chunk_index += 1;
                current_body.clear();
            } else if !current_body.is_empty() {
                // Content before first heading
                let chunk_id = format!("{}#{}", file_path, chunk_index);
                let chunk_content = current_body.join("\n");
                chunks.push((chunk_id, chunk_content.trim().to_string()));
                chunk_index += 1;
                current_body.clear();
            }
            current_heading = Some(line);
        } else {
            current_body.push(line);
        }
    }

    // Flush remaining content
    let remaining = current_body.join("\n");
    if let Some(heading) = current_heading {
        let chunk_id = format!("{}#{}", file_path, chunk_index);
        let chunk_content = format!("{}\n{}", heading, remaining);
        chunks.push((chunk_id, chunk_content.trim().to_string()));
    } else if !remaining.is_empty() {
        let chunk_id = format!("{}#{}_0", file_path, chunk_index);
        chunks.push((chunk_id, remaining.trim().to_string()));
    }

    // If no chunks were created from headings, treat the entire file as one chunk
    if chunks.is_empty() && !content.trim().is_empty() {
        chunks.push((
            format!("{}#0", file_path),
            content.trim().to_string(),
        ));
    }

    chunks
}

/// Compute a simple hash of content for change detection.
pub fn content_hash(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
