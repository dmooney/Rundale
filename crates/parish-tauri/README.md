# parish-tauri

Tauri backend bridge between Parish engine and Svelte desktop UI.

## Purpose

`parish-tauri` provides desktop app initialization, typed command handlers, and
event streaming between the Rust engine (`parish-core`) and the frontend.

## Key modules

- `commands` — game runtime IPC commands exposed to the frontend.
- `editor_commands` — mod editor-specific command surface.
- `events` — event emission for world/chat/simulation updates.
- `main` / `lib` — Tauri startup and state wiring.

## Notes

Keep UI transport concerns here; gameplay behavior belongs in shared engine
crates for parity with CLI and web modes.
