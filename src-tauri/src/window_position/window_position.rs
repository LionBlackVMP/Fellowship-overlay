use async_std::task::sleep;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, Position, WebviewWindow};

use crate::constants::{GAME_PROCESS_NAMES, OVERLAY_WINDOW_LABEL, SETTINGS_WINDOW_LABEL};
use crate::log_reader::{OverlayRuntime, OverlaySnapshot};

const MAIN_WINDOW_MIN_WIDTH: f64 = 940.0;
const MAIN_WINDOW_MIN_HEIGHT: f64 = 560.0;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{CloseHandle, HWND};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_HWNDPARENT, HWND_NOTOPMOST,
    HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE, SW_SHOW, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

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

pub fn restore_window_position<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
    position: Option<SavedWindowPosition>,
) {
    if let Some(position) = position {
        let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
    }
}

pub fn load_window_state<R: tauri::Runtime>(app: AppHandle<R>) -> WindowStateStore {
    let Some(path) = window_state_path(&app) else {
        return WindowStateStore::default();
    };

    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<WindowStateStore>(&contents).ok())
        .unwrap_or_default()
}

fn save_window_position<R: tauri::Runtime>(
    app: &AppHandle<R>,
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

fn write_window_state<R: tauri::Runtime>(app: &AppHandle<R>, state: &WindowStateStore) {
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

pub fn window_state_path<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir: PathBuf| dir.join(WINDOW_STATE_FILE))
}

pub fn resize_overlay_window<R: tauri::Runtime>(app: &AppHandle<R>, snapshot: &OverlaySnapshot) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };

    let player_count = snapshot.players.len().max(1) as f64;
    let outer_padding = 6.0;
    let frame_gap = 3.0;
    let frame_height = 106.0;
    let width = outer_padding * 2.0 + 116.0;
    let height =
        outer_padding * 2.0 + player_count * frame_height + (player_count - 1.0) * frame_gap;

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

pub fn show_main_window<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        let _ = window.set_min_size(Some(LogicalSize::new(
            MAIN_WINDOW_MIN_WIDTH,
            MAIN_WINDOW_MIN_HEIGHT,
        )));
        if let Ok(size) = window.outer_size() {
            let target_width = (size.width as f64).max(MAIN_WINDOW_MIN_WIDTH);
            let target_height = (size.height as f64).max(MAIN_WINDOW_MIN_HEIGHT);
            if (size.width as f64) < MAIN_WINDOW_MIN_WIDTH || (size.height as f64) < MAIN_WINDOW_MIN_HEIGHT {
                let _ = window.set_size(LogicalSize::new(target_width, target_height));
            }
        }
        detach_main_window(&window);
        ensure_main_window_visible(&window);
        let _ = window.set_title("Fellowship Overlay");
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        bring_main_window_to_front(&window);
    }
}

#[cfg(target_os = "windows")]
fn detach_main_window<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let Ok(window_handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = window_handle.as_raw() else {
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);

    unsafe {
        let _ = SetWindowLongPtrW(hwnd, GWL_HWNDPARENT, 0);
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let cleaned_style = ex_style & !(WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_NOACTIVATE.0 as isize)
            | (WS_EX_APPWINDOW.0 as isize);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, cleaned_style);
    }

    let _ = window.set_always_on_top(false);
    let _ = window.set_skip_taskbar(false);
}

#[cfg(not(target_os = "windows"))]
fn detach_main_window<R: tauri::Runtime>(_: &tauri::WebviewWindow<R>) {}

#[cfg(target_os = "windows")]
fn bring_main_window_to_front<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let Ok(window_handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = window_handle.as_raw() else {
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let _ = SetForegroundWindow(hwnd);
    }
}

#[cfg(not(target_os = "windows"))]
fn bring_main_window_to_front<R: tauri::Runtime>(_: &tauri::WebviewWindow<R>) {}

fn ensure_main_window_visible<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let Ok(position) = window.outer_position() else {
        center_main_window(window);
        return;
    };
    let Ok(size) = window.outer_size() else {
        center_main_window(window);
        return;
    };
    let Ok(monitors) = window.available_monitors() else {
        return;
    };

    let fits_any_monitor = monitors.iter().any(|monitor| {
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();

        let visible_left = position.x.max(monitor_position.x);
        let visible_top = position.y.max(monitor_position.y);
        let visible_right =
            (position.x + size.width as i32).min(monitor_position.x + monitor_size.width as i32);
        let visible_bottom =
            (position.y + size.height as i32).min(monitor_position.y + monitor_size.height as i32);

        visible_right - visible_left >= 120 && visible_bottom - visible_top >= 80
    });

    if !fits_any_monitor {
        center_main_window(window);
    }
}

fn center_main_window<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return;
    };

    let Ok(size) = window.outer_size() else {
        return;
    };

    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let x = monitor_position.x + ((monitor_size.width as i32 - size.width as i32) / 2).max(0);
    let y = monitor_position.y + ((monitor_size.height as i32 - size.height as i32) / 2).max(0);

    let _ = window.set_position(Position::Logical(LogicalPosition::new(x as f64, y as f64)));
}

#[cfg(target_os = "windows")]
pub fn sync_overlay_visibility<R: tauri::Runtime>(app: &AppHandle<R>, runtime: &mut OverlayRuntime) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };
    let is_visible = window.is_visible().unwrap_or(false);
    let is_minimized = window.is_minimized().unwrap_or(false);

    if !runtime.overlay_enabled {
        runtime.overlay_visibility_misses = 0;
        if is_visible {
            let _ = window.hide();
        }
        return;
    }

    if runtime.manually_minimized || is_minimized {
        return;
    }

    if is_overlay_allowed_foreground() {
        runtime.overlay_visibility_misses = 0;
        if !is_visible {
            let _ = window.show();
        }
    } else {
        runtime.overlay_visibility_misses = runtime.overlay_visibility_misses.saturating_add(1);
        if runtime.overlay_visibility_misses >= 3 && is_visible {
            let _ = window.hide();
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn sync_overlay_visibility<R: tauri::Runtime>(_: &AppHandle<R>, _: &mut OverlayRuntime) {}

#[cfg(target_os = "windows")]
fn is_overlay_allowed_foreground() -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return false;
    }

    let mut process_id = 0u32;
    unsafe {
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }
    if process_id == 0 {
        return false;
    }

    let Ok(exe_name) = process_name_from_pid(process_id) else {
        return false;
    };
    let normalized_name = exe_name.to_ascii_lowercase();
    GAME_PROCESS_NAMES
        .iter()
        .any(|game_name| normalized_name == *game_name)
        || normalized_name == "fellowship-overlay.exe"
}

#[cfg(target_os = "windows")]
fn process_name_from_pid(process_id: u32) -> Result<String, ()> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
            .map_err(|_| ())?;

        let mut buffer = vec![0u16; 260];
        let mut length = buffer.len() as u32;
        let query_result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        if query_result.is_err() {
            return Err(());
        }

        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        let file_name = std::path::Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(())?;
        Ok(file_name.to_string())
    }
}

#[derive(Clone)]
pub struct WindowTracker {
    cached_position: Arc<Mutex<Option<PhysicalPosition<i32>>>>,
    save_scheduled: Arc<Mutex<bool>>,
}

impl WindowTracker {
    pub fn new() -> Self {
        Self {
            cached_position: Arc::new(Mutex::new(None)),
            save_scheduled: Arc::new(Mutex::new(false)),
        }
    }

    pub fn register<R: tauri::Runtime>(
        &self,
        app: AppHandle<R>,
        window: &WebviewWindow<R>,
        label: &'static str,
    ) {
        let cached_pos = self.cached_position.clone();
        let save_flag = self.save_scheduled.clone();
        let tracked_window = window.clone();

        window.on_window_event(move |event| {
            if let tauri::WindowEvent::Moved(_) = event {
                if let Ok(pos) = tracked_window.outer_position() {
                    if let Ok(mut cached) = cached_pos.lock() {
                        *cached = Some(pos);
                    }

                    let Ok(mut flag) = save_flag.lock() else {
                        return;
                    };
                    if !*flag {
                        *flag = true;

                        let cached_clone = cached_pos.clone();
                        let app_clone = app.clone();
                        let save_flag_clone = save_flag.clone();
                        let tracked_label = label;

                        tauri::async_runtime::spawn(async move {
                            sleep(Duration::from_secs(1)).await;

                            if let Ok(cached) = cached_clone.lock() {
                                if let Some(pos) = *cached {
                                    save_window_position(&app_clone, tracked_label, pos);
                                }
                            }

                            if let Ok(mut pending) = save_flag_clone.lock() {
                                *pending = false;
                            }
                        });
                    }
                }
            }
        });
    }
}
