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
      className="player-frame"
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
