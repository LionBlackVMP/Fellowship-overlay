import { OverlaySnapshot, OverlayStatus } from "../../store/overlay";

type SettingsViewProps = {
  snapshot: OverlaySnapshot | null;
  logPath: string;
  status: OverlayStatus;
  error: string | null;
  onToggleOverlay: (enabled: boolean) => void;
};

export function SettingsView({
  snapshot,
  logPath,
  status,
  error,
  onToggleOverlay,
}: SettingsViewProps) {
  const overlayEnabled = snapshot?.overlay_enabled ?? true;
  const resolvedLogPath = snapshot?.resolved_path ?? logPath;
  const formattedLogPath = resolvedLogPath.split("\\").join("\\\u200b").split("/").join("/\u200b");

  return (
    <div className="settings-shell">
      <section className="settings-panel">
        <div className="settings-title">Fellowship Overlay</div>

        <div className="settings-block">
          <label className="field">
            <span>Combat log path</span>
            <div className="text-input text-display">{formattedLogPath}</div>
          </label>

          <div className="status-row">
            <span className={`status-pill status-${status}`}>{status}</span>
            <span>{snapshot?.resolved_path ? "Log attached" : "No log attached"}</span>
          </div>

          <div className="settings-actions">
            <button
              type="button"
              className={`toggle-button ${overlayEnabled ? "is-disable-action" : "is-enable-action"}`}
              onClick={() => onToggleOverlay(!overlayEnabled)}
            >
              {overlayEnabled ? "Disable Trinket Overlay" : "Enable Trinket Overlay"}
            </button>
          </div>

          <div className="settings-footer">
            <div className="settings-copy">
              <div>Overlay: {overlayEnabled ? "Enabled" : "Disabled"}</div>
              <div>Processed lines: {snapshot?.processed_line_count ?? 0}</div>
            </div>

            {error ? <p className="message error">{error}</p> : null}
            {!error ? (
              <p className="message hint">
                This window is your control panel. The compact overlay frames stay visible in game.
              </p>
            ) : null}
          </div>
        </div>
      </section>
    </div>
  );
}
