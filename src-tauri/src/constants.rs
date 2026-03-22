pub const SETTINGS_WINDOW_LABEL: &str = "main";
pub const OVERLAY_WINDOW_LABEL: &str = "overlay";
#[cfg(target_os = "windows")]
pub const GAME_PROCESS_NAMES: &[&str] = &["fellowship.exe", "fellowship-win64-shipping.exe"];
pub const DEFAULT_LOG_DIR: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Fellowship\fellowship\Saved\CombatLogs";