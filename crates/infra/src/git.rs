use std::path::PathBuf;
use std::process::Command;

pub struct GitCommitInfo {
    pub hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
    pub body: String,
}

pub fn get_recent_commits(
    project_path: &str,
    since: Option<&str>,
    max_count: u32,
) -> anyhow::Result<Vec<GitCommitInfo>> {
    let mut cmd = Command::new("git");
    cmd.current_dir(project_path)
        .arg("log")
        .arg("--format=%H%x00%s%x00%an%x00%aI%x00%b%x00")
        .arg("--no-merges")
        .arg(format!("-{}", max_count));

    if let Some(since_date) = since {
        cmd.arg(format!("--since={}", since_date));
    }

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git log failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();

    for entry in stdout.split('\0').collect::<Vec<&str>>().chunks(5) {
        if entry.len() >= 4 {
            let hash = entry[0].trim().to_string();
            let subject = entry[1].trim().to_string();
            let author = entry[2].trim().to_string();
            let date = entry[3].trim().to_string();
            let body = if entry.len() > 4 {
                entry[4].trim().to_string()
            } else {
                String::new()
            };
            if !hash.is_empty() {
                commits.push(GitCommitInfo {
                    hash,
                    subject,
                    author,
                    date,
                    body,
                });
            }
        }
    }

    Ok(commits)
}

pub fn get_commit_diff(
    project_path: &str,
    commit_hash: &str,
) -> anyhow::Result<(String, String)> {
    let stat_output = Command::new("git")
        .current_dir(project_path)
        .args(["diff", "--stat", &format!("{}^..{}", commit_hash, commit_hash)])
        .output()?;

    let stat = String::from_utf8_lossy(&stat_output.stdout).to_string();

    let diff_output = Command::new("git")
        .current_dir(project_path)
        .args(["diff", &format!("{}^..{}", commit_hash, commit_hash)])
        .output()?;

    let diff = String::from_utf8_lossy(&diff_output.stdout).to_string();

    Ok((stat, diff))
}

pub fn ensure_worktree_ignored(project_path: &str) -> anyhow::Result<()> {
    let output = Command::new("git")
        .current_dir(project_path)
        .args(["check-ignore", "-q", ".worktrees"])
        .output()?;

    if !output.status.success() {
        let gitignore = std::path::Path::new(project_path).join(".gitignore");
        let line = ".worktrees/\n";
        if gitignore.exists() {
            let content = std::fs::read_to_string(&gitignore)?;
            if !content.contains(".worktrees") {
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&gitignore)?;
                use std::io::Write;
                file.write_all(line.as_bytes())?;
            }
        } else {
            std::fs::write(&gitignore, line)?;
        }
    }
    Ok(())
}

pub fn create_todo_worktree(project_path: &str, todo_id: &str) -> anyhow::Result<PathBuf> {
    ensure_worktree_ignored(project_path)?;

    let worktree_path = PathBuf::from(project_path).join(".worktrees").join(todo_id);
    if worktree_path.exists() {
        return Ok(worktree_path);
    }

    let status = Command::new("git")
        .current_dir(project_path)
        .args([
            "worktree",
            "add",
            worktree_path.to_string_lossy().as_ref(),
            "-b",
            &format!("ph/{}", todo_id),
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("git worktree add failed");
    }

    Ok(worktree_path)
}

pub fn remove_todo_worktree(project_path: &str, todo_id: &str) -> anyhow::Result<()> {
    let worktree_path = PathBuf::from(project_path).join(".worktrees").join(todo_id);
    if !worktree_path.exists() {
        return Ok(());
    }

    let status = Command::new("git")
        .current_dir(project_path)
        .args([
            "worktree",
            "remove",
            worktree_path.to_string_lossy().as_ref(),
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("git worktree remove failed");
    }

    Ok(())
}

pub fn run_git_command(project_path: &str, args: &[&str]) -> anyhow::Result<std::process::Output> {
    if args.iter().any(|a| *a == "push") {
        anyhow::bail!("git push is not allowed in ph work workflow");
    }

    let output = Command::new("git")
        .current_dir(project_path)
        .args(args)
        .output()?;

    Ok(output)
}
