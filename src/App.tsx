import "./App.css";
import { openOverlayMainMenu, startOverlayDrag } from "./api";
import { OverlayView } from "./components/OverlayView";
import { SettingsView } from "./components/SettingsView";
import { useLogs } from "./hooks/useLogs";

function formatOverlayTimer(seconds: number) {
  if (seconds >= 60) {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${String(secs).padStart(2, "0")}`;
  }

  return String(seconds);
}

function App() {
  const { snapshot, logPath, setOverlayEnabled, status, error } = useLogs();
  const view = new URLSearchParams(window.location.search).get("view") ?? "settings";

  const startDragging = async () => {
    try {
      await startOverlayDrag();
    } catch (err) {
      console.error("Failed to start overlay drag:", err);
    }
  };

  const openMainMenu = async () => {
    try {
      await openOverlayMainMenu();
    } catch (err) {
      console.error("Failed to open main menu:", err);
    }
  };

  if (view === "overlay") {
    return (
      <OverlayView
        players={snapshot?.players ?? []}
        onOpenMenu={() => {
          void openMainMenu();
        }}
        onStartDragging={() => {
          void startDragging();
        }}
        formatTimer={formatOverlayTimer}
      />
    );
  }

  return (
    <SettingsView
      snapshot={snapshot}
      logPath={logPath}
      status={status}
      error={error}
      onToggleOverlay={(enabled) => {
        void setOverlayEnabled(enabled);
      }}
    />
  );
}

export default App;
