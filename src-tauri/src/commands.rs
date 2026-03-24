use crate::app_settings::load_app_settings;
use crate::skill_catalog::{build_catalog, SkillCatalog};
use std::path::Path;
use tauri::command;
use tauri::AppHandle;

#[command]
pub fn get_skill_catalog() -> SkillCatalog {
    let skills_path = Path::new("src/skills.json");
    let heroes_dir = Path::new("src/Heroes");
    build_catalog(skills_path, heroes_dir)
}

#[command]
pub fn get_default_log_path(app: AppHandle) -> Result<String, String> {
    if let Some(saved_log_directory) = load_app_settings(&app).log_directory {
        return Ok(saved_log_directory);
    }

    Ok(String::new())
}
