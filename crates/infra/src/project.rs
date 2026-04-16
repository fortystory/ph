use anyhow::Result;
use core::ProjectsFile;
use std::fs;

use crate::fs::data_dir;

pub fn projects_file_path() -> std::path::PathBuf {
    data_dir().join("projects.toml")
}

pub fn load_projects() -> Result<ProjectsFile> {
    let path = projects_file_path();

    if !path.exists() {
        return Ok(ProjectsFile { projects: vec![] });
    }

    let content = fs::read_to_string(path)?;
    let data: ProjectsFile = toml::from_str(&content)?;
    Ok(data)
}

pub fn save_projects(data: &ProjectsFile) -> Result<()> {
    let content = toml::to_string_pretty(data)?;
    fs::write(projects_file_path(), content)?;
    Ok(())
}
