import type { OverlayClientState } from "../store/overlay";

const OVERLAY_STATE_EVENT = "overlay://state";

async function coreInvoke<T>(command: string, args?: Record<string, unknown>) {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export function getDefaultLogPath() {
  return coreInvoke<string>("get_default_log_path");
}

export function setLogDirectory(path: string) {
  return coreInvoke<OverlayClientState>("set_log_directory", { path });
}

export function chooseLogDirectory() {
  return coreInvoke<OverlayClientState>("choose_log_directory");
}

export function getOverlayState() {
  return coreInvoke<OverlayClientState>("get_overlay_state");
}

export async function listenOverlayState(
  onState: (state: OverlayClientState) => void,
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<OverlayClientState>(OVERLAY_STATE_EVENT, (event) => {
    onState(event.payload);
  });
}

export function startOverlayDrag() {
  return coreInvoke("start_overlay_drag");
}

export function openOverlayMainMenu() {
  return coreInvoke("open_main_menu");
}

export function setOverlayEnabled(enabled: boolean) {
  return coreInvoke<OverlayClientState>("set_overlay_enabled", { enabled });
}
