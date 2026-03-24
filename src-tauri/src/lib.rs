mod app_settings;
mod commands;
mod constants;
mod log_reader;
mod skill_catalog;
mod window_position;

use commands::{get_default_log_path, get_skill_catalog};
use log_reader::{
    choose_log_directory, get_overlay_state, minimize_overlay, open_main_menu, set_log_directory,
    set_overlay_enabled, start_overlay_drag, OverlayState,
};
use window_position::{
    WindowTracker, restore_window_position, load_window_state, show_main_window,
};

use crate::constants::{SETTINGS_WINDOW_LABEL, OVERLAY_WINDOW_LABEL};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{CursorIcon, Manager, WebviewUrl, WebviewWindowBuilder};

const TRAY_SHOW_ID: &str = "show_overlay";
const TRAY_QUIT_ID: &str = "quit_app";

fn hide_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        let _ = window.hide();
    }
}

#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
pub fn run() {
    let app_icon = Image::from_bytes(include_bytes!("../icons/icon.png")).ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(OverlayState::new()))
        .setup(move |app| {
            let saved_state = load_window_state(app.handle().clone());

            if app.get_webview_window(OVERLAY_WINDOW_LABEL).is_none() {
                let overlay_window = WebviewWindowBuilder::new(
                    app,
                    OVERLAY_WINDOW_LABEL,
                    WebviewUrl::App("index.html?view=overlay".into()),
                )
                .title("Trinkets")
                .inner_size(160.0, 72.0)
                .resizable(false)
                .transparent(true)
                .decorations(false)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .build()?;

                let _ = overlay_window.set_shadow(false);
                let _ = overlay_window.set_cursor_visible(true);
                let _ = overlay_window.set_cursor_icon(CursorIcon::Default);
                restore_window_position(&overlay_window, saved_state.overlay);
                let tracker = WindowTracker::new();
                tracker.register(app.handle().clone(), &overlay_window, OVERLAY_WINDOW_LABEL);
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
                let tracker = WindowTracker::new();
                tracker.register(app.handle().clone(), &window, SETTINGS_WINDOW_LABEL);
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        hide_main_window(&app_handle);
                    }
                });
            }

            show_main_window(&app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_skill_catalog,
            get_default_log_path,
            choose_log_directory,
            get_overlay_state,
            minimize_overlay,
            start_overlay_drag,
            open_main_menu,
            set_log_directory,
            set_overlay_enabled
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
