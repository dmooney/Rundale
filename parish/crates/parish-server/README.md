# parish-server

Axum web backend for running Parish in a browser.

## Purpose

`parish-server` hosts the Svelte UI and exposes HTTP/WebSocket endpoints that
mirror the desktop/headless game behavior.

## Key modules

- `routes` and `ws` — API routes and real-time game channel.
- `session` and `state` — per-user session lifecycle and shared app state.
- `auth`, `cf_auth`, `middleware` — authentication and request policy.
- `editor_routes` — mod editor API surface.

## Notes

Each browser visitor gets an isolated session with persisted save state.
