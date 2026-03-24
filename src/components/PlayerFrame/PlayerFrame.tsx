import { MouseEvent } from "react";
import { PlayerOverlay } from "../../store/overlay";
import { TrinketIcon } from "../TrinketIcon";

type PlayerFrameProps = {
  player: PlayerOverlay;
  onOpenMenu: () => void;
  onStartDragging: () => void;
  formatTimer: (seconds: number) => string;
};

export function PlayerFrame({
  player,
  onOpenMenu,
  onStartDragging,
  formatTimer,
}: PlayerFrameProps) {
  const ultimateReady =
    player.spirit_current !== undefined &&
    player.spirit_current !== null &&
    player.spirit_ready_at !== undefined &&
    player.spirit_ready_at !== null &&
    player.spirit_current >= player.spirit_ready_at;

  const handleMouseDown = (event: MouseEvent<HTMLElement>) => {
    if (event.button === 0 && event.ctrlKey) {
      event.preventDefault();
      onOpenMenu();
      return;
    }

    if (event.button === 0) {
      onStartDragging();
    }
  };

  return (
    <section
      className={`player-frame ${ultimateReady ? "is-ultimate-ready" : ""}`}
      onMouseDown={handleMouseDown}
      onContextMenu={(event) => {
        event.preventDefault();
        if (event.ctrlKey) {
          onOpenMenu();
        }
      }}
    >
      <div className="frame-name" style={{ color: player.class_color }}>
        {player.name}
      </div>
      {player.spirit_current !== undefined &&
      player.spirit_current !== null &&
      player.spirit_max !== undefined &&
      player.spirit_max !== null &&
      player.spirit_progress !== undefined &&
      player.spirit_progress !== null ? (
        <div className="frame-spirit">
          <div
            className={`frame-spirit-bar ${
              player.spirit_ready_at !== undefined &&
              player.spirit_ready_at !== null &&
              player.spirit_current >= player.spirit_ready_at
                ? "is-ultimate-ready"
                : "is-building"
            }`}
          >
            <div
              className="frame-spirit-fill"
              style={{ width: `${Math.max(0, Math.min(1, player.spirit_progress)) * 100}%` }}
            />
            {player.spirit_ready_at !== undefined &&
            player.spirit_ready_at !== null &&
            player.spirit_max !== undefined &&
            player.spirit_max !== null &&
            player.spirit_ready_at < player.spirit_max ? (
              <div
                className="frame-spirit-threshold"
                style={{
                  left: `${Math.max(0, Math.min(1, player.spirit_ready_at / player.spirit_max)) * 100}%`,
                }}
              />
            ) : null}
            <div className="frame-spirit-value">
              <span>{player.spirit_current}</span>
              <span>{player.spirit_max}</span>
            </div>
          </div>
        </div>
      ) : null}
      <div className="frame-trinkets">
        {player.cooldowns.map((cooldown) => (
          <TrinketIcon
            key={cooldown.key}
            cooldown={cooldown}
            timerLabel={cooldown.ready ? "" : formatTimer(cooldown.remaining_seconds)}
          />
        ))}
      </div>
    </section>
  );
}
