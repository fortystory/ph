pub mod agent;
pub mod db;
pub mod fs;
pub mod git;
pub mod knowledge;
pub mod llm;
pub mod project;
pub mod todo;

pub use agent::load_agent;
pub use db::init_db;
pub use fs::{
    create_knowledge_dir, create_project_symlink, create_todo_docs_dir, data_dir, docs_dir,
    init_dirs, knowledge_dir, todo_docs_dir, workspace_dir,
};
pub use git::{
    create_todo_worktree, remove_todo_worktree, run_git_command,
};
pub use knowledge::load_knowledge;
pub use llm::{build_prompt, call_claude};
pub use project::{load_projects, projects_file_path, save_projects};
pub use todo::{add_todo, delete_todo, list_pending_todos, list_todos, load_todo_context, mark_done};
