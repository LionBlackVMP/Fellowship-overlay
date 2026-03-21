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
  cooldowns: CooldownView[];
};

export type OverlaySnapshot = {
  resolved_path: string;
  overlay_enabled: boolean;
  dungeon_active: boolean;
  processed_line_count: number;
  players: PlayerOverlay[];
};

export type OverlayStatus = "idle" | "loading" | "watching" | "error";
