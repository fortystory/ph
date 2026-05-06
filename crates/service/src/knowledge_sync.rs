use anyhow::Result;
use sqlx::SqlitePool;

pub struct SyncReport {
    pub commits_processed: usize,
    pub files_written: usize,
    pub skipped_duplicates: usize,
}

/// Sync git commit knowledge for a project.
/// Reads recent commits, generates markdown knowledge files, and stores them.
pub async fn sync_git_knowledge(
    pool: &SqlitePool,
    project_id: &str,
    project_path: &str,
) -> Result<SyncReport> {
    let commits = infra::get_recent_commits(project_path, None, 50)?;

    let mut report = SyncReport {
        commits_processed: 0,
        files_written: 0,
        skipped_duplicates: 0,
    };

    for commit in &commits {
        report.commits_processed += 1;

        // Skip if already synced
        if infra::is_source_synced(pool, project_id, "commit", &commit.hash).await? {
            report.skipped_duplicates += 1;
            continue;
        }

        // Skip if only lock/generated files changed
        let (stat, diff) = infra::get_commit_diff(project_path, &commit.hash)?;
        if is_trivial_commit(&stat) {
            continue;
        }

        let short_hash: String = commit.hash.chars().take(8).collect();
        let content = format_commit_knowledge(&short_hash, commit, &stat, &diff);

        let filename = format!("{}-{}.md", &commit.date[..10], short_hash);
        let file_path = infra::write_knowledge_file(project_id, "commits", &filename, &content)?;

        infra::record_sync(pool, project_id, "commit", &commit.hash, &file_path).await?;
        report.files_written += 1;
    }

    Ok(report)
}

/// Sync conversation knowledge from Claude Code sessions.
pub async fn sync_conversation_knowledge(
    pool: &SqlitePool,
    project_id: &str,
    project_path: &str,
) -> Result<SyncReport> {
    let sessions = find_sessions_for_project(project_path)?;

    let mut report = SyncReport {
        commits_processed: 0,
        files_written: 0,
        skipped_duplicates: 0,
    };

    for (session_id, session_path) in &sessions {
        report.commits_processed += 1;

        // Skip if already synced
        if infra::is_source_synced(pool, project_id, "session", session_id).await? {
            report.skipped_duplicates += 1;
            continue;
        }

        let messages = infra::parse_session(session_path)?;

        // Skip short conversations
        let user_count = messages.iter().filter(|(r, _)| r == "user").count();
        if user_count < 3 {
            continue;
        }

        let content = format_conversation_knowledge(&messages);
        let date = extract_date_from_session(session_id);
        let filename = format!("{}-{}.md", date, &session_id[..8.min(session_id.len())]);
        let file_path =
            infra::write_knowledge_file(project_id, "conversations", &filename, &content)?;

        infra::record_sync(pool, project_id, "session", session_id, &file_path).await?;
        report.files_written += 1;
    }

    Ok(report)
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
    let filename = format!("{}-{}.md", timestamp, sanitize_title(title));

    let full_content = format!(
        "## {} [{}]\n\n{}",
        title, category, content
    );

    let file_path = infra::write_knowledge_file(project_id, "live", &filename, &full_content)?;

    infra::record_sync(pool, project_id, "live", &filename, &file_path).await?;

    Ok(file_path)
}

fn format_commit_knowledge(
    short_hash: &str,
    commit: &infra::GitCommitInfo,
    stat: &str,
    diff: &str,
) -> String {
    // Condense large diffs
    let diff_section = if diff.lines().count() > 100 {
        format!("(diff too large, {} lines omitted)\n\n{}", diff.lines().count(), summarize_diff(diff))
    } else {
        diff.to_string()
    };

    format!(
        r#"## Commit: {} - {}

Date: {}
Author: {}

### Summary
{}

### Changed Files
{}

### Key Changes
{}
"#,
        short_hash, commit.subject, commit.date, commit.author, commit.subject, stat, diff_section
    )
}

fn format_conversation_knowledge(messages: &[(String, String)]) -> String {
    let mut content = String::from("## 对话摘要\n\n");

    for (role, text) in messages {
        let label = if role == "user" { "用户" } else { "助手" };
        // Truncate very long messages
        let truncated = if text.len() > 500 {
            format!("{}...", &text[..500])
        } else {
            text.clone()
        };
        content.push_str(&format!("### {}\n{}\n\n", label, truncated));
    }

    content
}

fn summarize_diff(diff: &str) -> String {
    // Extract only added/removed lines as summary
    let mut summary = String::new();
    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            summary.push_str(line);
            summary.push('\n');
        } else if line.starts_with('-') && !line.starts_with("---") {
            summary.push_str(line);
            summary.push('\n');
        }
    }
    if summary.len() > 2000 {
        let truncate_at = floor_char_boundary(&summary, 2000);
        summary.truncate(truncate_at);
        summary.push_str("\n...");
    }
    summary
}

fn floor_char_boundary(s: &str, max: usize) -> usize {
    if s.is_char_boundary(max) {
        max
    } else {
        (0..max).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0)
    }
}

fn is_trivial_commit(stat: &str) -> bool {
    let trivial_patterns = [
        "Cargo.lock",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "node_modules",
        ".min.js",
        ".min.css",
        "dist/",
        "build/",
        ".generated.",
    ];

    let lines: Vec<&str> = stat.lines().collect();
    if lines.is_empty() {
        return true;
    }

    // Check if ALL changed files are trivial
    lines.iter().all(|line| {
        trivial_patterns.iter().any(|p| line.contains(p))
    })
}

fn find_sessions_for_project(project_path: &str) -> Result<Vec<(String, std::path::PathBuf)>> {
    let claude_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("home dir not found"))?
        .join(".claude")
        .join("projects");

    if !claude_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    // The project path is slugified into a directory name
    // e.g., /home/forty/code/dhb/finance-manage-api → -home-forty-code-dhb-finance-manage-api
    for entry in std::fs::read_dir(&claude_dir)? {
        let entry = entry?;
        let dir_name = entry.file_name().to_string_lossy().to_string();

        // Check if this directory corresponds to the project path
        let slug = project_path.replace('/', "-").replace('_', "-");
        if !dir_name.contains(&slug) && !slug.contains(&dir_name) {
            // Also try matching by the last component of the path
            let project_name = std::path::Path::new(project_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !dir_name.contains(&project_name) {
                continue;
            }
        }

        let session_dir = entry.path();
        if !session_dir.is_dir() {
            continue;
        }

        // Find JSONL files
        for session_entry in std::fs::read_dir(&session_dir)? {
            let session_entry = session_entry?;
            let path = session_entry.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                let session_id = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                sessions.push((session_id, path));
            }
        }
    }

    Ok(sessions)
}

fn extract_date_from_session(session_id: &str) -> String {
    // Session IDs often contain timestamps or UUIDs
    // Try to extract a date, fallback to today
    if session_id.len() >= 10 {
        // Check if first chars look like a date
        let prefix: String = session_id.chars().take(10).collect();
        if prefix.matches('-').count() == 2 {
            return prefix;
        }
    }
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn sanitize_title(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
