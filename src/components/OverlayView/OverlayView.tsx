import { useEffect } from "react";
import { PlayerOverlay } from "../../store/overlay";
import { PlayerFrame } from "../PlayerFrame";

type OverlayViewProps = {
  players: PlayerOverlay[];
  onOpenMenu: () => void;
  onStartDragging: () => void;
  formatTimer: (seconds: number) => string;
};

export function OverlayView({
  players,
  onOpenMenu,
  onStartDragging,
  formatTimer,
}: OverlayViewProps) {
  useEffect(() => {
    const handleContextMenu = (event: MouseEvent) => {
      if (event.ctrlKey) {
        event.preventDefault();
        onOpenMenu();
        return;
      }

      event.preventDefault();
    };

    const handleRightMouse = (event: MouseEvent) => {
      if (event.button === 2) {
        event.preventDefault();
      }
    };

    window.addEventListener("contextmenu", handleContextMenu, true);
    document.addEventListener("contextmenu", handleContextMenu, true);
    window.addEventListener("mousedown", handleRightMouse, true);
    document.addEventListener("mousedown", handleRightMouse, true);
    window.addEventListener("mouseup", handleRightMouse, true);
    document.addEventListener("mouseup", handleRightMouse, true);
    window.addEventListener("auxclick", handleRightMouse, true);
    document.addEventListener("auxclick", handleRightMouse, true);

    return () => {
      window.removeEventListener("contextmenu", handleContextMenu, true);
      document.removeEventListener("contextmenu", handleContextMenu, true);
      window.removeEventListener("mousedown", handleRightMouse, true);
      document.removeEventListener("mousedown", handleRightMouse, true);
      window.removeEventListener("mouseup", handleRightMouse, true);
      document.removeEventListener("mouseup", handleRightMouse, true);
      window.removeEventListener("auxclick", handleRightMouse, true);
      document.removeEventListener("auxclick", handleRightMouse, true);
    };
  }, [onOpenMenu]);

  return (
    <div
      className="overlay-shell"
      onContextMenu={(event) => {
        event.preventDefault();
      }}
    >
      <div className="overlay-frames">
        {players.map((player) => (
          <PlayerFrame
            key={player.name}
            player={player}
            onOpenMenu={onOpenMenu}
            onStartDragging={onStartDragging}
            formatTimer={formatTimer}
          />
        ))}
      </div>
    </div>
  );
}
