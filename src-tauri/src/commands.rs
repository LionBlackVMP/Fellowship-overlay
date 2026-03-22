use crate::log_reader::{resolve_latest_log_path};
use crate::constants::{DEFAULT_LOG_DIR};
use crate::skill_catalog::{build_catalog, SkillCatalog};
use std::path::Path;
use tauri::command;

#[command]
pub fn get_skill_catalog() -> SkillCatalog {
    let skills_path = Path::new("src/skills.json");
    let heroes_dir = Path::new("src/Heroes");
    build_catalog(skills_path, heroes_dir)
}

#[command]
pub fn get_default_log_path() -> Result<String, String> {
    resolve_latest_log_path(DEFAULT_LOG_DIR).map(|path| path.to_string_lossy().to_string())
}
