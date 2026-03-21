mod commands;
mod log_reader;
mod skill_catalog;

use commands::{get_default_log_path, get_skill_catalog};
use log_reader::{
    minimize_overlay, open_main_menu, poll_overlay_state, set_overlay_enabled,
    show_main_window, start_overlay_drag, OverlayState,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::image::Image;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{CursorIcon, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const SETTINGS_WINDOW_LABEL: &str = "main";
const OVERLAY_WINDOW_LABEL: &str = "overlay";
const TRAY_SHOW_ID: &str = "show_overlay";
const TRAY_QUIT_ID: &str = "quit_app";
const WINDOW_STATE_FILE: &str = "window-state.json";

#[derive(Default, Serialize, Deserialize, Clone)]
struct WindowStateStore {
    main: Option<SavedWindowPosition>,
    overlay: Option<SavedWindowPosition>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct SavedWindowPosition {
    x: i32,
    y: i32,
}

fn hide_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        let _ = window.hide();
    }
}

#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
pub fn run() {
    let app_icon = Image::from_bytes(include_bytes!("../icons/icon.png")).ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(OverlayState::new())
        .setup(move |app| {
            let saved_state = load_window_state(app.handle().clone());

            if app.get_webview_window(OVERLAY_WINDOW_LABEL).is_none() {
                let overlay_window = WebviewWindowBuilder::new(
                    app,
                    OVERLAY_WINDOW_LABEL,
                    WebviewUrl::App("index.html?view=overlay".into()),
                )
                .title("Fellowship Overlay")
                .inner_size(160.0, 72.0)
                .resizable(false)
                .transparent(true)
                .decorations(false)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(true)
                .build()?;

                let _ = overlay_window.set_shadow(false);
                let _ = overlay_window.set_cursor_visible(true);
                let _ = overlay_window.set_cursor_icon(CursorIcon::Default);
                restore_window_position(&overlay_window, saved_state.overlay);
                register_window_position_tracking(app.handle().clone(), &overlay_window, OVERLAY_WINDOW_LABEL);
            }

            let show_item = tauri::menu::MenuItem::with_id(app, TRAY_SHOW_ID, "Show overlay", true, None::<&str>)?;
            let quit_item = tauri::menu::MenuItem::with_id(app, TRAY_QUIT_ID, "Quit", true, None::<&str>)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let mut tray_builder = TrayIconBuilder::with_id("main-tray")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .tooltip("Fellowship Trinket Overlay")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    TRAY_SHOW_ID => show_main_window(app),
                    TRAY_QUIT_ID => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(&tray.app_handle());
                    }
                });

            if let Some(icon) = app_icon.clone() {
                tray_builder = tray_builder.icon(icon);
            }

            tray_builder.build(app)?;

            if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
                restore_window_position(&window, saved_state.main);
                register_window_position_tracking(app.handle().clone(), &window, SETTINGS_WINDOW_LABEL);
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        hide_main_window(&app_handle);
                    }
                });
            }

            hide_main_window(&app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_skill_catalog,
            get_default_log_path,
            poll_overlay_state,
            minimize_overlay,
            start_overlay_drag,
            open_main_menu,
            set_overlay_enabled
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn register_window_position_tracking<R: tauri::Runtime>(
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

fn restore_window_position<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
    position: Option<SavedWindowPosition>,
) {
    if let Some(position) = position {
        let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
    }
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

fn load_window_state<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> WindowStateStore {
    let Some(path) = window_state_path(&app) else {
        return WindowStateStore::default();
    };

    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<WindowStateStore>(&contents).ok())
        .unwrap_or_default()
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

fn window_state_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(WINDOW_STATE_FILE))
}
