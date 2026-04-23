pub mod agent;
pub mod chunk;
pub mod db;
pub mod embed;
pub mod fs;
pub mod git;
pub mod knowledge;
pub mod knowledge_index;
pub mod llm;
pub mod mcp_server;
pub mod project;
pub mod todo;

pub use agent::load_agent;
pub use db::init_db;
pub use embed::Embedder;
pub use fs::{
    create_knowledge_dir, create_project_symlink, create_todo_docs_dir, data_dir, docs_dir,
    init_dirs, knowledge_dir, todo_docs_dir, workspace_dir,
};
pub use git::{
    create_todo_worktree, remove_todo_worktree, run_git_command,
};
pub use knowledge::load_knowledge;
pub use knowledge_index::{clear_index, is_stale, search, update_index};
pub use llm::{build_prompt, call_claude};
pub use mcp_server::run_mcp_server;
pub use project::{load_projects, projects_file_path, save_projects};
pub use todo::{
    add_todo, delete_todo, delete_todos_by_group, delete_todos_by_ids, end_stage_log,
    insert_stage_log, insert_todo, list_pending_todos, list_stage_logs, list_todos,
    load_todo_context, mark_done_with_time, update_todos_by_ids,
};
