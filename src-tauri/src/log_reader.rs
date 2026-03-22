use crate::window_position::{
resize_overlay_window
};
use crate::constants::{DEFAULT_LOG_DIR, OVERLAY_WINDOW_LABEL, GAME_PROCESS_NAMES};

use base64::Engine;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{CloseHandle, HWND};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId, SetWindowLongPtrW,
    GWL_EXSTYLE, GWL_HWNDPARENT, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

const OVERLAY_SNAPSHOT_EVENT: &str = "overlay://snapshot";

#[derive(Default)]
struct ReaderCursor {
    offset: u64,
    remainder: String,
    path: Option<String>,
}

#[derive(Clone)]
struct RelicMeta {
    id: u32,
    name: String,
    base_cooldown: u32,
    icon_src: String,
}

struct ActiveCooldown {
    key: String,
    player: String,
    relic_id: u32,
    relic_name: String,
    relic_icon_src: String,
    duration_seconds: u32,
    started_at_ms: u64,
}

struct CombatantInfo {
    player_name: String,
    class_id: u32,
    diamond_gem_power: u32,
}

struct OverlayRuntime {
    cursor: ReaderCursor,
    overlay_enabled: bool,
    dungeon_active: bool,
    players: BTreeSet<String>,
    player_classes: HashMap<String, u32>,
    player_relic_cdr: HashMap<String, f64>,
    equipped_relics: HashMap<String, BTreeMap<u32, RelicMeta>>,
    active_cooldowns: HashMap<String, ActiveCooldown>,
    processed_line_count: usize,
    overlay_visibility_misses: u8,
    gem_color_indices: HashMap<String, usize>,
    relics_by_activation: HashMap<u32, RelicMeta>,
    relics_by_item_id: HashMap<u32, RelicMeta>,
    manually_minimized: bool,
}

pub struct OverlayState {
    runtime: Mutex<OverlayRuntime>,
    last_snapshot: Mutex<Option<OverlaySnapshot>>,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct CooldownView {
    pub key: String,
    pub relic_id: u32,
    pub relic_name: String,
    pub relic_icon_src: String,
    pub duration_seconds: u32,
    pub remaining_seconds: u32,
    pub progress: f64,
    pub ready: bool,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct PlayerOverlay {
    pub name: String,
    pub class_id: Option<u32>,
    pub class_color: String,
    pub cooldowns: Vec<CooldownView>,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct OverlaySnapshot {
    pub resolved_path: String,
    pub overlay_enabled: bool,
    pub dungeon_active: bool,
    pub processed_line_count: usize,
    pub players: Vec<PlayerOverlay>,
}

#[derive(Deserialize)]
struct RawRelic {
    name: String,
    base_cooldown: u32,
    icon: String,
}

#[derive(Deserialize)]
struct RawCatalog {
    relics: HashMap<String, RawRelic>,
    item_mapping: HashMap<String, u32>,
}

#[derive(Deserialize)]
struct RawGemColorEntry {
    id: usize,
    label: String,
}

#[derive(Deserialize)]
struct RawGemCatalog {
    colors: HashMap<String, RawGemColorEntry>,
}

impl OverlayState {
    pub fn new() -> Self {
        Self {
            runtime: Mutex::new(OverlayRuntime {
                cursor: ReaderCursor::default(),
                overlay_enabled: true,
                dungeon_active: false,
                players: BTreeSet::new(),
                player_classes: HashMap::new(),
                player_relic_cdr: HashMap::new(),
                equipped_relics: HashMap::new(),
                active_cooldowns: HashMap::new(),
                processed_line_count: 0,
                overlay_visibility_misses: 0,
                gem_color_indices: load_gem_color_indices().unwrap_or_default(),
                relics_by_activation: load_relics_by_activation().unwrap_or_default(),
                relics_by_item_id: load_relics_by_item_id().unwrap_or_default(),
                manually_minimized: false, // <- добавлено
            }),
            last_snapshot: Mutex::new(None),
        }
    }
}

#[tauri::command]
pub fn get_overlay_state(
    path: Option<String>,
    manage_window: Option<bool>,
    state: State<'_, OverlayState>,
    app: AppHandle,
) -> Result<OverlaySnapshot, String> {
    let path = path.unwrap_or_else(|| DEFAULT_LOG_DIR.to_string());
    refresh_overlay_snapshot(&path, manage_window.unwrap_or(true), &state, &app)
}

#[tauri::command]
pub fn minimize_overlay(window: WebviewWindow, state: State<'_, OverlayState>) -> Result<(), String> {
    if let Ok(mut runtime) = state.runtime.lock() {
        runtime.manually_minimized = true; // отметим, что свернули вручную
    }
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_overlay_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_main_menu(app: AppHandle) -> Result<(), String> {
    show_main_window(&app);
    Ok(())
}

#[tauri::command]
pub fn set_overlay_enabled(
    enabled: bool,
    state: State<'_, OverlayState>,
    app: AppHandle,
) -> Result<OverlaySnapshot, String> {
    let mut runtime = state.runtime.lock().map_err(|e| e.to_string())?;
    runtime.overlay_enabled = enabled;
    runtime.overlay_visibility_misses = 0;
    sync_overlay_visibility(&app, &mut runtime);
    drop(runtime);

    let snapshot = refresh_overlay_snapshot(DEFAULT_LOG_DIR, true, &state, &app)?;
    let _ = app.emit(OVERLAY_SNAPSHOT_EVENT, &snapshot);
    if let Ok(mut last_snapshot) = state.last_snapshot.lock() {
        *last_snapshot = Some(snapshot.clone());
    }
    Ok(snapshot)
}

fn refresh_overlay_snapshot<R: tauri::Runtime>(
    path: &str,
    manage_window: bool,
    state: &State<'_, OverlayState>,
    app: &AppHandle<R>,
) -> Result<OverlaySnapshot, String> {
    let resolved_path = resolve_latest_log_path(path)?;
    let resolved_path_string = resolved_path.to_string_lossy().to_string();
    let mut runtime = state.runtime.lock().map_err(|e| e.to_string())?;

    if runtime.cursor.path.as_deref() != Some(resolved_path_string.as_str()) {
        bootstrap_runtime_from_log(&resolved_path, &mut runtime, &resolved_path_string)?;
    }

    let lines = read_new_lines(&resolved_path, &mut runtime.cursor)?;
    let dungeon_started_now = apply_lines_to_runtime(&mut runtime, &lines);
    if dungeon_started_now {
        show_main_window(app);
    }

    let snapshot = build_snapshot(&runtime, resolved_path_string);
    if manage_window {
        resize_overlay_window(app, &snapshot);
        sync_overlay_visibility(app, &mut runtime);
    }

    Ok(snapshot)
}

fn apply_lines_to_runtime(
    runtime: &mut OverlayRuntime,
    lines: &[String],
) -> bool {
    let mut dungeon_started_now = false;
    let mut combatant_snapshot: BTreeSet<String> = BTreeSet::new();
    let mut class_snapshot: HashMap<String, u32> = HashMap::new();
    let mut relic_cdr_snapshot: HashMap<String, f64> = HashMap::new();
    let mut equipped_snapshot: HashMap<String, BTreeMap<u32, RelicMeta>> = HashMap::new();

    for line in lines {
        runtime.processed_line_count += 1;

        if is_dungeon_start(line) && !runtime.dungeon_active {
            runtime.dungeon_active = true;
            runtime.players.clear();
            runtime.player_classes.clear();
            runtime.player_relic_cdr.clear();
            runtime.equipped_relics.clear();
            runtime.active_cooldowns.clear();
            dungeon_started_now = true;
        }

        if !runtime.dungeon_active {
            continue;
        }

        if let Some(combatant_info) = parse_combatant_info(line, &runtime.gem_color_indices) {
            let player_name = combatant_info.player_name.clone();
            combatant_snapshot.insert(player_name.clone());
            class_snapshot.insert(player_name.clone(), combatant_info.class_id);
            relic_cdr_snapshot.insert(
                player_name.clone(),
                relic_cooldown_multiplier(combatant_info.class_id, combatant_info.diamond_gem_power),
            );
            let equipped = parse_equipped_relics(line, &runtime.relics_by_item_id);
            if !equipped.is_empty() {
                equipped_snapshot.insert(
                    player_name,
                    equipped
                        .into_iter()
                        .map(|relic| (relic.id, relic))
                        .collect::<BTreeMap<_, _>>(),
                );
            }
        } else if let Some(player_name) = parse_player_name(line) {
            runtime.players.insert(player_name);
        }

        if let Some((player, relic, started_at_ms)) =
            parse_relic_trigger(line, &runtime.relics_by_activation)
        {
            let duration_seconds = adjusted_relic_cooldown(
                relic.base_cooldown,
                runtime.player_relic_cdr.get(&player).copied().unwrap_or(1.0),
            );
            let key = format!("{player}:{}", relic.id);
            runtime.active_cooldowns.insert(
                key.clone(),
                ActiveCooldown {
                    key,
                    player,
                    relic_id: relic.id,
                    relic_name: relic.name.clone(),
                    relic_icon_src: relic.icon_src.clone(),
                    duration_seconds,
                    started_at_ms,
                },
            );
        }
    }

    if !combatant_snapshot.is_empty() {
        runtime.players = combatant_snapshot;
        runtime.player_classes = class_snapshot;
        runtime.player_relic_cdr = relic_cdr_snapshot;
        runtime.equipped_relics = equipped_snapshot;
        let current_players = runtime.players.clone();
        runtime
            .active_cooldowns
            .retain(|_, cooldown| current_players.contains(&cooldown.player));
    }

    prune_expired_cooldowns(&mut runtime.active_cooldowns);

    dungeon_started_now
}

fn build_snapshot(runtime: &OverlayRuntime, resolved_path: String) -> OverlaySnapshot {
    let now = now_ms();
    let mut active_by_player: HashMap<String, HashMap<u32, CooldownView>> = HashMap::new();

    for cooldown in runtime.active_cooldowns.values() {
        let elapsed_seconds = ((now.saturating_sub(cooldown.started_at_ms)) / 1000) as u32;
        let remaining_seconds = cooldown.duration_seconds.saturating_sub(elapsed_seconds);
        if remaining_seconds == 0 {
            continue;
        }

        let progress = if cooldown.duration_seconds == 0 {
            0.0
        } else {
            remaining_seconds as f64 / cooldown.duration_seconds as f64
        };

        active_by_player
            .entry(cooldown.player.clone())
            .or_default()
            .insert(
                cooldown.relic_id,
                CooldownView {
                    key: cooldown.key.clone(),
                    relic_id: cooldown.relic_id,
                    relic_name: cooldown.relic_name.clone(),
                    relic_icon_src: cooldown.relic_icon_src.clone(),
                    duration_seconds: cooldown.duration_seconds,
                    remaining_seconds,
                    progress,
                    ready: false,
                },
            );
    }

    let mut players = runtime
        .players
        .iter()
        .map(|name| {
            let mut cooldowns = runtime
                .equipped_relics
                .get(name)
                .cloned()
                .unwrap_or_default()
                .into_values()
                .map(|relic| {
                    active_by_player
                        .get(name)
                        .and_then(|active| active.get(&relic.id))
                        .cloned()
                        .unwrap_or(CooldownView {
                            key: format!("{name}:{}", relic.id),
                            relic_id: relic.id,
                            relic_name: relic.name,
                            relic_icon_src: relic.icon_src,
                            duration_seconds: adjusted_relic_cooldown(
                                relic.base_cooldown,
                                runtime.player_relic_cdr.get(name).copied().unwrap_or(1.0),
                            ),
                            remaining_seconds: 0,
                            progress: 0.0,
                            ready: true,
                        })
                })
                .collect::<Vec<_>>();

            if cooldowns.is_empty() {
                cooldowns = active_by_player
                    .remove(name)
                    .unwrap_or_default()
                    .into_values()
                    .collect::<Vec<_>>();
            }

            cooldowns.sort_by(|a, b| {
                let a_order = if a.ready { 1 } else { 0 };
                let b_order = if b.ready { 1 } else { 0 };
                a_order
                    .cmp(&b_order)
                    .then(a.remaining_seconds.cmp(&b.remaining_seconds))
                    .then(a.relic_name.cmp(&b.relic_name))
            });

            PlayerOverlay {
                name: name.clone(),
                class_id: runtime.player_classes.get(name).copied(),
                class_color: runtime
                    .player_classes
                    .get(name)
                    .map(|class_id| class_color(*class_id).to_string())
                    .unwrap_or_else(|| "#f1d4a1".to_string()),
                cooldowns,
            }
        })
        .collect::<Vec<_>>();

    players.sort_by(|a, b| a.name.cmp(&b.name));

    OverlaySnapshot {
        resolved_path,
        overlay_enabled: runtime.overlay_enabled,
        dungeon_active: runtime.dungeon_active,
        processed_line_count: runtime.processed_line_count,
        players,
    }
}

fn bootstrap_runtime_from_log(
    resolved_path: &Path,
    runtime: &mut OverlayRuntime,
    path: &str,
) -> Result<(), String> {
    let file_bytes = fs::read(resolved_path).map_err(|e| format!("read error: {e}"))?;
    let file_size = file_bytes.len() as u64;
    let text = String::from_utf8_lossy(&file_bytes).replace("\r\n", "\n");
    let all_lines = text
        .split('\n')
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    runtime.cursor = ReaderCursor {
        offset: file_size,
        remainder: String::new(),
        path: Some(path.to_string()),
    };
    runtime.dungeon_active = false;
    runtime.players.clear();
    runtime.player_classes.clear();
    runtime.player_relic_cdr.clear();
    runtime.equipped_relics.clear();
    runtime.active_cooldowns.clear();
    runtime.processed_line_count = all_lines.len();

    let start_index = all_lines
        .iter()
        .rposition(|line| is_dungeon_start(line))
        .unwrap_or(all_lines.len());

    if start_index == all_lines.len() {
        return Ok(());
    }

    let slice = all_lines[start_index..].to_vec();
    apply_lines_to_runtime(runtime, &slice);
    runtime.processed_line_count = all_lines.len();
    Ok(())
}

fn read_new_lines(path: &Path, cursor: &mut ReaderCursor) -> Result<Vec<String>, String> {
    let mut file = File::open(path).map_err(|e| format!("open error: {e}"))?;
    let metadata = file.metadata().map_err(|e| format!("meta error: {e}"))?;
    let file_size = metadata.len();

    if cursor.offset > file_size {
        cursor.offset = 0;
        cursor.remainder.clear();
    }

    file.seek(SeekFrom::Start(cursor.offset))
        .map_err(|e| format!("seek error: {e}"))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| format!("read error: {e}"))?;
    cursor.offset = file_size;

    let chunk = String::from_utf8_lossy(&buffer);
    let combined = format!("{}{}", cursor.remainder, chunk);
    let normalized = combined.replace("\r\n", "\n");
    let ends_with_newline = normalized.ends_with('\n');

    let mut parts = normalized
        .split('\n')
        .map(|line| line.trim().to_string())
        .collect::<Vec<_>>();

    cursor.remainder = if ends_with_newline {
        String::new()
    } else {
        parts.pop().unwrap_or_default()
    };

    Ok(parts.into_iter().filter(|line| !line.is_empty()).collect())
}

fn parse_combatant_info(
    line: &str,
    gem_color_indices: &HashMap<String, usize>,
) -> Option<CombatantInfo> {
    let parts = line.split('|').collect::<Vec<_>>();
    if parts.len() < 11 || parts[1] != "COMBATANT_INFO" {
        return None;
    }

    let player_name = clean_name(parts[4])?;
    let class_id = parts[6].parse::<u32>().ok()?;
    let diamond_gem_power = parse_gem_power(parts[10], gem_color_indices, "diamond");

    Some(CombatantInfo {
        player_name,
        class_id,
        diamond_gem_power,
    })
}

fn parse_player_name(line: &str) -> Option<String> {
    let parts = line.split('|').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }

    if parts[2].starts_with("Player-") {
        return clean_name(parts[3]);
    }

    None
}

fn parse_relic_trigger(
    line: &str,
    relics_by_ability: &HashMap<u32, RelicMeta>,
) -> Option<(String, RelicMeta, u64)> {
    parse_activation(line, relics_by_ability)
        .or_else(|| parse_effect_trigger(line, relics_by_ability))
}

fn parse_activation(
    line: &str,
    relics_by_ability: &HashMap<u32, RelicMeta>,
) -> Option<(String, RelicMeta, u64)> {
    let parts = line.split('|').collect::<Vec<_>>();
    if parts.len() < 6 || parts[1] != "ABILITY_ACTIVATED" {
        return None;
    }

    let player = clean_name(parts[3])?;
    let ability_id = parts[4].parse::<u32>().ok()?;
    let relic = relics_by_ability.get(&ability_id)?.clone();
    let started_at_ms = parse_timestamp_ms(line).unwrap_or_else(now_ms);

    Some((player, relic, started_at_ms))
}

fn parse_effect_trigger(
    line: &str,
    relics_by_ability: &HashMap<u32, RelicMeta>,
) -> Option<(String, RelicMeta, u64)> {
    let parts = line.split('|').collect::<Vec<_>>();
    let event_type = parts.get(1).copied()?;
    if event_type != "EFFECT_APPLIED" && event_type != "EFFECT_REFRESHED" {
        return None;
    }

    let player = clean_name(parts.get(3).copied()?)?;
    let started_at_ms = parse_timestamp_ms(line).unwrap_or_else(now_ms);

    for token_index in (parts.len().saturating_sub(6)..parts.len()).rev() {
        let Ok(ability_id) = parts[token_index].parse::<u32>() else {
            continue;
        };

        if let Some(relic) = relics_by_ability.get(&ability_id) {
            return Some((player, relic.clone(), started_at_ms));
        }
    }

    None
}

fn parse_equipped_relics(
    line: &str,
    relics_by_ability: &HashMap<u32, RelicMeta>,
) -> Vec<RelicMeta> {
    let parts = line.split('|').collect::<Vec<_>>();
    let Some(equipment_section) = parts.get(11) else {
        return Vec::new();
    };

    let mut seen = BTreeSet::new();
    let mut relics = Vec::new();

    for id in extract_equipped_item_ids(equipment_section) {
        let Some(relic) = relics_by_ability.get(&id) else {
            continue;
        };

        if seen.insert(relic.id) {
            relics.push(relic.clone());
        }
    }

    relics
}

fn parse_gem_power(
    gem_section: &str,
    gem_color_indices: &HashMap<String, usize>,
    color_name: &str,
) -> u32 {
    let Some(gem_index) = gem_color_indices.get(color_name).copied() else {
        return 0;
    };

    let trimmed = gem_section.trim().trim_start_matches('[').trim_end_matches(']');
    trimmed
        .split(',')
        .nth(gem_index)
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

fn extract_equipped_item_ids(equipment_section: &str) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut tuple_depth = 0usize;
    let mut current_number = String::new();
    let mut capturing_first_number = false;

    for ch in equipment_section.chars() {
        match ch {
            '(' => {
                tuple_depth += 1;
                if tuple_depth == 1 {
                    current_number.clear();
                    capturing_first_number = true;
                }
            }
            ')' => {
                if tuple_depth == 1 && capturing_first_number && !current_number.is_empty() {
                    if let Ok(id) = current_number.parse::<u32>() {
                        ids.push(id);
                    }
                }
                capturing_first_number = false;
                current_number.clear();
                tuple_depth = tuple_depth.saturating_sub(1);
            }
            ',' => {
                if tuple_depth == 1 && capturing_first_number {
                    if let Ok(id) = current_number.parse::<u32>() {
                        ids.push(id);
                    }
                    current_number.clear();
                    capturing_first_number = false;
                }
            }
            digit if digit.is_ascii_digit() && tuple_depth == 1 && capturing_first_number => {
                current_number.push(digit);
            }
            _ => {}
        }
    }

    ids
}

fn clean_name(value: &str) -> Option<String> {
    let cleaned = value.trim().trim_matches('"').to_string();
    if cleaned.is_empty()
        || cleaned.starts_with("Player-")
        || cleaned.starts_with("Npc-")
        || cleaned.starts_with("UnrecognizedType-")
    {
        None
    } else {
        Some(cleaned)
    }
}

fn is_dungeon_start(line: &str) -> bool {
    line.split('|').nth(1) == Some("DUNGEON_START")
}

fn parse_timestamp_ms(line: &str) -> Option<u64> {
    let timestamp = line.split('|').next()?;
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|date_time| date_time.timestamp_millis() as u64)
}

fn prune_expired_cooldowns(active_cooldowns: &mut HashMap<String, ActiveCooldown>) {
    let now = now_ms();
    active_cooldowns.retain(|_, cooldown| {
        let elapsed_seconds = ((now.saturating_sub(cooldown.started_at_ms)) / 1000) as u32;
        elapsed_seconds < cooldown.duration_seconds
    });
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn relic_cooldown_multiplier(class_id: u32, diamond_gem_power: u32) -> f64 {
    if class_id != 10 {
        return 1.0;
    }

    if diamond_gem_power >= 2640 {
        0.76
    } else if diamond_gem_power >= 960 {
        0.92
    } else {
        1.0
    }
}

fn adjusted_relic_cooldown(base_cooldown: u32, multiplier: f64) -> u32 {
    ((base_cooldown as f64) * multiplier).round().max(1.0) as u32
}

fn load_gem_color_indices() -> Result<HashMap<String, usize>, String> {
    let catalog: RawGemCatalog =
        serde_json::from_str(include_str!("gem_colors.json")).map_err(|e| e.to_string())?;

    Ok(catalog
        .colors
        .into_iter()
        .map(|(name, entry)| {
            let _ = entry.label;
            (name, entry.id)
        })
        .collect())
}

fn class_color(class_id: u32) -> &'static str {
    match class_id {
        22 => "#b46831",
        13 => "#28e05c",
        25 => "#077365",
        24 => "#fc9fec",
        14 => "#ea4f84",
        20 => "#dddbc5",
        11 => "#965a90",
        10 => "#527af5",
        7 => "#eb6328",
        2 => "#935dff",
        17 => "#1ea3ee",
        _ => "#f1d4a1",
    }
}

fn load_relic_catalog() -> Result<(HashMap<u32, RelicMeta>, HashMap<u32, RelicMeta>), String> {
    let raw: RawCatalog =
        serde_json::from_str(include_str!("relics.json")).map_err(|e| e.to_string())?;
    let icons_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons");

    let relics = raw
        .relics
        .into_iter()
        .filter_map(|(id, relic)| {
            let parsed_id = id.parse::<u32>().ok()?;
            Some((
                parsed_id,
                RelicMeta {
                    id: parsed_id,
                    name: relic.name,
                    base_cooldown: relic.base_cooldown,
                    icon_src: load_icon_src(&icons_dir.join(relic.icon)),
                },
            ))
        })
        .collect::<HashMap<_, _>>();

    let mut relics_by_item_id = HashMap::new();
    for (item_id, relic_id) in raw.item_mapping {
        let Ok(parsed_item_id) = item_id.parse::<u32>() else {
            continue;
        };

        if let Some(relic) = relics.get(&relic_id) {
            relics_by_item_id.insert(parsed_item_id, relic.clone());
        }
    }

    Ok((relics, relics_by_item_id))
}

fn load_relics_by_activation() -> Result<HashMap<u32, RelicMeta>, String> {
    let (relics_by_activation, _) = load_relic_catalog()?;
    Ok(relics_by_activation)
}

fn load_relics_by_item_id() -> Result<HashMap<u32, RelicMeta>, String> {
    let (_, relics_by_item_id) = load_relic_catalog()?;
    Ok(relics_by_item_id)
}

fn load_icon_src(path: &Path) -> String {
    let mime = match path.extension().and_then(|ext| ext.to_str()).unwrap_or_default() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => "application/octet-stream",
    };

    match fs::read(path) {
        Ok(bytes) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            format!("data:{mime};base64,{encoded}")
        }
        Err(_) => String::new(),
    }
}

pub fn resolve_latest_log_path(input_path: &str) -> Result<PathBuf, String> {
    let requested_path = Path::new(input_path);

    if requested_path.is_dir() {
        return latest_log_in_dir(requested_path);
    }

    if requested_path.exists() {
        if let Some(parent) = requested_path.parent() {
            if let Ok(latest) = latest_log_in_dir(parent) {
                return Ok(latest);
            }
        }

        return Ok(requested_path.to_path_buf());
    }

    if let Some(parent) = requested_path.parent() {
        if parent.exists() {
            return latest_log_in_dir(parent);
        }
    }

    latest_log_in_dir(Path::new(DEFAULT_LOG_DIR))
}

fn latest_log_in_dir(dir: &Path) -> Result<PathBuf, String> {
    let mut latest: Option<(SystemTime, PathBuf)> = None;

    let entries = fs::read_dir(dir).map_err(|e| format!("open error: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir error: {e}"))?;
        let path = entry.path();

        if !path.is_file() || !is_combat_log_file(&path) {
            continue;
        }

        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map_err(|e| format!("meta error: {e}"))?;

        match &latest {
            Some((current_modified, current_path)) => {
                if modified > *current_modified
                    || (modified == *current_modified && path > *current_path)
                {
                    latest = Some((modified, path));
                }
            }
            None => latest = Some((modified, path)),
        }
    }

    latest
        .map(|(_, path)| path)
        .ok_or_else(|| format!("No CombatLog*.txt files found in {}", dir.display()))
}

fn is_combat_log_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    file_name.starts_with("CombatLog") && file_name.ends_with(".txt")
}

pub fn show_main_window<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        detach_main_window(&window);
        let _ = window.set_title("Fellowship Overlay");

        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
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
        let cleaned_style = ex_style
            & !(WS_EX_TOOLWINDOW.0 as isize)
            & !(WS_EX_NOACTIVATE.0 as isize)
            | (WS_EX_APPWINDOW.0 as isize);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, cleaned_style);
    }

    let _ = window.set_always_on_top(false);
    let _ = window.set_skip_taskbar(false);
}

#[cfg(not(target_os = "windows"))]
fn detach_main_window<R: tauri::Runtime>(_: &tauri::WebviewWindow<R>) {}

#[cfg(target_os = "windows")]
fn sync_overlay_visibility<R: tauri::Runtime>(app: &AppHandle<R>, runtime: &mut OverlayRuntime) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else { return; };

    let is_visible = window.is_visible().unwrap_or(false);
    let is_minimized = window.is_minimized().unwrap_or(false);

    // Если оверлей отключен — скрываем и сбрасываем счетчик
    if !runtime.overlay_enabled {
        runtime.overlay_visibility_misses = 0;
        if is_visible {
            let _ = window.hide();
        }
        return;
    }

    // Если окно свернуто пользователем — не показываем
    if runtime.manually_minimized || is_minimized {
        return;
    }

    // Проверяем можно ли показывать оверлей (игра на переднем плане)
    if is_overlay_allowed_foreground() {
        runtime.overlay_visibility_misses = 0;
        if !is_visible {
            let _ = window.show();
        }
    } else {
        runtime.overlay_visibility_misses = runtime.overlay_visibility_misses.saturating_add(1);
        if runtime.overlay_visibility_misses >= 1 && is_visible {
            let _ = window.hide();
        }
    }
}


#[cfg(not(target_os = "windows"))]
fn sync_overlay_visibility<R: tauri::Runtime>(_: &AppHandle<R>, _: &mut OverlayRuntime) {}

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
    GAME_PROCESS_NAMES.iter().any(|game_name| normalized_name == *game_name)
        || normalized_name == "fellowship-overlay.exe"
}

#[cfg(target_os = "windows")]
fn process_name_from_pid(process_id: u32) -> Result<String, ()> {
    unsafe {
        let process =
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).map_err(|_| ())?;

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
        let file_name = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(())?;

        Ok(file_name.to_string())
    }
}
