use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::AppHandle;
use tauri::Manager;

const APP_SETTINGS_FILE: &str = "app-settings.json";

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct AppSettingsStore {
    pub log_directory: Option<String>,
}

pub fn load_app_settings<R: tauri::Runtime>(app: &AppHandle<R>) -> AppSettingsStore {
    let Some(path) = app_settings_path(app) else {
        return AppSettingsStore::default();
    };

    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<AppSettingsStore>(&contents).ok())
        .unwrap_or_default()
}

pub fn save_app_settings<R: tauri::Runtime>(app: &AppHandle<R>, settings: &AppSettingsStore) {
    let Some(path) = app_settings_path(app) else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(contents) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, contents);
    }
}

fn app_settings_path<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir: PathBuf| dir.join(APP_SETTINGS_FILE))
}
