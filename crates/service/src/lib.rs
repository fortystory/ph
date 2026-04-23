pub mod stage_prompt;
pub use stage_prompt::stage_instruction;

use anyhow::Result;
use core::Project;
use infra::{load_projects, save_projects};
use infra::{create_project_symlink, create_knowledge_dir, todo_docs_dir};
use sqlx::SqlitePool;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WorkItem {
    pub projects: Vec<String>,
    pub title: String,
    pub priority: i32,
    pub short_id: String,
    pub ids: Vec<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct TimeReport {
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub stage_durations: Vec<(String, i64)>, // (stage, seconds)
    pub total_seconds: i64,
}

pub fn add_project(id: &str, name: &str, path: &str) -> Result<()> {
    let mut data = load_projects()?;

    if data.projects.iter().any(|p| p.id == id) {
        anyhow::bail!("project already exists");
    }

    let project = Project {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string(),
        agent: None,
        prompt: None,
        knowledge: Some(id.to_string()),
    };

    // 先创建资源
    create_project_symlink(id, path)?;
    create_knowledge_dir(id)?;

    data.projects.push(project);
    save_projects(&data)?;

    Ok(())
}

pub fn list_projects() -> Result<Vec<Project>> {
    Ok(load_projects()?.projects)
}

pub async fn add_todo(pool: &SqlitePool, project: &str, title: &str) -> Result<()> {
    infra::add_todo(pool, project, title).await
}

pub async fn add_work_item(
    pool: &SqlitePool,
    title: &str,
    status: &str,
    priority: i32,
    projects: &[String],
) -> Result<String> {
    let created_at = chrono::Utc::now().to_rfc3339();
    for proj in projects {
        let id = uuid::Uuid::new_v4().to_string();
        infra::insert_todo(pool, &id, proj, title, status, priority, &created_at).await?;
    }
    Ok(created_at)
}

pub async fn edit_work_item(
    pool: &SqlitePool,
    original: &WorkItem,
    title: &str,
    status: &str,
    priority: i32,
    projects: &[String],
) -> Result<()> {
    infra::update_todos_by_ids(pool, &original.ids, title, status, priority).await?;
    for proj in projects {
        if !original.projects.contains(proj) {
            let id = uuid::Uuid::new_v4().to_string();
            infra::insert_todo(pool, &id, proj, title, status, priority, &original.created_at).await?;
        }
    }
    for proj in &original.projects {
        if !projects.contains(proj) {
            infra::delete_todos_by_group(
                pool,
                &original.title,
                original.priority,
                &original.created_at,
                proj,
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn delete_todos_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<()> {
    infra::delete_todos_by_ids(pool, ids).await
}

pub async fn list_todos(pool: &SqlitePool, project: Option<&str>) -> Result<Vec<core::Todo>> {
    infra::list_todos(pool, project).await
}

pub async fn list_pending_todos(pool: &SqlitePool) -> Result<Vec<core::Todo>> {
    infra::list_pending_todos(pool).await
}

pub async fn list_work_items(pool: &SqlitePool, project: Option<&str>) -> Result<Vec<WorkItem>> {
    let todos = infra::list_todos(pool, project).await?;
    Ok(group_todos_into_work_items(todos))
}

pub async fn list_pending_work_items(pool: &SqlitePool) -> Result<Vec<WorkItem>> {
    let todos = infra::list_pending_todos(pool).await?;
    Ok(group_todos_into_work_items(todos))
}

fn group_todos_into_work_items(todos: Vec<core::Todo>) -> Vec<WorkItem> {
    let mut groups: HashMap<(String, i32, String), Vec<core::Todo>> = HashMap::new();
    for t in todos {
        let key = (t.title.clone(), t.priority, t.created_at.clone());
        groups.entry(key).or_default().push(t);
    }

    let mut items: Vec<WorkItem> = groups
        .into_values()
        .map(|group| {
            let mut projects: Vec<String> = group.iter().map(|t| t.project_id.clone()).collect();
            projects.sort();
            projects.dedup();

            let ids: Vec<String> = group.iter().map(|t| t.id.clone()).collect();
            let short_id = compute_short_id(&ids);

            WorkItem {
                projects,
                title: group[0].title.clone(),
                priority: group[0].priority,
                short_id,
                ids,
                status: group[0].status.clone(),
                created_at: group[0].created_at.clone(),
            }
        })
        .collect();

    items.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.title.cmp(&b.title))
    });

    items
}

fn compute_short_id(ids: &[String]) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let first = extract_base_id(&ids[0]);
    if ids.iter().all(|id| extract_base_id(id) == first) {
        first
    } else {
        ids[0].chars().take(8).collect()
    }
}

fn extract_base_id(id: &str) -> String {
    if let Some(pos) = id.rfind('-') {
        let prefix = &id[..pos];
        if prefix.starts_with("TODO-") {
            return prefix.to_string();
        }
    }
    id.chars().take(8).collect()
}

pub async fn start_stage(
    pool: &SqlitePool,
    work_item: &WorkItem,
    stage: &str,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    infra::insert_stage_log(
        pool,
        &id,
        &work_item.title,
        work_item.priority,
        &work_item.created_at,
        stage,
        &now,
    )
    .await?;
    Ok(id)
}

pub async fn end_stage(pool: &SqlitePool, log_id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    infra::end_stage_log(pool, log_id, &now).await
}

pub async fn load_time_report(pool: &SqlitePool, work_item: &WorkItem) -> Result<TimeReport> {
    let logs = infra::list_stage_logs(
        pool,
        &work_item.title,
        work_item.priority,
        &work_item.created_at,
    )
    .await?;
    let mut total = 0i64;
    let mut stage_durations: Vec<(String, i64)> = Vec::new();
    for log in &logs {
        if let Some(ref ended) = log.ended_at {
            let start = chrono::DateTime::parse_from_rfc3339(&log.started_at)?;
            let end = chrono::DateTime::parse_from_rfc3339(ended)?;
            let secs = (end - start).num_seconds().max(0);
            total += secs;
            stage_durations.push((log.stage.clone(), secs));
        }
    }
    let todos = infra::list_todos(pool, None).await?;
    let completed_at = todos
        .iter()
        .find(|t| {
            t.title == work_item.title
                && t.priority == work_item.priority
                && t.created_at == work_item.created_at
        })
        .and_then(|t| t.completed_at.clone());
    let started_at = if logs.is_empty() {
        None
    } else {
        Some(logs[0].started_at.clone())
    };
    Ok(TimeReport {
        started_at,
        completed_at,
        stage_durations,
        total_seconds: total,
    })
}

pub async fn done_todo_with_time(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    infra::mark_done_with_time(pool, id, &now).await
}

pub async fn remove_todo(pool: &SqlitePool, id: &str) -> Result<()> {
    infra::delete_todo(pool, id).await
}

pub fn create_todo_docs_dir(todo_id: &str) -> Result<()> {
    infra::create_todo_docs_dir(todo_id)
}

pub fn list_todo_docs(todo_id: &str) -> Result<Vec<String>> {
    let dir = infra::todo_docs_dir(todo_id);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut files = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            files.push(path.to_string_lossy().to_string());
        }
    }
    Ok(files)
}

pub fn load_todo_docs(todo_id: &str) -> Result<String> {
    let dir = infra::todo_docs_dir(todo_id);
    let mut result = String::new();

    if !dir.exists() {
        return Ok(result);
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            let content = std::fs::read_to_string(&path)?;
            result.push_str(&format!(
                "\n\n# File: {}\n{}",
                path.file_name().unwrap().to_string_lossy(),
                content
            ));
        }
    }

    Ok(result)
}

pub fn resolve_agent_for_stage(stage: &str) -> String {
    match stage {
        "requirements" => "analyst",
        "design" => "architect",
        "tasks" => "planner",
        "progress" => "coder",
        "review" => "reviewer",
        _ => "default",
    }
    .to_string()
}

pub fn detect_stage(todo_id: &str) -> String {
    let docs = todo_docs_dir(todo_id);
    if !docs.join("01-requirements.md").exists() {
        return "requirements".to_string();
    }
    if !docs.join("02-design.md").exists() {
        return "design".to_string();
    }
    if !docs.join("03-tasks.md").exists() {
        return "tasks".to_string();
    }
    if !docs.join("04-progress.md").exists() {
        return "progress".to_string();
    }
    if !docs.join("05-review.md").exists() {
        return "review".to_string();
    }
    "done".to_string()
}

pub fn load_stage_doc(todo_id: &str, stage: &str) -> String {
    let path = todo_docs_dir(todo_id).join(format!("{}-{}.md", stage_number(stage), stage));
    if path.exists() {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    }
}

pub fn save_stage_doc(todo_id: &str, stage: &str, content: &str) -> Result<()> {
    let dir = todo_docs_dir(todo_id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}-{}.md", stage_number(stage), stage));
    std::fs::write(path, content)?;
    Ok(())
}

fn stage_number(stage: &str) -> &str {
    match stage {
        "requirements" => "01",
        "design" => "02",
        "tasks" => "03",
        "progress" => "04",
        "review" => "05",
        _ => "00",
    }
}

pub async fn run_agent(
    pool: &SqlitePool,
    project_id: &str,
    user_input: Option<&str>,
) -> anyhow::Result<String> {
    let projects = load_projects()?;
    let project = projects
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| anyhow::anyhow!("project not found"))?;

    let agent_name = project.agent.as_deref().unwrap_or("coder");
    let agent = infra::load_agent(agent_name)?;

    let todos = infra::load_todo_context(pool, project_id).await?;
    let knowledge = infra::load_knowledge(project_id)?;

    let prompt = infra::build_prompt(project, &[], &agent, &todos, &knowledge, "", user_input);

    let result = infra::call_claude(&prompt, &agent.model).await?;

    Ok(result)
}

pub async fn load_knowledge_rag(
    pool: &SqlitePool,
    project_id: &str,
    todo_title: &str,
    stage: &str,
    top_k: i64,
) -> Result<String> {
    eprintln!("[rag] load_knowledge_rag start project_id={}", project_id);

    // Initialize embedder (downloads model on first use)
    eprintln!("[rag] creating embedder...");
    let embedder = infra::Embedder::new()?;
    eprintln!("[rag] embedder ready");

    // Check and update index if stale
    eprintln!("[rag] checking is_stale...");
    if infra::is_stale(pool, project_id).await? {
        eprintln!("[rag] index is stale, updating...");
        infra::update_index(pool, project_id, &embedder).await?;
        eprintln!("[rag] index update done");
    } else {
        eprintln!("[rag] index is fresh");
    }

    // Build query from todo title + stage context
    let query = format!("{} {}", todo_title, stage_display_name_cn(stage));
    eprintln!("[rag] embedding query: {}", query);
    let query_vec = embedder.embed_query(&query)?;
    eprintln!("[rag] query embedded, dim={}", query_vec.len());

    // Semantic search
    eprintln!("[rag] searching qdrant...");
    let chunks = infra::search(pool, project_id, query_vec, top_k).await?;
    eprintln!("[rag] search done, chunks={}", chunks.len());

    // Format for prompt
    let mut result = String::new();
    for chunk in &chunks {
        result.push_str(&format!(
            "\n\n# 文件：{}\n{}",
            chunk.file_path, chunk.content
        ));
    }

    eprintln!("[rag] load_knowledge_rag done");
    Ok(result)
}

fn stage_display_name_cn(stage: &str) -> &str {
    match stage {
        "requirements" => "需求澄清",
        "design" => "方案设计",
        "tasks" => "任务拆解",
        "progress" => "编码实现",
        "review" => "验收回顾",
        _ => stage,
    }
}

pub async fn run_pre_clarify(
    pool: &SqlitePool,
    work_item: &WorkItem,
    primary_project: &core::Project,
    related_projects: &[core::Project],
    todo_doc_id: &str,
    agent: &core::Agent,
) -> Result<()> {
    let mut knowledge = String::new();
    for proj in std::iter::once(primary_project).chain(related_projects.iter()) {
        let proj_knowledge = load_knowledge_rag(
            pool,
            &proj.id,
            &work_item.title,
            "requirements",
            5,
        )
        .await?;
        if !proj_knowledge.is_empty() {
            if !knowledge.is_empty() {
                knowledge.push('\n');
            }
            knowledge.push_str(&format!("\n## 项目 {}\n{}", proj.id, proj_knowledge),
            );
        }
    }

    let pre_clarify_prompt = build_pre_clarify_prompt(
        primary_project,
        related_projects,
        agent,
        work_item,
        &knowledge,
        todo_doc_id,
    );

    let prompt_path = infra::data_dir().join("prompts").join("pre-clarify.md");
    std::fs::create_dir_all(prompt_path.parent().unwrap())?;
    std::fs::write(&prompt_path, &pre_clarify_prompt)?;

    let work_dir = primary_project.path.clone();

    println!(
        "\n┌─[预澄清 / agent: {}]─{}─┐",
        agent.name,
        "─".repeat(20)
    );
    println!("│  launching claude in {}", work_dir);
    println!("│  task:  {}", work_item.title);
    println!("└{}┘", "─".repeat(48));

    let status = std::process::Command::new("claude")
        .current_dir(&work_dir)
        .arg(&pre_clarify_prompt)
        .status()?;

    if !status.success() {
        anyhow::bail!("Claude Code exited with non-zero status");
    }

    Ok(())
}

fn build_pre_clarify_prompt(
    primary_project: &core::Project,
    related_projects: &[core::Project],
    agent: &core::Agent,
    work_item: &WorkItem,
    knowledge: &str,
    todo_doc_id: &str,
) -> String {
    let mut project_info = format!(
        "## 主项目\n名称：{}\n路径：{}",
        primary_project.name, primary_project.path
    );
    if let Some(ref prompt) = primary_project.prompt {
        project_info.push_str(&format!("\n\n{}\n", prompt.trim()));
    }
    if !related_projects.is_empty() {
        project_info.push_str("\n\n## 关联项目\n");
        for (i, p) in related_projects.iter().enumerate() {
            project_info.push_str(&format!("{}. {}：{}\n", i + 1, p.name, p.path));
            if let Some(ref prompt) = p.prompt {
                project_info.push_str(&format!("   {}\n", prompt.trim()));
            }
        }
    }

    let output_dir = infra::todo_docs_dir(todo_doc_id);

    let knowledge_section = if knowledge.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n> 以下知识片段与「{}」相关，可为需求澄清提供背景。\n{}",
            work_item.title, knowledge
        )
    };

    format!(
        r#"{system}

# 项目
{project_info}

# 待办事项
- {title}
  优先级: {priority}
  关联项目: {projects}
  状态: {status}

# 项目知识{knowledge_section}

# 任务
这是一个**需求预澄清 session**。用户刚刚添加了一个待办事项，但需求细节尚未明确。

你的任务是：
1. 与用户进行对话，深入了解这个需求的背景和动机
2. 澄清功能范围（包含什么、不包含什么）
3. 了解验收标准和成功指标
4. 识别潜在风险、依赖和边界条件
5. 询问任何你认为需要澄清的问题

对话风格：
- 以提问开始，引导用户展开描述
- 每次只问 1-2 个问题，避免信息过载
- 根据用户的回答追问细节
- 如果用户提到技术方案，提醒他们本阶段只关注"做什么"而非"怎么做"

当用户表示"差不多了"或"可以结束了"时：
1. 总结你理解的需求
2. 将澄清结果写入 {output_dir}/01-requirements.md
3. 文档应包含：背景、目标、功能范围、非功能需求、验收标准
4. 完成后退出 Claude Code

如果用户一开始就说"不需要讨论"或"直接开始"，则：
1. 基于 todo 标题和你的理解，写一份简要的需求文档
2. 写入 {output_dir}/01-requirements.md
3. 退出 Claude Code
"#,
        system = agent.system_prompt,
        project_info = project_info,
        title = work_item.title,
        priority = work_item.priority,
        projects = work_item.projects.join(", "),
        status = work_item.status,
        knowledge_section = knowledge_section,
        output_dir = output_dir.display(),
    )
}
