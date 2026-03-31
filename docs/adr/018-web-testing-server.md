# ADR-018: Web Server Mode for Chrome GUI Testing

**Status:** Accepted
**Date:** 2026-03-26
**Context:** [Roadmap](../requirements/roadmap.md) | [Phase 8: Tauri GUI](../plans/phase-8-tauri-gui.md)

## Context

The Parish desktop GUI runs inside Tauri's WebView, which cannot be driven by
standard browser automation tools (Playwright, Puppeteer, Selenium). This makes
automated visual testing of the Svelte frontend impossible without the native
desktop app running.

We need a way to serve the same Svelte frontend in a standard browser so that
Chrome can be automated via Playwright for end-to-end GUI testing.

## Decision

Add an **axum web server mode** (`--web [port]`) to the Parish CLI that:

1. Serves the built Svelte frontend (`ui/dist/`) as static files.
2. Exposes the same 5 commands as REST endpoints (`GET /api/world-snapshot`,
   `GET /api/map`, `GET /api/npcs-here`, `GET /api/theme`,
   `POST /api/submit-input`).
3. Relays the same 6 push events over WebSocket (`GET /api/ws`):
   `stream-token`, `stream-end`, `text-log`, `world-update`, `loading`,
   `theme-update`.
4. Reuses all game logic from `parish-core` — no duplication of game systems.

The frontend `ipc.ts` auto-detects whether it's running inside Tauri or in a
browser and uses the appropriate transport (Tauri invoke/listen vs fetch/WebSocket).

### Architecture

```
parish-core::ipc      ← shared types + handler functions
    ↑           ↑
src-tauri/     crates/parish-server/
(Tauri IPC)    (axum HTTP + WebSocket)
```

- **IPC types** (`WorldSnapshot`, `MapData`, `NpcInfo`, `ThemePalette`, event
  payloads) are defined in `parish_core::ipc::types` and re-exported by both
  `src-tauri` and `parish-server`.
- **Handler functions** (`snapshot_from_world`, `build_map_data`,
  `build_npcs_here`, `build_theme`) live in `parish_core::ipc::handlers`.
- **Streaming logic** is duplicated in `parish-server` (~50 lines), adapted
  to emit events through a `tokio::sync::broadcast` channel instead of
  `tauri::AppHandle`. The function is small and the two copies are simple
  enough that duplication is preferable to adding a shared trait.

### Testing

Playwright E2E tests in `ui/e2e/` drive Chromium against the axum server:

```sh
cd ui && npx playwright test
```

The Playwright config starts the server automatically via
`cargo run -- --web 3099`.

## Consequences

- The Tauri desktop app is **unchanged** — it continues to work via Tauri IPC.
- The web server is single-session, intended for local testing only (no auth).
- Svelte components require zero changes (only `ipc.ts` was modified).
- Five `data-testid` attributes were added to root elements of key components
  for reliable Playwright selectors.
- The `parish-server` crate adds `axum`, `tower-http` dependencies but only
  to the workspace, not to the Tauri binary.
