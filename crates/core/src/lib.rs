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
}

#[derive(Debug)]
pub struct Todo {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub priority: i32,
    pub created_at: String,
}
