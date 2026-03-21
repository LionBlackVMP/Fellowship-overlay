import { invoke } from "@tauri-apps/api/core";
import type { OverlaySnapshot } from "../store/overlay";

export function getDefaultLogPath() {
  return invoke<string>("get_default_log_path");
}

export function pollOverlayState(path: string) {
  return invoke<OverlaySnapshot>("poll_overlay_state", { path });
}

export function startOverlayDrag() {
  return invoke("start_overlay_drag");
}

export function openOverlayMainMenu() {
  return invoke("open_main_menu");
}

export function setOverlayEnabled(enabled: boolean) {
  return invoke("set_overlay_enabled", { enabled });
}
