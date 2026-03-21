# Fellowship Overlay

Compact Tauri + React overlay for **Fellowship** that tracks **trinket cooldowns** from the game's combat log and shows them in a small in-game party frame.

## What It Does

- Watches the latest `CombatLog*.txt` file in the Fellowship `Saved/CombatLogs` folder
- Detects party members from the combat log
- Tracks equipped trinkets and their cooldown state
- Shows cooldowns in a compact overlay directly over the game window
- Keeps a separate settings window for controlling the overlay

## What Is Tracked

The overlay focuses on **trinket cooldowns**.

For each party member it displays:

- Character name
- Equipped trinket icons
- Active cooldown timer directly on top of the icon
- Ready state when a trinket is available again

## Preview

![Fellowship Overlay Preview](./docs/overlay-preview.png)

## Tech Stack

- [Tauri 2](https://tauri.app/)
- [React](https://react.dev/)
- [TypeScript](https://www.typescriptlang.org/)
- [Redux Toolkit](https://redux-toolkit.js.org/)
- Rust backend parser for combat-log processing

## Development Run

From the project root:

```bash
npm install
npm run tauri dev
```

This starts:

- the Vite frontend
- the Tauri desktop app
- the Rust backend that parses the combat log

## Build A Release Version

If you want to run it on your PC as a normal production app, build a release package:

```bash
npm install
npm run tauri build
```

After the build finishes, the production files will be here:

- [src-tauri/target/release](E:/projects/Fellowship-overlay/src-tauri/target/release)
- [src-tauri/target/release/bundle](E:/projects/Fellowship-overlay/src-tauri/target/release/bundle)

Usually on Windows you will get one of these:

- `.msi` installer
- setup `.exe`
- release executable in `src-tauri/target/release`

## How To Install And Run On Your PC

### Option 1: Install with the generated installer

1. Build the app:

```bash
npm run tauri build
```

2. Open:

[src-tauri/target/release/bundle](E:/projects/Fellowship-overlay/src-tauri/target/release/bundle)

3. Run the generated installer:

- `.msi`, or
- setup `.exe`

4. Launch the installed app from Windows like a normal desktop program.

### Option 2: Run the built executable directly

1. Build the app:

```bash
npm run tauri build
```

2. Open:

[src-tauri/target/release](E:/projects/Fellowship-overlay/src-tauri/target/release)

3. Run the executable directly:

```text
fellowship-overlay.exe
```

## Requirements

You usually need these installed on Windows before building:

- [Node.js](https://nodejs.org/)
- [Rust](https://www.rust-lang.org/tools/install)
- Microsoft Visual Studio C++ Build Tools
- Microsoft Edge WebView2 Runtime

## How It Works

The Rust backend:

- resolves the newest combat log file
- parses new log lines
- tracks party members and trinket cooldown state
- controls overlay visibility

The React frontend:

- renders the current snapshot from the backend
- shows the settings window
- renders the compact in-game trinket frames

## Notes

- The overlay depends on Fellowship combat log output
- If no supported combat log is available, the overlay will not show useful cooldown data
- The small overlay window and the main settings window are handled separately

## Repository

GitHub repository:

[https://github.com/LionBlackVMP/Fellowship-overlay](https://github.com/LionBlackVMP/Fellowship-overlay)
