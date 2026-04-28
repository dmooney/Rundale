# Chrome Browser Testing Session Report

> Date: 2026-03-30
> Branch: `claude/web-ui-chrome-testing-ZUNZ1`
> Method: Live manual testing via Claude-in-Chrome MCP extension
> LLM Provider: OpenRouter (nvidia/nemotron-3-super-120b-a12b:free)

## Overview

First end-to-end browser test of Rundale running in Chrome via the axum web
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
| Theme/palette updates | Pass | Time-of-day palette updated as clock advanced |
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

---

## Session 2: 2026-04-01

> Branch: `claude/fix-open-issues-dfsiw` (post-rebase onto origin/main)
> Method: Live manual testing via Claude-in-Chrome MCP extension
> LLM Provider: Groq (llama-3.1-8b-instant) — .env not loaded by web server; NPC tests inconclusive

### Overview

Post-rebase smoke test to verify merge conflict resolutions in `input/mod.rs`
and `movement.rs` didn't break the web UI. Core navigation, system commands,
and edge cases all passed. NPC conversation was inconclusive due to the web
server not picking up the `.env` file (LLM requests hit a dead endpoint).

### Setup

1. Built frontend: `cd ui && npm run build`
2. Killed stale server on port 3001 (leftover from previous session)
3. Started fresh server: `cargo run -- --web 3001`
4. Navigated Chrome to `http://127.0.0.1:3001`

### Features Tested

| Feature | Status | Notes |
|---------|--------|-------|
| Page load / initial render | Pass | Status bar, map, NPCs, chat, input all present |
| Status bar | Pass | Location, time, weather, season update on travel |
| Map rendering | Pass | SVG minimap with player dot and labels |
| NPC sidebar updates | Pass | NPCs appear/disappear based on location and time |
| Travel (direct hop) | Pass | Crossroads: narration + time advance |
| Travel (multi-hop) | Pass | Village to Pub via Crossroads: 19 min narration |
| Invalid location | Pass | "You haven't the faintest notion how to reach 'castle dracula'." |
| Already-here detection | Pass | "Sure, you're already standing right here." |
| Empty submit | Pass | No-op, input stays focused |
| `/help` | Pass | Shows command list with descriptions |
| `/status` | Pass | "Location: Darcy's Pub \| Afternoon \| Spring" |
| `/pause` | Pass | "The clocks of the parish stand still." + status bar indicator |
| `/resume` | Pass | "Time stirs again in the parish." + indicator removed |
| `/wait 180` | Pass | Advances 3 hours, NPC schedules update |
| Idle message (no NPCs) | Pass | "Only the sound of a distant crow." |
| NPC conversation (LLM) | Inconclusive | .env not loaded; loading indicator shown but no response |
| Console errors | Pass | No JavaScript errors throughout session |

### Bugs Found

None (all tests that could run passed cleanly).

### Notes

- The multi-hop travel narration correctly uses the new verb logic from the
  rebase: "You set off along the road north past low fields to the crossroads
  toward Darcy's Pub."
- The `/fork` and `/load` commands were not explicitly tested in-browser but
  the conflict resolution preserved both bare-command support and safe `.len()`
  slicing.
- The web server does not automatically load `.env` from the project root;
  LLM-dependent tests require the provider env vars to be exported in the shell
  or passed on the command line.

### Environment

- macOS Darwin 24.6.0
- Chrome with Claude-in-Chrome MCP extension
- Rust axum server (debug build, freshly compiled post-rebase)
- Svelte 5 + SvelteKit (static adapter)

---

## Session 2 — 2026-04-03

> Branch: `claude/fix-issue-179-G1T3U` (rebased onto `origin/main`)
> Method: Live manual testing via Claude-in-Chrome MCP extension
> Focus: Verifying #179 fix — 7 missing persistence API routes in web server

### Summary

| Test | Result |
|------|--------|
| Page Load (status bar, map, NPCs, chat, input) | ✅ PASS |
| Navigation — Darcy's Pub → The Crossroads | ✅ PASS |
| Navigation — The Crossroads → St. Brigid's Church | ✅ PASS |
| Edge case: invalid location ("go to timbuktu") | ✅ PASS |
| Edge case: already-here | ✅ PASS |
| Edge case: empty submit | ✅ PASS |
| `/help` system command | ✅ PASS |
| `/status` system command | ✅ PASS |
| `/pause` system command | ✅ PASS |
| `/resume` system command | ✅ PASS |
| `GET /api/discover-save-files` (was 404, now 200) | ✅ PASS |
| `GET /api/save-state` (was 404, now 200) | ✅ PASS |
| `GET /api/save-game` (was 404, now 200) | ✅ PASS |
| `GET /api/new-save-file` (was 404, now 200) | ✅ PASS |
| `GET /api/new-game` (was 404, now 200) | ✅ PASS |
| `POST /api/load-branch` — nonexistent file → 500 (expected) | ✅ PASS |
| `POST /api/create-branch` — no active save → 400 (expected) | ✅ PASS |
| Ledger UI — opens, shows save entry with YOU ARE HERE | ✅ PASS |
| Browser console errors | ✅ NONE |

### Key Observations

- **All 7 persistence routes are live.** Before this fix they returned 404; now all respond correctly.
- **Save flow works end-to-end:** `save-game` returned `"Game saved to parish_004.db (branch: main)."`, `discover-save-files` found 4 save files, `save-state` reported `{branch_id: 1, branch_name: "main", filename: "parish_004.db"}`.
- **new-game route resets properly:** Calling `/api/new-game` restarted at Kilteevan Village with the log message "A new chapter begins in the parish..." visible in the chat.
- **Ledger UI:** Opening the LEDGER button after saving shows the "main" branch card at "Kilteevan Village, 20 Mar 1820, Morning" with "YOU ARE HERE" indicator.
- **React-to-message route** (added in `origin/main` alongside this fix) is present in the router and was correctly merged during rebase.
- **Note on server restart:** The server running on port 3001 at test start was from an old binary (pre-fix). Had to kill and restart from the worktree binary to validate the new routes. All routes passed after restart.

### Bugs Found

None.

### Environment

- macOS Darwin 24.6.0
- Chrome with Claude-in-Chrome MCP extension
- Rust axum web server, debug build from `/Users/dmooney/Parish/.worktrees/2`
- Svelte 5 + SvelteKit (static adapter, freshly built)
