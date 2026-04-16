use std::path::PathBuf;
use std::process::Command;

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
