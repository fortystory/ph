use anyhow::Result;

use crate::fs::knowledge_dir;

pub fn load_knowledge(project_id: &str) -> Result<String> {
    let dir = knowledge_dir().join(project_id);

    let mut result = String::new();

    if !dir.exists() {
        return Ok(result);
    }

    fn walk(dir: &std::path::Path, base: &std::path::Path, result: &mut String) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let content = std::fs::read_to_string(&path)?;
                let rel = path.strip_prefix(base)?.to_string_lossy();
                result.push_str(&format!(
                    "\n\n# 文件：{}\n{}",
                    rel,
                    content
                ));
            } else if path.is_dir() {
                walk(&path, base, result)?;
            }
        }
        Ok(())
    }

    walk(&dir, &dir, &mut result)?;

    Ok(result)
}
