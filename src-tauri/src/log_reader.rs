use crate::app_settings::{load_app_settings, save_app_settings, AppSettingsStore};
use base64::Engine;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri::State;
use tokio::task;
use tokio::time::{sleep, Duration};

pub const OVERLAY_STATE_EVENT: &str = "overlay://state";
const OVERLAY_MONITOR_INTERVAL_MS: u64 = 100;
const OVERLAY_MONITOR_STARTUP_DELAY_MS: u64 = 0;

#[derive(Default)]
pub struct ReaderCursor {
    pub offset: u64,
    pub remainder: String,
    pub path: Option<String>,
}

#[derive(Clone)]
pub struct RelicMeta {
    pub id: u32,
    pub name: String,
    pub base_cooldown: u32,
    pub icon_src: String,
}

pub struct ActiveCooldown {
    pub key: String,
    pub player: String,
    pub relic_id: u32,
    pub relic_name: String,
    pub relic_icon_src: String,
    pub duration_seconds: u32,
    pub started_at_ms: u64,
}

struct CombatantInfo {
    player_name: String,
    class_id: u32,
    diamond_gem_power: u32,
    sapphire_gem_power: u32,
    spirit_percent: f64,
}

#[derive(Clone)]
pub struct SpiritState {
    pub current: f64,
    pub max: f64,
    pub updated_at_ms: u64,
}

#[derive(Clone)]
pub struct SpiritResourceMeta {
    pub resource_id: u32,
    pub class_name: String,
    pub label: String,
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
    pub spirit_label: Option<String>,
    pub spirit_current: Option<u32>,
    pub spirit_max: Option<u32>,
    pub spirit_progress: Option<f64>,
    pub spirit_ready_at: Option<u32>,
    pub cooldowns: Vec<CooldownView>,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct OverlaySnapshot {
    pub configured_log_dir: String,
    pub resolved_path: String,
    pub overlay_enabled: bool,
    pub dungeon_active: bool,
    pub processed_line_count: usize,
    pub players: Vec<PlayerOverlay>,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct OverlayClientState {
    pub snapshot: Option<OverlaySnapshot>,
    pub status: String,
    pub error: Option<String>,
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

#[derive(Deserialize)]
struct RawSpiritResourceEntry {
    class_id: u32,
    class_name: String,
    resource_id: u32,
    label: String,
}

#[derive(Deserialize)]
struct RawSpiritResourceCatalog {
    resources: Vec<RawSpiritResourceEntry>,
}

pub struct OverlayRuntime {
    pub configured_log_dir: Option<String>,
    pub cursor: ReaderCursor,
    pub overlay_enabled: bool,
    pub dungeon_active: bool,
    pub players: BTreeSet<String>,
    pub player_classes: HashMap<String, u32>,
    pub player_relic_cdr: HashMap<String, f64>,
    pub player_spirit_regen_per_second: HashMap<String, f64>,
    pub player_spirit: HashMap<String, SpiritState>,
    pub player_spirit_caps: HashMap<String, u32>,
    pub player_spirit_ready_at: HashMap<String, u32>,
    pub equipped_relics: HashMap<String, BTreeMap<u32, RelicMeta>>,
    pub active_cooldowns: HashMap<String, ActiveCooldown>,
    pub processed_line_count: usize,
    pub overlay_visibility_misses: u8,
    pub gem_color_indices: HashMap<String, usize>,
    pub spirit_resources_by_class: HashMap<u32, SpiritResourceMeta>,
    pub relics_by_activation: HashMap<u32, RelicMeta>,
    pub relics_by_item_id: HashMap<u32, RelicMeta>,
    pub manually_minimized: bool,
}

pub struct OverlayState {
    pub runtime: Mutex<OverlayRuntime>,
    pub last_client_state: Mutex<OverlayClientState>,
    pub monitor_started: Mutex<bool>,
}

impl OverlayState {
    pub fn new() -> Self {
        Self {
            runtime: Mutex::new(OverlayRuntime {
                cursor: ReaderCursor::default(),
                configured_log_dir: None,
                overlay_enabled: true,
                dungeon_active: false,
                players: BTreeSet::new(),
                player_classes: HashMap::new(),
                player_relic_cdr: HashMap::new(),
                player_spirit_regen_per_second: HashMap::new(),
                player_spirit: HashMap::new(),
                player_spirit_caps: HashMap::new(),
                player_spirit_ready_at: HashMap::new(),
                equipped_relics: HashMap::new(),
                active_cooldowns: HashMap::new(),
                processed_line_count: 0,
                overlay_visibility_misses: 0,
                gem_color_indices: load_gem_color_indices().unwrap_or_default(),
                spirit_resources_by_class: load_spirit_resources_by_class().unwrap_or_default(),
                relics_by_activation: load_relics_by_activation().unwrap_or_default(),
                relics_by_item_id: load_relics_by_item_id().unwrap_or_default(),
                manually_minimized: false,
            }),
            last_client_state: Mutex::new(OverlayClientState {
                snapshot: None,
                status: "idle".to_string(),
                error: None,
            }),
            monitor_started: Mutex::new(false),
        }
    }
}

#[tauri::command]
pub fn get_overlay_state(
    state: State<'_, Arc<OverlayState>>,
    app: AppHandle,
) -> Result<OverlayClientState, String> {
    ensure_configured_log_dir_loaded(&app, state.inner());
    if has_configured_log_dir(state.inner()) {
        ensure_overlay_monitor_started(&app, state.inner());
    }

    let cached = state
        .last_client_state
        .lock()
        .map_err(|e| e.to_string())?
        .clone();

    if cached.snapshot.is_some() || cached.error.is_some() || cached.status != "idle" {
        return Ok(cached);
    }

    let next_state = client_state_from_runtime(state.inner(), None);
    store_client_state(state.inner(), &next_state);
    Ok(next_state)
}

#[tauri::command]
pub fn minimize_overlay(
    window: WebviewWindow,
    state: State<'_, Arc<OverlayState>>,
) -> Result<(), String> {
    if let Ok(mut runtime) = state.runtime.lock() {
        runtime.manually_minimized = true;
    }

    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_overlay_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_main_menu(app: AppHandle) -> Result<(), String> {
    crate::window_position::show_main_window(&app);
    Ok(())
}

#[tauri::command]
pub fn set_overlay_enabled(
    enabled: bool,
    state: State<'_, Arc<OverlayState>>,
    app: AppHandle,
) -> Result<OverlayClientState, String> {
    ensure_configured_log_dir_loaded(&app, state.inner());

    {
        let mut runtime = state.runtime.lock().map_err(|e| e.to_string())?;
        runtime.overlay_enabled = enabled;
        if enabled {
            runtime.manually_minimized = false;
            runtime.overlay_visibility_misses = 0;
        }
    }

    if has_configured_log_dir(state.inner()) {
        ensure_overlay_monitor_started(&app, state.inner());
    }

    if !enabled {
        if let Some(window) = app.get_webview_window(crate::constants::OVERLAY_WINDOW_LABEL) {
            let _ = window.hide();
        }
    }

    let next_state = client_state_from_runtime(state.inner(), None);
    let changed = store_client_state(state.inner(), &next_state);
    if changed {
        let _ = app.emit(OVERLAY_STATE_EVENT, &next_state);
    }

    Ok(next_state)
}

#[tauri::command]
pub fn choose_log_directory(
    state: State<'_, Arc<OverlayState>>,
    app: AppHandle,
) -> Result<OverlayClientState, String> {
    let current_directory = state
        .runtime
        .lock()
        .ok()
        .and_then(|runtime| runtime.configured_log_dir.clone());

    let mut dialog = FileDialog::new().set_title("Select the folder that contains CombatLog*.txt");
    if let Some(current_directory) = current_directory.as_deref() {
        dialog = dialog.set_directory(current_directory);
    }

    let selected_directory = dialog.pick_folder();
    let Some(selected_directory) = selected_directory else {
        return Ok(client_state_from_runtime(state.inner(), None));
    };

    apply_log_directory_change(
        Some(selected_directory.to_string_lossy().to_string()),
        state.inner(),
        &app,
    )
}

#[tauri::command]
pub fn set_log_directory(
    path: String,
    state: State<'_, Arc<OverlayState>>,
    app: AppHandle,
) -> Result<OverlayClientState, String> {
    apply_log_directory_change(Some(path), state.inner(), &app)
}

fn ensure_overlay_monitor_started<R: tauri::Runtime>(app: &AppHandle<R>, state: &Arc<OverlayState>) {
    let Ok(mut started) = state.monitor_started.lock() else {
        return;
    };

    if *started {
        return;
    }

    *started = true;
    start_overlay_monitor(app.clone(), state.clone());
}

fn ensure_configured_log_dir_loaded<R: tauri::Runtime>(app: &AppHandle<R>, state: &Arc<OverlayState>) {
    let Ok(mut runtime) = state.runtime.lock() else {
        return;
    };

    if runtime.configured_log_dir.is_some() {
        return;
    }

    runtime.configured_log_dir = load_app_settings(app)
        .log_directory
        .and_then(|value| normalize_log_directory(&value));
}

fn has_configured_log_dir(state: &Arc<OverlayState>) -> bool {
    state
        .runtime
        .lock()
        .ok()
        .and_then(|runtime| runtime.configured_log_dir.clone())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

pub fn start_overlay_monitor<R: tauri::Runtime>(app: AppHandle<R>, state: Arc<OverlayState>) {
    tauri::async_runtime::spawn(async move {
        sleep(Duration::from_millis(OVERLAY_MONITOR_STARTUP_DELAY_MS)).await;

        loop {
            let app_clone = app.clone();
            let state_clone = state.clone();
            let next_state = match task::spawn_blocking(move || compute_client_state(false, &state_clone, &app_clone))
            .await
            {
                Ok(next_state) => next_state,
                Err(error) => client_state_from_runtime(&state, Some(error.to_string())),
            };

            if let Some(snapshot) = next_state.snapshot.as_ref() {
                crate::window_position::resize_overlay_window(&app, snapshot);
                if let Ok(mut runtime) = state.runtime.lock() {
                    crate::window_position::sync_overlay_visibility(&app, &mut runtime);
                }
            }

            if store_client_state(&state, &next_state) {
                let _ = app.emit(OVERLAY_STATE_EVENT, &next_state);
            }

            sleep(Duration::from_millis(OVERLAY_MONITOR_INTERVAL_MS)).await;
        }
    });
}

fn compute_client_state<R: tauri::Runtime>(
    manage_window: bool,
    state: &Arc<OverlayState>,
    app: &AppHandle<R>,
) -> OverlayClientState {
    let configured_log_dir = state
        .runtime
        .lock()
        .ok()
        .and_then(|runtime| runtime.configured_log_dir.clone())
        .unwrap_or_default();

    if configured_log_dir.trim().is_empty() {
        return client_state_from_runtime(state, None);
    }

    match refresh_overlay_snapshot(&configured_log_dir, manage_window, state, app) {
        Ok(snapshot) => OverlayClientState {
            snapshot: Some(snapshot),
            status: "watching".to_string(),
            error: None,
        },
        Err(error) if error.contains("No CombatLog*.txt files found") => {
            client_state_from_runtime(state, None)
        }
        Err(error) => client_state_from_runtime(state, Some(error)),
    }
}

fn client_state_from_runtime(
    state: &Arc<OverlayState>,
    error: Option<String>,
) -> OverlayClientState {
    let runtime = match state.runtime.lock() {
        Ok(runtime) => runtime,
        Err(_) => {
            return OverlayClientState {
                snapshot: None,
                status: "error".to_string(),
                error: Some("Failed to lock overlay runtime.".to_string()),
            };
        }
    };

    let snapshot = build_snapshot(
        &runtime,
        runtime.configured_log_dir.clone().unwrap_or_default(),
        current_resolved_path(&runtime),
    );
    OverlayClientState {
        status: if error.is_some() {
            "error".to_string()
        } else if snapshot.configured_log_dir.is_empty() {
            "idle".to_string()
        } else if snapshot.resolved_path.is_empty() {
            "loading".to_string()
        } else {
            "watching".to_string()
        },
        snapshot: Some(snapshot),
        error,
    }
}

fn store_client_state(state: &Arc<OverlayState>, next_state: &OverlayClientState) -> bool {
    let Ok(mut last_client_state) = state.last_client_state.lock() else {
        return true;
    };

    if client_states_equivalent_for_emit(&last_client_state, next_state) {
        return false;
    }

    *last_client_state = next_state.clone();
    true
}

fn client_states_equivalent_for_emit(
    left: &OverlayClientState,
    right: &OverlayClientState,
) -> bool {
    if left.status != right.status || left.error != right.error {
        return false;
    }

    snapshots_equivalent_for_emit(left.snapshot.as_ref(), right.snapshot.as_ref())
}

fn snapshots_equivalent_for_emit(
    left: Option<&OverlaySnapshot>,
    right: Option<&OverlaySnapshot>,
) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return left.is_none() && right.is_none();
    };

    if left.configured_log_dir != right.configured_log_dir
        || left.resolved_path != right.resolved_path
        || left.overlay_enabled != right.overlay_enabled
        || left.dungeon_active != right.dungeon_active
        || left.players.len() != right.players.len()
    {
        return false;
    }

    for (left_player, right_player) in left.players.iter().zip(right.players.iter()) {
        if !players_equivalent_for_emit(left_player, right_player) {
            return false;
        }
    }

    true
}

fn players_equivalent_for_emit(left: &PlayerOverlay, right: &PlayerOverlay) -> bool {
    if left.name != right.name
        || left.class_id != right.class_id
        || left.class_color != right.class_color
        || left.spirit_label != right.spirit_label
        || left.spirit_current != right.spirit_current
        || left.spirit_max != right.spirit_max
        || left.spirit_progress != right.spirit_progress
        || left.spirit_ready_at != right.spirit_ready_at
        || left.cooldowns.len() != right.cooldowns.len()
    {
        return false;
    }

    for (left_cooldown, right_cooldown) in left.cooldowns.iter().zip(right.cooldowns.iter()) {
        if !cooldowns_equivalent_for_emit(left_cooldown, right_cooldown) {
            return false;
        }
    }

    true
}

fn cooldowns_equivalent_for_emit(left: &CooldownView, right: &CooldownView) -> bool {
    left.key == right.key
        && left.relic_id == right.relic_id
        && left.relic_name == right.relic_name
        && left.relic_icon_src == right.relic_icon_src
        && left.duration_seconds == right.duration_seconds
        && left.remaining_seconds == right.remaining_seconds
        && left.ready == right.ready
}

pub fn refresh_overlay_snapshot<R: tauri::Runtime>(
    path: &str,
    manage_window: bool,
    state: &Arc<OverlayState>,
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
        runtime.manually_minimized = false;
    }

    let snapshot = build_snapshot(
        &runtime,
        runtime.configured_log_dir.clone().unwrap_or_default(),
        resolved_path_string,
    );
    if manage_window {
        crate::window_position::resize_overlay_window(app, &snapshot);
        crate::window_position::sync_overlay_visibility(app, &mut runtime);
    }

    Ok(snapshot)
}

pub fn apply_lines_to_runtime(runtime: &mut OverlayRuntime, lines: &[String]) -> bool {
    let mut dungeon_started_now = false;
    let mut combatant_snapshot: BTreeSet<String> = BTreeSet::new();
    let mut class_snapshot: HashMap<String, u32> = HashMap::new();
    let mut relic_cdr_snapshot: HashMap<String, f64> = HashMap::new();
    let mut equipped_snapshot: HashMap<String, BTreeMap<u32, RelicMeta>> = HashMap::new();

    for line in lines {
        runtime.processed_line_count += 1;

        if is_dungeon_start(line) {
            runtime.dungeon_active = true;
            runtime.players.clear();
            runtime.player_classes.clear();
            runtime.player_relic_cdr.clear();
            runtime.player_spirit_regen_per_second.clear();
            runtime.player_spirit.clear();
            runtime.player_spirit_caps.clear();
            runtime.player_spirit_ready_at.clear();
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
            runtime.players.insert(player_name.clone());
            runtime
                .player_classes
                .insert(player_name.clone(), combatant_info.class_id);
            relic_cdr_snapshot.insert(
                player_name.clone(),
                relic_cooldown_multiplier(
                    combatant_info.class_id,
                    combatant_info.diamond_gem_power,
                ),
            );
            runtime.player_spirit_regen_per_second.insert(
                player_name.clone(),
                resolve_spirit_regen_per_second(combatant_info.spirit_percent),
            );
            if let Some(spirit_cap) = resolve_spirit_cap(
                combatant_info.class_id,
                combatant_info.sapphire_gem_power,
                &runtime.spirit_resources_by_class,
            ) {
                runtime.player_spirit_caps.insert(player_name.clone(), spirit_cap);
            }
            runtime.player_spirit_ready_at.insert(
                player_name.clone(),
                resolve_spirit_ready_threshold(combatant_info.sapphire_gem_power),
            );
            if let Some(initial_spirit) = initial_spirit_state(
                &player_name,
                combatant_info.class_id,
                &runtime.player_spirit_caps,
                &runtime.spirit_resources_by_class,
            ) {
                runtime
                    .player_spirit
                    .entry(player_name.clone())
                    .or_insert(initial_spirit);
            }

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

        if let Some((player_name, spirit_state)) =
            parse_spirit_resource_change(
                line,
                &runtime.player_classes,
                &runtime.spirit_resources_by_class,
                &runtime.player_spirit_caps,
            )
        {
            apply_spirit_update(runtime, player_name, spirit_state);
        }

        for (player_name, spirit_state) in parse_spirit_snapshots_from_event(
            line,
            &runtime.player_classes,
            &runtime.spirit_resources_by_class,
            &runtime.player_spirit_caps,
        ) {
            apply_spirit_update(runtime, player_name, spirit_state);
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
            .player_spirit_regen_per_second
            .retain(|player_name, _| current_players.contains(player_name));
        runtime
            .player_spirit_caps
            .retain(|player_name, _| current_players.contains(player_name));
        runtime
            .player_spirit_ready_at
            .retain(|player_name, _| current_players.contains(player_name));
        runtime
            .player_spirit
            .retain(|player_name, _| current_players.contains(player_name));
        runtime
            .active_cooldowns
            .retain(|_, cooldown| current_players.contains(&cooldown.player));
    }

    prune_expired_cooldowns(&mut runtime.active_cooldowns);

    dungeon_started_now
}

pub fn build_snapshot(
    runtime: &OverlayRuntime,
    configured_log_dir: String,
    resolved_path: String,
) -> OverlaySnapshot {
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

            cooldowns.sort_by(|left, right| {
                let left_order = if left.ready { 1 } else { 0 };
                let right_order = if right.ready { 1 } else { 0 };
                left_order
                    .cmp(&right_order)
                    .then(left.remaining_seconds.cmp(&right.remaining_seconds))
                    .then(left.relic_name.cmp(&right.relic_name))
            });

            PlayerOverlay {
                name: name.clone(),
                class_id: runtime.player_classes.get(name).copied(),
                class_color: runtime
                    .player_classes
                    .get(name)
                    .map(|class_id| class_color(*class_id).to_string())
                    .unwrap_or_else(|| "#f1d4a1".to_string()),
                spirit_label: runtime
                    .player_classes
                    .get(name)
                    .and_then(|class_id| runtime.spirit_resources_by_class.get(class_id))
                    .map(|resource| resource.label.clone()),
                spirit_current: simulated_spirit_state(
                    runtime.player_spirit.get(name),
                    runtime.player_spirit_regen_per_second.get(name).copied(),
                    now,
                )
                .map(|spirit| spirit.current.round().max(0.0) as u32),
                spirit_max: simulated_spirit_state(
                    runtime.player_spirit.get(name),
                    runtime.player_spirit_regen_per_second.get(name).copied(),
                    now,
                )
                .map(|spirit| spirit.max.round().max(0.0) as u32),
                spirit_progress: simulated_spirit_state(
                    runtime.player_spirit.get(name),
                    runtime.player_spirit_regen_per_second.get(name).copied(),
                    now,
                )
                .and_then(|spirit| {
                    if spirit.max <= 0.0 {
                        None
                    } else {
                        Some((spirit.current / spirit.max).clamp(0.0, 1.0))
                    }
                }),
                spirit_ready_at: runtime.player_spirit_ready_at.get(name).copied(),
                cooldowns,
            }
        })
        .collect::<Vec<_>>();

    players.sort_by(|left, right| left.name.cmp(&right.name));

    OverlaySnapshot {
        configured_log_dir,
        resolved_path,
        overlay_enabled: runtime.overlay_enabled,
        dungeon_active: runtime.dungeon_active,
        processed_line_count: runtime.processed_line_count,
        players,
    }
}

fn reset_runtime_for_log_directory_change(runtime: &mut OverlayRuntime) {
    runtime.cursor = ReaderCursor::default();
    runtime.dungeon_active = false;
    runtime.players.clear();
    runtime.player_classes.clear();
    runtime.player_relic_cdr.clear();
    runtime.player_spirit_regen_per_second.clear();
    runtime.player_spirit.clear();
    runtime.player_spirit_caps.clear();
    runtime.player_spirit_ready_at.clear();
    runtime.equipped_relics.clear();
    runtime.active_cooldowns.clear();
    runtime.processed_line_count = 0;
    runtime.overlay_visibility_misses = 0;
    runtime.manually_minimized = false;
}

fn apply_log_directory_change<R: tauri::Runtime>(
    path: Option<String>,
    state: &Arc<OverlayState>,
    app: &AppHandle<R>,
) -> Result<OverlayClientState, String> {
    let normalized = path
        .as_deref()
        .and_then(normalize_log_directory);

    save_app_settings(
        app,
        &AppSettingsStore {
            log_directory: normalized.clone(),
        },
    );

    {
        let mut runtime = state.runtime.lock().map_err(|e| e.to_string())?;
        runtime.configured_log_dir = normalized;
        reset_runtime_for_log_directory_change(&mut runtime);
    }

    if has_configured_log_dir(state) {
        ensure_overlay_monitor_started(app, state);
    } else if let Some(window) = app.get_webview_window(crate::constants::OVERLAY_WINDOW_LABEL) {
        let _ = window.hide();
    }

    let next_state = client_state_from_runtime(state, None);
    let changed = store_client_state(state, &next_state);
    if changed {
        let _ = app.emit(OVERLAY_STATE_EVENT, &next_state);
    }
    Ok(next_state)
}

fn normalize_log_directory(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }

    let path = PathBuf::from(trimmed);
    if is_combat_log_file(&path) {
        return path.parent().map(|parent| parent.to_string_lossy().to_string());
    }

    Some(path.to_string_lossy().to_string())
}

pub fn bootstrap_runtime_from_log(
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
    runtime.player_spirit_regen_per_second.clear();
    runtime.player_spirit.clear();
    runtime.player_spirit_caps.clear();
    runtime.player_spirit_ready_at.clear();
    runtime.equipped_relics.clear();
    runtime.active_cooldowns.clear();
    runtime.processed_line_count = all_lines.len();

    let Some((start_index, explicit_dungeon_start)) = find_bootstrap_start_index(&all_lines) else {
        return Ok(());
    };

    let slice = all_lines[start_index..].to_vec();
    if !explicit_dungeon_start {
        runtime.dungeon_active = true;
    }
    apply_lines_to_runtime(runtime, &slice);
    runtime.processed_line_count = all_lines.len();
    Ok(())
}

fn find_bootstrap_start_index(lines: &[String]) -> Option<(usize, bool)> {
    if let Some(start_index) = lines.iter().rposition(|line| is_dungeon_start(line)) {
        return Some((start_index, true));
    }

    let last_combatant_index = lines
        .iter()
        .rposition(|line| line.split('|').nth(1) == Some("COMBATANT_INFO"))?;
    let mut block_start = last_combatant_index;
    while block_start > 0
        && lines[block_start - 1].split('|').nth(1) == Some("COMBATANT_INFO")
    {
        block_start -= 1;
    }

    Some((block_start, false))
}

pub fn read_new_lines(path: &Path, cursor: &mut ReaderCursor) -> Result<Vec<String>, String> {
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
    let spirit_percent = parse_spirit_percent(parts[8]).unwrap_or(0.0);
    let diamond_gem_power = parse_gem_power(parts[10], gem_color_indices, "diamond");
    let sapphire_gem_power = parse_gem_power(parts[10], gem_color_indices, "sapphire");

    Some(CombatantInfo {
        player_name,
        class_id,
        diamond_gem_power,
        sapphire_gem_power,
        spirit_percent,
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
    relics_by_activation: &HashMap<u32, RelicMeta>,
) -> Option<(String, RelicMeta, u64)> {
    parse_activation(line, relics_by_activation)
        .or_else(|| parse_effect_trigger(line, relics_by_activation))
}

fn parse_spirit_resource_change(
    line: &str,
    player_classes: &HashMap<String, u32>,
    spirit_resources_by_class: &HashMap<u32, SpiritResourceMeta>,
    player_spirit_caps: &HashMap<String, u32>,
) -> Option<(String, SpiritState)> {
    let parts = line.split('|').collect::<Vec<_>>();
    if parts.len() < 10 || parts[1] != "RESOURCE_CHANGED" {
        return None;
    }

    let player_name = clean_name(parts[3])?;
    let class_id = *player_classes.get(&player_name)?;
    let spirit_resource = spirit_resources_by_class.get(&class_id)?;
    let resource_id = parts[6].parse::<u32>().ok()?;
    if resource_id != spirit_resource.resource_id {
        return None;
    }

    let current = parts[8].replace(',', ".").parse::<f64>().ok()?;
    let _raw_max = parts[9].replace(',', ".").parse::<f64>().ok()?;
    let max = player_spirit_caps
        .get(&player_name)
        .copied()
        .map(|value| value as f64)
        .unwrap_or(100.0);

    Some((
        player_name,
        SpiritState {
            current: current.max(0.0),
            max: max.max(0.0),
            updated_at_ms: parse_timestamp_ms(line).unwrap_or_else(now_ms),
        },
    ))
}

fn parse_spirit_snapshots_from_event(
    line: &str,
    player_classes: &HashMap<String, u32>,
    spirit_resources_by_class: &HashMap<u32, SpiritResourceMeta>,
    player_spirit_caps: &HashMap<String, u32>,
) -> Vec<(String, SpiritState)> {
    let parts = line.split('|').collect::<Vec<_>>();
    if parts.len() < 6 {
        return Vec::new();
    }

    let event_type = parts[1];
    if event_type == "COMBATANT_INFO" || event_type == "RESOURCE_CHANGED" || event_type == "DUNGEON_START" {
        return Vec::new();
    }

    let source_player = if parts[2].starts_with("Player-") {
        clean_name(parts[3])
    } else {
        None
    };
    let target_player = if parts[4].starts_with("Player-") {
        clean_name(parts[5])
    } else {
        None
    };

    let resource_arrays = parts
        .iter()
        .enumerate()
        .filter_map(|(index, part)| is_resource_snapshot_field(part).then_some((index, *part)))
        .collect::<Vec<_>>();

    if resource_arrays.is_empty() {
        return Vec::new();
    }

    let mut snapshots = Vec::new();
    let snapshot_timestamp_ms = parse_timestamp_ms(line).unwrap_or_else(now_ms);

    let snapshot_count = resource_arrays.len();

    for (ordinal, (index, snapshot_field)) in resource_arrays.into_iter().enumerate() {
        let Some(owner) = resolve_snapshot_owner(
            event_type,
            index,
            ordinal,
            snapshot_count,
            source_player.as_ref(),
            target_player.as_ref(),
        ) else {
            continue;
        };

        if let Some(spirit_state) = parse_spirit_state_from_snapshot(
            snapshot_field,
            owner,
            player_classes,
            spirit_resources_by_class,
            player_spirit_caps,
            snapshot_timestamp_ms,
        ) {
            snapshots.push((owner.to_string(), spirit_state));
        }
    }

    snapshots
}

fn resolve_snapshot_owner<'a>(
    event_type: &str,
    snapshot_index: usize,
    snapshot_ordinal: usize,
    snapshot_count: usize,
    source_player: Option<&'a String>,
    target_player: Option<&'a String>,
) -> Option<&'a str> {
    match (source_player, target_player) {
        (Some(source_player), None) => (snapshot_ordinal == 0).then_some(source_player.as_str()),
        (None, Some(target_player)) => {
            (snapshot_ordinal + 1 == snapshot_count).then_some(target_player.as_str())
        }
        (Some(source_player), Some(target_player)) => {
            if snapshot_count >= 2 {
                if snapshot_ordinal == 0 {
                    Some(source_player.as_str())
                } else if snapshot_ordinal + 1 == snapshot_count {
                    Some(target_player.as_str())
                } else {
                    None
                }
            } else if event_type.starts_with("EFFECT_") {
                Some(target_player.as_str())
            } else if snapshot_index > 22 {
                Some(target_player.as_str())
            } else {
                Some(source_player.as_str())
            }
        }
        _ => None,
    }
}

fn parse_spirit_state_from_snapshot(
    snapshot_field: &str,
    player_name: &str,
    player_classes: &HashMap<String, u32>,
    spirit_resources_by_class: &HashMap<u32, SpiritResourceMeta>,
    player_spirit_caps: &HashMap<String, u32>,
    updated_at_ms: u64,
) -> Option<SpiritState> {
    let class_id = *player_classes.get(player_name)?;
    let spirit_resource = spirit_resources_by_class.get(&class_id)?;

    for (resource_id, current, raw_max) in extract_resource_triplets(snapshot_field) {
        if resource_id != spirit_resource.resource_id {
            continue;
        }

        let max = player_spirit_caps
            .get(player_name)
            .copied()
            .map(|value| value as f64)
            .unwrap_or(raw_max.max(100.0));

        return Some(SpiritState {
            current: current.max(0.0),
            max: max.max(0.0),
            updated_at_ms,
        });
    }

    None
}

fn apply_spirit_update(runtime: &mut OverlayRuntime, player_name: String, next_state: SpiritState) {
    let ready_at = runtime
        .player_spirit_ready_at
        .get(&player_name)
        .copied()
        .unwrap_or(100) as f64;

    let should_apply = should_accept_spirit_update(
        runtime.player_spirit.get(&player_name),
        &next_state,
        ready_at,
    );

    if should_apply {
        runtime.player_spirit.insert(player_name, next_state);
    }
}

fn simulated_spirit_state(
    base_state: Option<&SpiritState>,
    regen_per_second: Option<f64>,
    now_ms_value: u64,
) -> Option<SpiritState> {
    let base_state = base_state?;
    let regen_per_second = regen_per_second.unwrap_or(0.0).max(0.0);
    if regen_per_second <= 0.0 || base_state.updated_at_ms >= now_ms_value {
        return Some(base_state.clone());
    }

    let elapsed_seconds = (now_ms_value.saturating_sub(base_state.updated_at_ms)) as f64 / 1000.0;
    let current = (base_state.current + regen_per_second * elapsed_seconds).clamp(0.0, base_state.max);

    Some(SpiritState {
        current,
        max: base_state.max,
        updated_at_ms: now_ms_value,
    })
}

fn should_accept_spirit_update(
    current_state: Option<&SpiritState>,
    next_state: &SpiritState,
    ready_at: f64,
) -> bool {
    let Some(current_state) = current_state else {
        return true;
    };

    let current_value = current_state.current.max(0.0);
    let next_value = next_state.current.max(0.0);

    if next_value >= current_value {
        return true;
    }

    let spent_amount = current_value - next_value;
    let tolerance = 18.0;

    spent_amount >= (ready_at - tolerance) && spent_amount <= (ready_at + tolerance)
}

fn is_resource_snapshot_field(value: &&str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("[(") && trimmed.ends_with(")]")
}

fn extract_resource_triplets(snapshot_field: &str) -> Vec<(u32, f64, f64)> {
    let trimmed = snapshot_field.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut tuples = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;

    for ch in trimmed.chars() {
        match ch {
            '(' => {
                depth += 1;
                if depth == 1 {
                    current.clear();
                } else {
                    current.push(ch);
                }
            }
            ')' => {
                if depth == 1 {
                    let parts = current.split(',').map(str::trim).collect::<Vec<_>>();
                    if parts.len() == 3 {
                        if let (Ok(resource_id), Ok(current_value), Ok(max_value)) = (
                            parts[0].parse::<u32>(),
                            parts[1].replace(',', ".").parse::<f64>(),
                            parts[2].replace(',', ".").parse::<f64>(),
                        ) {
                            tuples.push((resource_id, current_value, max_value));
                        }
                    }
                    current.clear();
                } else if depth > 1 {
                    current.push(ch);
                }
                depth = depth.saturating_sub(1);
            }
            _ => {
                if depth >= 1 {
                    current.push(ch);
                }
            }
        }
    }

    tuples
}

fn parse_activation(
    line: &str,
    relics_by_activation: &HashMap<u32, RelicMeta>,
) -> Option<(String, RelicMeta, u64)> {
    let parts = line.split('|').collect::<Vec<_>>();
    if parts.len() < 6 || parts[1] != "ABILITY_ACTIVATED" {
        return None;
    }

    let player = clean_name(parts[3])?;
    let ability_id = parts[4].parse::<u32>().ok()?;
    let relic = relics_by_activation.get(&ability_id)?.clone();
    let started_at_ms = parse_timestamp_ms(line).unwrap_or_else(now_ms);

    Some((player, relic, started_at_ms))
}

fn parse_effect_trigger(
    line: &str,
    relics_by_activation: &HashMap<u32, RelicMeta>,
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

        if let Some(relic) = relics_by_activation.get(&ability_id) {
            return Some((player, relic.clone(), started_at_ms));
        }
    }

    None
}

fn parse_equipped_relics(
    line: &str,
    relics_by_item_id: &HashMap<u32, RelicMeta>,
) -> Vec<RelicMeta> {
    let parts = line.split('|').collect::<Vec<_>>();
    let Some(equipment_section) = parts.get(11) else {
        return Vec::new();
    };

    let mut seen = BTreeSet::new();
    let mut relics = Vec::new();

    for id in extract_equipped_item_ids(equipment_section) {
        let Some(relic) = relics_by_item_id.get(&id) else {
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

pub fn load_gem_color_indices() -> Result<HashMap<String, usize>, String> {
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

fn load_spirit_resources_by_class() -> Result<HashMap<u32, SpiritResourceMeta>, String> {
    let catalog: RawSpiritResourceCatalog =
        serde_json::from_str(include_str!("spirit_resources.json")).map_err(|e| e.to_string())?;

    Ok(catalog
        .resources
        .into_iter()
        .map(|entry| {
                (
                    entry.class_id,
                    SpiritResourceMeta {
                        resource_id: entry.resource_id,
                        class_name: entry.class_name,
                        label: entry.label,
                    },
                )
        })
        .collect())
}

fn initial_spirit_state(
    player_name: &str,
    class_id: u32,
    player_spirit_caps: &HashMap<String, u32>,
    spirit_resources_by_class: &HashMap<u32, SpiritResourceMeta>,
) -> Option<SpiritState> {
    spirit_resources_by_class.get(&class_id)?;
    let max = player_spirit_caps
        .get(player_name)
        .copied()
        .map(|value| value as f64)
        .unwrap_or(100.0);

    Some(SpiritState {
        current: 0.0,
        max,
        updated_at_ms: now_ms(),
    })
}

fn resolve_spirit_cap(
    class_id: u32,
    sapphire_gem_power: u32,
    spirit_resources_by_class: &HashMap<u32, SpiritResourceMeta>,
) -> Option<u32> {
    spirit_resources_by_class.get(&class_id)?;
    Some(100 + sapphire_spirit_bonus(sapphire_gem_power))
}

fn resolve_spirit_regen_per_second(spirit_percent: f64) -> f64 {
    0.29 + (spirit_percent.max(0.0) / 100.0)
}

fn parse_spirit_percent(stat_block: &str) -> Option<f64> {
    let trimmed = stat_block.trim().trim_start_matches('[').trim_end_matches(']');
    let parts = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
    let value = parts.last()?.replace(',', ".").parse::<f64>().ok()?;
    Some(value.max(0.0))
}

fn sapphire_spirit_bonus(sapphire_gem_power: u32) -> u32 {
    if sapphire_gem_power >= 1200 {
        30
    } else if sapphire_gem_power >= 120 {
        10
    } else {
        0
    }
}

fn resolve_spirit_ready_threshold(sapphire_gem_power: u32) -> u32 {
    if sapphire_gem_power >= 2640 {
        85
    } else if sapphire_gem_power >= 960 {
        95
    } else {
        100
    }
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
                    icon_src: load_icon_src(&relic.icon),
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

pub fn load_relics_by_activation() -> Result<HashMap<u32, RelicMeta>, String> {
    let (relics_by_activation, _) = load_relic_catalog()?;
    Ok(relics_by_activation)
}

pub fn load_relics_by_item_id() -> Result<HashMap<u32, RelicMeta>, String> {
    let (_, relics_by_item_id) = load_relic_catalog()?;
    Ok(relics_by_item_id)
}

fn load_icon_src(relative_path: &str) -> String {
    let Some((mime, bytes)) = embedded_icon_asset(relative_path) else {
        return String::new();
    };

    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{encoded}")
}

fn embedded_icon_asset(relative_path: &str) -> Option<(&'static str, &'static [u8])> {
    match relative_path.replace('\\', "/").as_str() {
        "icons_trink/Sanctuary.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Sanctuary.jpg"),
        )),
        "icons_trink/Restore_Mana.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Restore_Mana.jpg"),
        )),
        "icons_trink/Obsidian_Skin.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Obsidian_Skin.jpg"),
        )),
        "icons_trink/Chickenize.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Chickenize.jpg"),
        )),
        "icons_trink/Major_Dispel.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Major_Dispel.jpg"),
        )),
        "icons_trink/Bloodrite_Drums.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Bloodrite_Drums.jpg"),
        )),
        "icons_trink/Major_Invisibility.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Major_Invisibility.jpg"),
        )),
        "icons_trink/Conjure_Portal.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Conjure_Portal.jpg"),
        )),
        "icons_trink/Revive.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Revive.jpg"),
        )),
        "icons_trink/Rejuvenate.jpg" => Some((
            "image/jpeg",
            include_bytes!("../icons/icons_trink/Rejuvenate.jpg"),
        )),
        _ => None,
    }
}

fn current_resolved_path(runtime: &OverlayRuntime) -> String {
    runtime.cursor.path.clone().unwrap_or_default()
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
    Err(format!(
        "No CombatLog*.txt files found in {}",
        requested_path.display()
    ))
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
