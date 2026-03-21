import { CSSProperties } from "react";
import { CooldownView } from "../../store/overlay";

type TrinketIconProps = {
  cooldown: CooldownView;
  timerLabel: string;
};

export function TrinketIcon({ cooldown, timerLabel }: TrinketIconProps) {
  return (
    <div
      className={`trinket-chip ${cooldown.ready ? "is-ready" : "is-cooling"}`}
      style={{ "--cooldown-progress": `${cooldown.progress}` } as CSSProperties}
      >
      <img
        className="trinket-icon"
        src={cooldown.relic_icon_src}
        alt={cooldown.relic_name}
      />
      {!cooldown.ready ? <div className="trinket-cooldown-sweep" /> : null}
      {timerLabel ? <div className="trinket-timer">{timerLabel}</div> : null}
    </div>
  );
}
