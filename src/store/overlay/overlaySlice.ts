import { PayloadAction, createSlice } from "@reduxjs/toolkit";
import { OverlayClientState, OverlaySnapshot, OverlayStatus } from "./overlayTypes";

export type OverlayState = {
  snapshot: OverlaySnapshot | null;
  logPath: string;
  status: OverlayStatus;
  error: string | null;
};

const initialState: OverlayState = {
  snapshot: null,
  logPath: "",
  status: "loading",
  error: null,
};

const overlaySlice = createSlice({
  name: "overlay",
  initialState,
  reducers: {
    setLogPath(state, action: PayloadAction<string>) {
      state.logPath = action.payload;
    },
    setStatus(state, action: PayloadAction<OverlayStatus>) {
      state.status = action.payload;
    },
    setError(state, action: PayloadAction<string | null>) {
      state.error = action.payload;
    },
    setSnapshot(state, action: PayloadAction<OverlaySnapshot | null>) {
      state.snapshot = action.payload;
    },
    applyServerUpdate(state, action: PayloadAction<OverlayClientState>) {
      const nextSnapshot = action.payload.snapshot;
      const nextStatus = action.payload.status;
      const nextError = action.payload.error;

      if (
        state.status === nextStatus &&
        state.error === nextError &&
        snapshotsEqual(state.snapshot, nextSnapshot)
      ) {
        return;
      }

      state.status = nextStatus;
      state.error = nextError;
      state.snapshot = nextSnapshot;

      if (nextSnapshot?.resolved_path) {
        state.logPath = nextSnapshot.configured_log_dir || nextSnapshot.resolved_path;
      } else if (nextSnapshot?.configured_log_dir) {
        state.logPath = nextSnapshot.configured_log_dir;
      }
    },
  },
});

export const { applyServerUpdate, setError, setLogPath, setSnapshot, setStatus } =
  overlaySlice.actions;

export const overlayReducer = overlaySlice.reducer;

function snapshotsEqual(
  left: OverlaySnapshot | null,
  right: OverlaySnapshot | null,
): boolean {
  if (left === right) {
    return true;
  }

  if (left === null || right === null) {
    return false;
  }

  if (
    left.configured_log_dir !== right.configured_log_dir ||
    left.resolved_path !== right.resolved_path ||
    left.overlay_enabled !== right.overlay_enabled ||
    left.dungeon_active !== right.dungeon_active ||
    left.players.length !== right.players.length
  ) {
    return false;
  }

  for (let playerIndex = 0; playerIndex < left.players.length; playerIndex += 1) {
    const leftPlayer = left.players[playerIndex];
    const rightPlayer = right.players[playerIndex];

    if (
      leftPlayer.name !== rightPlayer.name ||
      leftPlayer.class_color !== rightPlayer.class_color ||
      leftPlayer.class_id !== rightPlayer.class_id ||
      leftPlayer.spirit_label !== rightPlayer.spirit_label ||
      leftPlayer.spirit_current !== rightPlayer.spirit_current ||
      leftPlayer.spirit_max !== rightPlayer.spirit_max ||
      leftPlayer.spirit_progress !== rightPlayer.spirit_progress ||
      leftPlayer.spirit_ready_at !== rightPlayer.spirit_ready_at ||
      leftPlayer.cooldowns.length !== rightPlayer.cooldowns.length
    ) {
      return false;
    }

    for (let cooldownIndex = 0; cooldownIndex < leftPlayer.cooldowns.length; cooldownIndex += 1) {
      const leftCooldown = leftPlayer.cooldowns[cooldownIndex];
      const rightCooldown = rightPlayer.cooldowns[cooldownIndex];

      if (
        leftCooldown.key !== rightCooldown.key ||
        leftCooldown.relic_id !== rightCooldown.relic_id ||
        leftCooldown.relic_name !== rightCooldown.relic_name ||
        leftCooldown.duration_seconds !== rightCooldown.duration_seconds ||
        leftCooldown.remaining_seconds !== rightCooldown.remaining_seconds ||
        leftCooldown.ready !== rightCooldown.ready ||
        leftCooldown.relic_icon_src !== rightCooldown.relic_icon_src
      ) {
        return false;
      }
    }
  }

  return true;
}
