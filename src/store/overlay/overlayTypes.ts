export type CooldownView = {
  key: string;
  relic_id: number;
  relic_name: string;
  relic_icon_src: string;
  duration_seconds: number;
  remaining_seconds: number;
  progress: number;
  ready: boolean;
};

export type PlayerOverlay = {
  name: string;
  class_id?: number | null;
  class_color: string;
  spirit_label?: string | null;
  spirit_current?: number | null;
  spirit_max?: number | null;
  spirit_progress?: number | null;
  spirit_ready_at?: number | null;
  cooldowns: CooldownView[];
};

export type OverlaySnapshot = {
  configured_log_dir: string;
  resolved_path: string;
  overlay_enabled: boolean;
  dungeon_active: boolean;
  processed_line_count: number;
  players: PlayerOverlay[];
};

export type OverlayStatus = "idle" | "loading" | "watching" | "error";

export type OverlayClientState = {
  snapshot: OverlaySnapshot | null;
  status: OverlayStatus;
  error: string | null;
};
