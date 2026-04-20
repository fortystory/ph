use anyhow::Result;
use serde_json::json;

fn load_claude_api_key() -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .or_else(|| std::env::var("ANTHROPIC_AUTH_TOKEN").ok())
        .or_else(|| {
            let settings_path = dirs::home_dir()?.join(".claude/settings.json");
            let content = std::fs::read_to_string(settings_path).ok()?;
            let settings: serde_json::Value = serde_json::from_str(&content).ok()?;
            settings
                .get("env")?
                .get("ANTHROPIC_AUTH_TOKEN")?
                .as_str()
                .map(|s| s.to_string())
        })
}

fn load_claude_base_url() -> String {
    std::env::var("ANTHROPIC_BASE_URL")
        .ok()
        .or_else(|| {
            let settings_path = dirs::home_dir()?.join(".claude/settings.json");
            let content = std::fs::read_to_string(settings_path).ok()?;
            let settings: serde_json::Value = serde_json::from_str(&content).ok()?;
            settings
                .get("env")?
                .get("ANTHROPIC_BASE_URL")?
                .as_str()
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "https://api.anthropic.com".to_string())
}

pub fn build_prompt(
    projects: &[core::Project],
    agent: &core::Agent,
    todos: &str,
    knowledge: &str,
    todo_docs: &str,
    user_input: Option<&str>,
) -> String {
    let project_info = if projects.len() == 1 {
        format!(
            "名称：{}\n路径：{}",
            projects[0].name, projects[0].path
        )
    } else {
        projects
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{}. {}：{}", i + 1, p.name, p.path))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let task = user_input.unwrap_or("请分析当前项目并给出建议。");
    let agent_teams_hint = if projects.len() > 1 {
        "\n\n> 提示：此任务涉及多个项目。如果你已启用 Claude Code agent teams（设置环境变量 CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1），\
可以在此会话中运行 `Create an agent team with one teammate per project to work on each part in parallel.` 来并行处理各项目。"
    } else {
        ""
    };

    format!(
r#"{system}

# 项目
{project_info}

# 待办事项
{todos}

# 项目知识
{knowledge}
{docs}
# 任务
{task}{agent_teams_hint}
"#,
        system = agent.system_prompt,
        project_info = project_info,
        todos = todos,
        knowledge = knowledge,
        docs = if todo_docs.is_empty() {
            String::new()
        } else {
            format!("\n# 工作流文档\n{}\n", todo_docs)
        },
        task = task,
        agent_teams_hint = agent_teams_hint,
    )
}

pub async fn call_claude(prompt: &str, model: &str) -> Result<String> {
    let api_key = load_claude_api_key()
        .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN not found"))?;

    let base_url = load_claude_base_url();
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));

    let client = reqwest::Client::new();

    let resp = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": model,
            "max_tokens": 1000,
            "messages": [
                { "role": "user", "content": prompt }
            ]
        }))
        .send()
        .await?;

    let json: serde_json::Value = resp.json().await?;

    let text = json["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(text)
}
