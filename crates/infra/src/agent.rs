use anyhow::Result;

use crate::fs::data_dir;

pub fn load_agent(name: &str) -> Result<core::Agent> {
    let path = data_dir().join("agents").join(format!("{}.toml", name));
    let content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        let default_path = data_dir().join("agents").join("default.toml");
        std::fs::read_to_string(default_path)?
    };
    Ok(toml::from_str(&content)?)
}
