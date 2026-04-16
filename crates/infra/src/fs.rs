use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    std::env::var("PH_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir().unwrap().join(".project-hub")
        })
}

pub fn init_dirs() -> anyhow::Result<()> {
    let base = data_dir();

    std::fs::create_dir_all(base.join("workspace"))?;
    std::fs::create_dir_all(base.join("agents"))?;
    std::fs::create_dir_all(base.join("prompts"))?;
    std::fs::create_dir_all(base.join("knowledge"))?;
    std::fs::create_dir_all(base.join("docs"))?;

    init_default_agents()?;

    Ok(())
}

fn init_default_agents() -> anyhow::Result<()> {
    let agents = data_dir().join("agents");

    let defaults: [(&str, &str); 6] = [
        ("default", DEFAULT_AGENT),
        ("analyst", ANALYST_AGENT),
        ("architect", ARCHITECT_AGENT),
        ("planner", PLANNER_AGENT),
        ("coder", CODER_AGENT),
        ("reviewer", REVIEWER_AGENT),
    ];

    for (name, content) in defaults {
        let path = agents.join(format!("{}.toml", name));
        if !path.exists() {
            std::fs::write(path, content)?;
        }
    }

    Ok(())
}

const DEFAULT_AGENT: &str = r#"name = "default"
model = "claude-sonnet-4-6"
system_prompt = "你是一个有帮助的 AI 助手。"
"#;

const ANALYST_AGENT: &str = r#"name = "analyst"
model = "claude-sonnet-4-6"
system_prompt = "你是一个需求分析师。你的任务是帮助用户澄清 todo 的边界、验收标准和潜在风险。不要写代码，只输出结构化的需求文档。"
"#;

const ARCHITECT_AGENT: &str = r#"name = "architect"
model = "claude-sonnet-4-6"
system_prompt = "你是一个系统架构师。基于已确认的需求，设计最小可行的实现方案。列出关键文件、接口和依赖关系。不要写代码。"
"#;

const PLANNER_AGENT: &str = r#"name = "planner"
model = "claude-sonnet-4-6"
system_prompt = "你是一个任务规划师。将设计方案拆分为可执行的、2-5 分钟的步骤。每个步骤必须包含：文件路径、操作说明、验收标准。"
"#;

const CODER_AGENT: &str = r#"name = "coder"
model = "claude-sonnet-4-6"
system_prompt = "你是一个严谨的 Rust 工程师。按步骤执行编码任务，使用 TDD（先写测试再实现），频繁提交（每完成一个步骤就 git commit）。遇到不确定性时暂停并向用户确认。"
"#;

const REVIEWER_AGENT: &str = r#"name = "reviewer"
model = "claude-sonnet-4-6"
system_prompt = "你是一个代码审查员。检查已完成的实现是否满足需求、是否引入回归、是否遵循项目架构。输出结构化的 review 报告。"
"#;

pub fn workspace_dir() -> PathBuf {
    data_dir().join("workspace")
}

pub fn knowledge_dir() -> PathBuf {
    data_dir().join("knowledge")
}

pub fn docs_dir() -> PathBuf {
    data_dir().join("docs")
}

pub fn todo_docs_dir(todo_id: &str) -> PathBuf {
    docs_dir().join(todo_id)
}

pub fn create_project_symlink(id: &str, real_path: &str) -> anyhow::Result<()> {
    let link = workspace_dir().join(id);

    if link.exists() {
        std::fs::remove_file(&link)?;
    }

    #[cfg(target_family = "unix")]
    std::os::unix::fs::symlink(real_path, &link)?;

    #[cfg(target_family = "windows")]
    std::os::windows::fs::symlink_dir(real_path, &link)?;

    Ok(())
}

pub fn create_knowledge_dir(id: &str) -> anyhow::Result<()> {
    let dir = knowledge_dir().join(id);
    std::fs::create_dir_all(dir)?;
    Ok(())
}

pub fn create_todo_docs_dir(todo_id: &str) -> anyhow::Result<()> {
    let dir = todo_docs_dir(todo_id);
    std::fs::create_dir_all(dir)?;
    Ok(())
}
