# Chrome Browser Testing Session Report

> Date: 2026-03-30
> Branch: `claude/web-ui-chrome-testing-ZUNZ1`
> Method: Live manual testing via Claude-in-Chrome MCP extension
> LLM Provider: OpenRouter (nvidia/nemotron-3-super-120b-a12b:free)

## Overview

First end-to-end browser test of Parish running in Chrome via the axum web
server (`cargo run -- --web 3001`). The Svelte frontend was served as static
files with a WebSocket event relay replacing Tauri IPC.

## Setup

1. Built frontend: `cd ui && npm install && npm run build`
2. Started server: `cargo run -- --web 3001`
3. Navigated Chrome to `http://127.0.0.1:3001`

## Bugs Found

### Bug 1: IPC URL mapping drops `get_` prefix (Critical)

The `command()` function in `ui/src/lib/ipc.ts` converted Tauri command names
to REST endpoints by replacing underscores with hyphens, but did not strip the
`get_` prefix. This caused all initial data fetches to 404:

- `get_world_snapshot` mapped to `/api/get-world-snapshot` instead of `/api/world-snapshot`
- `get_map` mapped to `/api/get-map` instead of `/api/map`
- `get_npcs_here` mapped to `/api/get-npcs-here` instead of `/api/npcs-here`

**Impact**: Map, NPC sidebar, and world state all failed to load on page open.
The game appeared broken (no map, no NPCs, empty sidebar).

**Fix applied**: Added `.replace(/^get_/, '')` before the hyphen conversion in
`ipc.ts:34`.

### Bug 2: Missing `/api/debug-snapshot` endpoint (Moderate)

The frontend calls `getDebugSnapshot()` on mount and when toggling the debug
panel, but the axum server had no corresponding route.

**Impact**: Debug panel had no data, and a 404 error appeared in the console on
every page load.

**Fix applied**: Added `get_debug_snapshot` handler in `routes.rs` using
`parish_core::debug_snapshot::build_debug_snapshot`, and wired the route in
`lib.rs`.

## Features Tested

| Feature | Status | Notes |
|---------|--------|-------|
| Page load / initial data fetch | Pass (after fix) | World snapshot, map, NPCs all load |
| Status bar | Pass | Location, time, weather, season update correctly |
| Map rendering | Pass (after fix) | SVG map with geo-projected nodes, player dot |
| Map player position updates | Pass | Dot moves on travel |
| NPC sidebar | Pass (after fix) | Shows name, role, mood for NPCs at location |
| Chat panel | Pass | Scrolling log with player/system/NPC entries |
| Input field (Enter key) | Pass | Submits on Enter |
| Input field (Send button) | Pass | Submits on click |
| Empty submit | Pass | No-op, no error |
| Travel (`go to <place>`) | Pass | Narration, time advance, location update |
| Invalid location | Pass | "You haven't the faintest notion..." + exits |
| Already-here detection | Pass | "Sure, you're already standing right here." |
| Look command | Pass | Renders location description + exits |
| NPC conversation (LLM) | Pass | Streamed response via OpenRouter, in character |
| NPC token streaming | Pass | Text appears incrementally in chat |
| Irish word hints (Focail) | Pass | Populated from NPC response metadata |
| Idle message (no NPCs) | Pass | "The clouds shift. The parish carries on." |
| `/help` | Pass | Shows command list |
| `/status` | Pass | Shows location, time, season |
| `/pause` / `/resume` | Pass | Clock stops/starts, themed messages |
| `/speed fast` | Pass | "The parish quickens its step." |
| Debug panel toggle (DBG button) | Pass | Panel slides in below game area |
| Debug: Overview tab | Pass | Clock, location, tier summary |
| Debug: NPCs tab | Pass | All 8 NPCs with tier, mood, location, transit |
| Debug: World tab | Pass | Not deeply tested |
| Debug: Events tab | Pass | Empty (no debug events stored in web mode) |
| Debug: Inference tab | Pass | Not deeply tested |
| WebSocket reconnect | Not tested | Would need to kill/restart server mid-session |
| Theme/palette updates | Pass | Time-of-day tinting updated as clock advanced |
| Multiple NPC conversations | Pass | Talked to Fr. Tierney and Tommy O'Brien |

## Observations

- The game clock ran from 08:12 to 14:20 during testing (with `/speed fast`)
- NPC schedule ticking works: Niamh Darcy was observed "In Transit" in the
  debug panel
- The Focail panel accumulated hints across conversations (Dia dhuit, a chara,
  craic)
- Chat auto-scrolls to bottom on new messages
- Map is small but readable; node labels truncate at 14 characters

## Environment

- macOS Darwin 24.6.0
- Chrome with Claude-in-Chrome MCP extension
- Rust axum server (debug build)
- Svelte 5 + SvelteKit (static adapter)
