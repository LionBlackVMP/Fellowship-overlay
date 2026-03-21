import { PayloadAction, createSlice } from "@reduxjs/toolkit";
import { OverlaySnapshot, OverlayStatus } from "./overlayTypes";

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
    applyPollingSuccess(state, action: PayloadAction<OverlaySnapshot>) {
      state.status = "watching";
      state.error = null;
      if (action.payload.resolved_path && action.payload.resolved_path !== state.logPath) {
        state.logPath = action.payload.resolved_path;
      }

      if (snapshotsEqual(state.snapshot, action.payload)) {
        return;
      }

      state.snapshot = action.payload;
    },
  },
});

export const { applyPollingSuccess, setError, setLogPath, setSnapshot, setStatus } =
  overlaySlice.actions;

export const overlayReducer = overlaySlice.reducer;

function snapshotsEqual(left: OverlaySnapshot | null, right: OverlaySnapshot): boolean {
  if (left === null) {
    return false;
  }

  if (
    left.resolved_path !== right.resolved_path ||
    left.overlay_enabled !== right.overlay_enabled ||
    left.dungeon_active !== right.dungeon_active ||
    left.processed_line_count !== right.processed_line_count ||
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
      leftPlayer.cooldowns.length !== rightPlayer.cooldowns.length
    ) {
      return false;
    }

    for (let cooldownIndex = 0; cooldownIndex < leftPlayer.cooldowns.length; cooldownIndex += 1) {
      const leftCooldown = leftPlayer.cooldowns[cooldownIndex];
      const rightCooldown = rightPlayer.cooldowns[cooldownIndex];

      if (
        leftCooldown.key !== rightCooldown.key ||
        leftCooldown.remaining_seconds !== rightCooldown.remaining_seconds ||
        leftCooldown.progress !== rightCooldown.progress ||
        leftCooldown.ready !== rightCooldown.ready ||
        leftCooldown.relic_icon_src !== rightCooldown.relic_icon_src
      ) {
        return false;
      }
    }
  }

  return true;
}
