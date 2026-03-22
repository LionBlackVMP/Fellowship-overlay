use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{Manager, PhysicalPosition, WebviewWindow, AppHandle, LogicalSize};
use crate::log_reader::{OverlaySnapshot};
use crate::constants::{SETTINGS_WINDOW_LABEL, OVERLAY_WINDOW_LABEL};

const WINDOW_STATE_FILE: &str = "window-state.json";

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct WindowStateStore {
    pub main: Option<SavedWindowPosition>,
    pub overlay: Option<SavedWindowPosition>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct SavedWindowPosition {
    pub x: i32,
    pub y: i32,
}

pub fn register_window_position_tracking<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    window: &WebviewWindow<R>,
    label: &'static str,
) {
    let tracked_window = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(_) = event {
            if let Ok(position) = tracked_window.outer_position() {
                save_window_position(&app, label, position);
            }
        }
    });
}

pub fn restore_window_position<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
    position: Option<SavedWindowPosition>,
) {
    if let Some(position) = position {
        let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
    }
}

pub fn load_window_state<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> WindowStateStore {
    let Some(path) = window_state_path(&app) else {
        return WindowStateStore::default();
    };

    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<WindowStateStore>(&contents).ok())
        .unwrap_or_default()
}

fn save_window_position<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    label: &str,
    position: PhysicalPosition<i32>,
) {
    let mut state = load_window_state(app.clone());
    let saved_position = Some(SavedWindowPosition {
        x: position.x,
        y: position.y,
    });

    match label {
        SETTINGS_WINDOW_LABEL => state.main = saved_position,
        OVERLAY_WINDOW_LABEL => state.overlay = saved_position,
        _ => return,
    }

    write_window_state(app, &state);
}

fn write_window_state<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &WindowStateStore) {
    let Some(path) = window_state_path(app) else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(contents) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, contents);
    }
}

pub fn window_state_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(WINDOW_STATE_FILE))
}

pub fn resize_overlay_window<R: tauri::Runtime>(app: &AppHandle<R>, snapshot: &OverlaySnapshot) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };

    let player_count = snapshot.players.len().max(1) as f64;
    let outer_padding = 6.0;
    let frame_gap = 3.0;
    let frame_height = 84.0;
    let width = outer_padding * 2.0 + 117.0;
    let height = outer_padding * 2.0 + player_count * frame_height + (player_count - 1.0) * frame_gap;

    let should_resize = window
        .inner_size()
        .map(|size| {
            let width_diff = (size.width as f64 - width).abs();
            let height_diff = (size.height as f64 - height).abs();
            width_diff > 1.0 || height_diff > 1.0
        })
        .unwrap_or(true);

    if should_resize {
        let _ = window.set_size(LogicalSize::new(width, height));
    }
}
