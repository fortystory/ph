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
                    service::done_todo(&pool, &id).await?;
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
            let projects = service::list_projects()?;

            let work_items = service::list_pending_work_items(&pool).await?;
            if work_items.is_empty() {
                println!("No pending todos.");
                return Ok(());
            }

            let items: Vec<picker::Item> = work_items
                .iter()
                .enumerate()
                .map(|(idx, item)| {
                    let stage = service::detect_stage(
                        item.ids.first().cloned().as_deref().unwrap_or(&item.short_id)
                    );
                    let stage_cn = match stage.as_str() {
                        "requirements" => "需求澄清",
                        "design" => "方案设计",
                        "tasks" => "任务拆解",
                        "progress" => "编码实现",
                        "review" => "验收回顾",
                        "done" => "已完成",
                        _ => &stage,
                    };
                    picker::Item {
                        title: item.title.clone(),
                        priority: item.priority,
                        short_id: item.short_id.clone(),
                        projects: item.projects.join(" "),
                        detail_lines: vec![
                            format!("阶段: {}", stage_cn),
                            format!("ID 列表: {}", item.ids.join(", ")),
                        ],
                        index: idx,
                    }
                })
                .collect();

            let selected_idx = picker::pick(items)?;
            let Some(selected_idx) = selected_idx else {
                return Ok(());
            };
            let selected = work_items.into_iter().nth(selected_idx).unwrap();

            let target_project_id = selected.projects.first()
                .ok_or_else(|| anyhow::anyhow!("no project linked to this todo"))?
                .clone();

            let project = projects
                .into_iter()
                .find(|p| p.id == target_project_id)
                .ok_or_else(|| anyhow::anyhow!("project not found"))?;

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
                    return Ok(());
                }
            }

            let todos_ctx = infra::load_todo_context(&pool, &project.id).await?;
            let knowledge = infra::load_knowledge(&project.id)?;

            loop {
                if !cli.yes {
                    let stage = current_stage.clone();
                    let title = selected.title.clone();
                    let confirmed = tokio::task::spawn_blocking(move || {
                        let msg = format!("Enter stage '{}' for '{}' ?", stage, title);
                        picker::confirm(&msg)
                    }).await??;
                    if !confirmed {
                        println!("Exiting workflow.");
                        break;
                    }
                }

                let work_dir = if current_stage == "progress" {
                    let path = infra::create_todo_worktree(&project.path, &todo_doc_id)?;
                    println!("Using worktree: {}", path.display());
                    path.to_string_lossy().to_string()
                } else {
                    project.path.clone()
                };

                let agent_name = service::resolve_agent_for_stage(&current_stage);
                let agent = infra::load_agent(&agent_name)?;

                let stage_ctx = build_stage_context(&todo_doc_id, &current_stage);

                let task_text = format!(
                    "Current stage: {}. Agent role: {}.\n\n\
                     Please work through this stage and save your outputs to the appropriate document in {}.\n\
                     When finished, exit Claude Code so the workflow can continue.",
                    current_stage,
                    agent.name,
                    infra::todo_docs_dir(&todo_doc_id).display()
                );

                let prompt = infra::build_prompt(
                    &project,
                    &agent,
                    &todos_ctx,
                    &knowledge,
                    &stage_ctx,
                    Some(&task_text),
                );

                println!(
                    "\n┌─[{} / stage: {} / agent: {}]─{}─┐",
                    project.id,
                    current_stage,
                    agent.name,
                    "─".repeat(20usize.saturating_sub(project.id.len() + current_stage.len() + agent.name.len()))
                );
                println!("│  launching claude in {}", work_dir);
                println!("│  task:  {}", selected.title);
                println!("└{}┘", "─".repeat(48));

                let status = std::process::Command::new("claude")
                    .current_dir(&work_dir)
                    .arg(&prompt)
                    .status()?;

                if !status.success() {
                    anyhow::bail!("Claude Code exited with non-zero status");
                }

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
                    service::done_todo(&pool, id).await?;
                }
                println!("[✓] mission complete");
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
            ctx.push_str(&format!("\n\n# Stage {}: {}\n{}", num, stage, doc));
        }
    }
    ctx.trim_start().to_string()
}
