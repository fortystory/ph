use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,

    pub agent: Option<String>,
    pub prompt: Option<String>,
    pub knowledge: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectsFile {
    pub projects: Vec<Project>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Agent {
    pub name: String,
    pub model: String,
    pub system_prompt: String,
    pub stage_prompts: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug)]
pub struct Todo {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub priority: i32,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug)]
pub struct TodoStageLog {
    pub id: String,
    pub title: String,
    pub priority: i32,
    pub created_at: String,
    pub stage: String,
    pub started_at: String,
    pub ended_at: Option<String>,
}

#[derive(Debug)]
pub struct KnowledgeChunk {
    pub id: String,
    pub project_id: String,
    pub file_path: String,
    pub chunk_index: i32,
    pub content: String,
    pub content_hash: String,
    pub updated_at: String,
}
