use clap::{Parser, Subcommand};

mod picker;

#[derive(Parser)]
#[command(
    name = "ph",
    author,
    version,
    about = "A local-first AI project management hub",
    long_about = "Project Hub (ph) is a CLI tool for managing local projects, todos, and running AI agents with project context."
)]
pub struct Cli {
    #[arg(long, global = true, help = "Skip confirmation prompts")]
    pub yes: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize the project hub data directory
    Init,

    /// Manage projects
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },

    /// Manage todos
    Todo {
        #[command(subcommand)]
        cmd: TodoCmd,
    },

    /// Run an AI agent for a project
    Run {
        /// Project ID
        project: String,

        /// Optional task or question for the agent
        input: Option<String>,
    },

    /// Start an interactive work session: select a todo and launch Claude Code
    Work,
}

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// Add a new project
    Add {
        /// Unique project ID
        id: String,

        /// Human-readable project name
        name: String,

        /// Absolute or relative path to the project
        path: String,
    },

    /// List all registered projects
    List,
}

#[derive(Subcommand)]
pub enum TodoCmd {
    /// Add a new todo to a project
    Add {
        /// Project ID
        #[arg(long)]
        project: String,

        /// Todo title
        title: String,
    },

    /// List todos, optionally filtered by project
    List {
        /// Filter by project ID
        #[arg(long)]
        project: Option<String>,
    },

    /// Mark a todo as done
    Done {
        /// Todo ID
        id: String,
    },

    /// Remove a todo
    Remove {
        /// Todo ID
        id: String,
    },

    /// Show or create the docs directory for a todo
    Doc {
        /// Todo ID
        id: String,
    },
}

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            infra::init_dirs()?;
            infra::init_db().await?;
            println!("initialized");
        }

        Commands::Project { cmd } => match cmd {
            ProjectCmd::Add { id, name, path } => {
                service::add_project(&id, &name, &path)?;
                println!("project added");
            }
            ProjectCmd::List => {
                let list = service::list_projects()?;
                for p in list {
                    println!("{} -> {}", p.id, p.path);
                }
            }
        },

        Commands::Todo { cmd } => {
            let pool = infra::init_db().await?;

            match cmd {
                TodoCmd::Add { project, title } => {
                    service::add_todo(&pool, &project, &title).await?;
                    println!("todo added");
                }
                TodoCmd::List { project } => {
                    let list = service::list_work_items(&pool, project.as_deref()).await?;
                    for item in list {
                        let projects = item.projects.join(", ");
                        println!(
                            "[{}] {} ({}) [{}]",
                            item.status, item.title, item.short_id, projects
                        );
                    }
                }
                TodoCmd::Done { id } => {
                    service::done_todo_with_time(&pool, &id).await?;
                    println!("done");
                }
                TodoCmd::Remove { id } => {
                    service::remove_todo(&pool, &id).await?;
                    println!("removed");
                }
                TodoCmd::Doc { id } => {
                    service::create_todo_docs_dir(&id)?;
                    let docs = service::list_todo_docs(&id)?;
                    let dir = infra::todo_docs_dir(&id);
                    println!("{}", dir.display());
                    if docs.is_empty() {
                        println!("(no docs yet)");
                    } else {
                        for d in docs {
                            println!("  - {}", d);
                        }
                    }
                }
            }
        }

        Commands::Run { project, input } => {
            let pool = infra::init_db().await?;

            let result = service::run_agent(
                &pool,
                &project,
                input.as_deref(),
            )
            .await?;

            println!("{}", result);
        }

        Commands::Work => {
            let pool = infra::init_db().await?;
            let all_projects = service::list_projects()?;
            let project_names: Vec<String> = all_projects.iter().map(|p| p.id.clone()).collect();

            loop {
                let work_items = service::list_work_items(&pool, None).await?;
                let mut items: Vec<picker::Item> = Vec::new();
                for (idx, item) in work_items.iter().enumerate() {
                    let stage = service::detect_stage(
                        item.ids.first().cloned().as_deref().unwrap_or(&item.short_id)
                    );
                    let stage_cn = stage_display_name(&stage);
                    let mut detail_lines = vec![
                        format!("阶段: {}", stage_cn),
                        format!("ID 列表: {}", item.ids.join(", ")),
                    ];
                    if let Ok(time_report) = service::load_time_report(&pool, item).await {
                        detail_lines.push(format!("添加时间: {}", item.created_at));
                        if let Some(ref started) = time_report.started_at {
                            detail_lines.push(format!("开始时间: {}", started));
                        }
                        if let Some(ref completed) = time_report.completed_at {
                            detail_lines.push(format!("完成时间: {}", completed));
                        }
                        for (stage, secs) in &time_report.stage_durations {
                            detail_lines.push(format!("{}: {}m", stage_display_name(stage), secs / 60));
                        }
                        if time_report.total_seconds > 0 {
                            detail_lines.push(format!("总耗时: {}m", time_report.total_seconds / 60));
                        }
                    }
                    items.push(picker::Item {
                        title: item.title.clone(),
                        priority: item.priority,
                        short_id: item.short_id.clone(),
                        projects: item.projects.join(" "),
                        detail_lines,
                        index: idx,
                        status: item.status.clone(),
                    });
                }

                let action = picker::pick(items, project_names.clone())?;
                match action {
                    picker::Action::Quit => break,
                    picker::Action::Select(idx) => {
                        let selected = work_items.into_iter().nth(idx).unwrap();
                        let linked_projects: Vec<core::Project> = selected.projects.iter()
                            .filter_map(|pid| all_projects.iter().find(|p| p.id == *pid))
                            .cloned()
                            .collect();
                        if linked_projects.is_empty() {
                            anyhow::bail!("no project linked to this todo");
                        }
                        let primary_project = &linked_projects[0];
                        let todo_doc_id = selected.ids.first()
                            .cloned()
                            .unwrap_or_else(|| selected.short_id.clone());
                        let mut current_stage = service::detect_stage(&todo_doc_id);
                        if current_stage == "done" {
                            println!("All workflow stages are already completed for this todo.");
                            let restart = if cli.yes {
                                false
                            } else {
                                tokio::task::spawn_blocking(move || {
                                    picker::confirm("Restart from review stage?")
                                }).await??
                            };
                            if restart {
                                current_stage = "review".to_string();
                            } else {
                                println!("Exiting. Use `ph todo done <id>` to mark finished.");
                                continue;
                            }
                        }

                        let todos_ctx = format!(
                            "- {}\n  优先级: {}\n  关联项目: {}\n  状态: {}\n",
                            selected.title,
                            selected.priority,
                            selected.projects.join(", "),
                            selected.status,
                        );

                        loop {
                            if !cli.yes {
                                let stage = stage_display_name(&current_stage);
                                let title = selected.title.clone();
                                let confirmed = tokio::task::spawn_blocking(move || {
                                    let msg = format!("是否进入阶段 '{}' 处理任务 '{}' ?", stage, title);
                                    picker::confirm(&msg)
                                }).await??;
                                if !confirmed {
                                    println!("Exiting workflow.");
                                    break;
                                }
                            }

                            let stage_log_id = service::start_stage(
                                &pool, &selected, &current_stage
                            ).await?;

                            let work_dir = if current_stage == "progress" {
                                let path = infra::create_todo_worktree(&primary_project.path, &todo_doc_id)?;
                                println!("Using worktree: {}", path.display());
                                path.to_string_lossy().to_string()
                            } else {
                                primary_project.path.clone()
                            };

                            let agent_name = service::resolve_agent_for_stage(&current_stage);
                            let agent = infra::load_agent(&agent_name)?;

                            let stage_ctx = build_stage_context(&todo_doc_id, &current_stage);

                            // Load knowledge from all linked projects
                            let mut knowledge = String::new();
                            for proj in &linked_projects {
                                let proj_knowledge = service::load_knowledge_rag(
                                    &pool,
                                    &proj.id,
                                    &selected.title,
                                    &current_stage,
                                    3,
                                ).await?;
                                if !proj_knowledge.is_empty() {
                                    if !knowledge.is_empty() {
                                        knowledge.push('\n');
                                    }
                                    knowledge.push_str(&format!("\n## 项目 {}\n{}", proj.id, proj_knowledge));
                                }
                            }
                            if !knowledge.is_empty() {
                                knowledge.insert_str(
                                    0,
                                    &format!(
                                        "\n> 以下知识片段通过语义检索获得，与「{}」的「{}」阶段最相关。请结合这些背景知识完成本阶段工作。\n",
                                        selected.title,
                                        stage_display_name(&current_stage)
                                    ),
                                );
                            }

                            let task_text = service::stage_instruction(
                                &current_stage,
                                &infra::todo_docs_dir(&todo_doc_id),
                                &agent,
                            );

                            let related = if linked_projects.len() > 1 {
                                &linked_projects[1..]
                            } else {
                                &[]
                            };
                            let prompt = infra::build_prompt(
                                &linked_projects[0],
                                related,
                                &agent,
                                &todos_ctx,
                                &knowledge,
                                &stage_ctx,
                                Some(&task_text),
                            );

                            let final_prompt = if cli.yes {
                                prompt
                            } else {
                                tokio::task::spawn_blocking(move || {
                                    edit_prompt(&prompt)
                                }).await??
                            };

                            let stage_cn = stage_display_name(&current_stage);
                            println!(
                                "\n┌─[{} / stage: {} / agent: {}]─{}─┐",
                                primary_project.id,
                                stage_cn,
                                agent.name,
                                "─".repeat(20usize.saturating_sub(primary_project.id.len() + stage_cn.len() + agent.name.len()))
                            );
                            println!("│  launching claude in {}", work_dir);
                            println!("│  task:  {}", selected.title);
                            println!("└{}┘", "─".repeat(48));

                            let status = std::process::Command::new("claude")
                                .current_dir(&work_dir)
                                .arg(&final_prompt)
                                .status()?;

                            if !status.success() {
                                anyhow::bail!("Claude Code exited with non-zero status");
                            }

                            service::end_stage(&pool, &stage_log_id).await?;

                            let next_stage = service::detect_stage(&todo_doc_id);
                            if next_stage == current_stage {
                                println!("Stage did not progress. Exiting workflow.");
                                break;
                            }
                            if next_stage == "done" {
                                println!("All stages complete.");
                                break;
                            }
                            current_stage = next_stage;
                        }

                        let mark_done = if cli.yes {
                            true
                        } else {
                            tokio::task::spawn_blocking(move || {
                                picker::confirm("Mark todo as done?")
                            }).await??
                        };

                        if mark_done {
                            for id in &selected.ids {
                                service::done_todo_with_time(&pool, id).await?;
                            }
                            println!("[✓] mission complete");
                        }
                    }
                    picker::Action::Add { title, priority, status, projects } => {
                        service::add_work_item(&pool, &title, &status, priority, &projects).await?;
                        println!("todo added");
                    }
                    picker::Action::Edit { index, title, priority, status, projects } => {
                        if let Some(original) = work_items.get(index) {
                            service::edit_work_item(&pool, original, &title, &status, priority, &projects).await?;
                            println!("todo updated");
                        }
                    }
                    picker::Action::Delete(idx) => {
                        if let Some(item) = work_items.get(idx) {
                            let title = item.title.clone();
                            let confirmed = tokio::task::spawn_blocking(move || {
                                picker::confirm(&format!("确认删除任务 '{}' ?", title))
                            }).await??;
                            if confirmed {
                                service::delete_todos_by_ids(&pool, &item.ids).await?;
                                println!("todo removed");
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn build_stage_context(todo_doc_id: &str, current_stage: &str) -> String {
    let mut ctx = String::new();
    let stages: &[(&str, &str)] = &[
        ("01", "requirements"),
        ("02", "design"),
        ("03", "tasks"),
        ("04", "progress"),
        ("05", "review"),
    ];
    for (num, stage) in stages {
        if *stage == current_stage {
            break;
        }
        let doc = service::load_stage_doc(todo_doc_id, stage);
        if !doc.is_empty() {
            ctx.push_str(&format!("\n\n# 阶段 {}：{}\n{}", num, stage, doc));
        }
    }
    ctx.trim_start().to_string()
}

fn edit_prompt(prompt: &str) -> std::io::Result<String> {
    let path = infra::data_dir().join("prompts").join("last-work-prompt.md");
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, prompt)?;

    let edit = picker::confirm("启动 Claude 前是否预览/编辑 prompt？")?;
    if edit {
        let editor = std::env::var("EDITOR")
            .ok()
            .unwrap_or_else(|| "vi".to_string());
        let status = std::process::Command::new(&editor)
            .arg(&path)
            .status()?;
        if !status.success() {
            eprintln!("Warning: editor exited with non-zero status");
        }
    }

    std::fs::read_to_string(&path)
}

fn stage_display_name(stage: &str) -> String {
    match stage {
        "requirements" => "需求澄清".to_string(),
        "design" => "方案设计".to_string(),
        "tasks" => "任务拆解".to_string(),
        "progress" => "编码实现".to_string(),
        "review" => "验收回顾".to_string(),
        "done" => "已完成".to_string(),
        _ => stage.to_string(),
    }
}

