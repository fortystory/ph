use anyhow::Result;

use crate::fs::knowledge_dir;

pub fn load_knowledge(project_id: &str) -> Result<String> {
    let dir = knowledge_dir().join(project_id);

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
