# First Contribution Guide

This guide is for newcomers who want to make their first meaningful contribution without getting lost in the repo layout.

## TL;DR: Mental model

Parish is a Rust workspace built around a shared core engine plus multiple runtime surfaces:

- `crates/parish-core` → engine and simulation logic (shared)
- root crate (`src/`) → CLI/headless runtime and app bootstrapping
- `crates/parish-tauri/` → desktop backend
- `crates/parish-server/` → web server backend
- `apps/ui/` → Svelte frontend with a dual transport layer

When in doubt: if logic should work in more than one mode, put it in `parish-core`.

---

## Repository structure and what matters most

### Workspace layers

1. **Core domain (`crates/parish-core`)**
   - World state and simulation
   - NPC management and cognitive tiers
   - Input classification
   - Inference queue and provider routing
   - Persistence abstractions
   - IPC data mappers shared by Tauri and web

2. **Root app (`src/`)**
   - App startup and mode routing
   - Headless REPL/game loop
   - Config loading and override wiring
   - Test harness orchestration

3. **Desktop app (`crates/parish-tauri/`)**
   - Tauri command handlers
   - Event emission wiring
   - Integration with core IPC payloads

4. **Web server (`crates/parish-server/`)**
   - Axum routes and WebSocket flow
   - Browser-compatible endpoints mirroring desktop behavior

5. **UI (`apps/ui/src/`)**
   - Main screen composition
   - Typed API wrappers over IPC
   - Runtime auto-switch between Tauri and browser transports

### Runtime boot flow (high level)

`crates/parish-cli/src/main.rs` chooses a mode (`--script`, `--web`, or headless default), resolves provider/config layering, and then starts the corresponding runtime path.

### Core simulation triangle to understand early

If you only study three subsystems first, make them:

- `world/` state and event flow
- `npc/` manager + tiered behavior
- `inference/` queue worker and category-based provider routing

---

## Contribution routing: where should each change go?

### 1) New slash/system command

Start in input classification and command dispatch, then wire each runtime surface:

- shared parse/intent behavior in `parish-core`
- headless dispatch path in `crates/parish-cli/src/headless.rs`
- web command endpoint in `crates/parish-server/src/routes.rs`
- desktop command wiring in `crates/parish-tauri/src/`

**Rule of thumb:** keep command semantics shared; keep transport-specific glue local.

### 2) New NPC behavior

Typical touch points:

- `crates/parish-core/src/npc/manager.rs`
- tier tick logic under `crates/parish-core/src/npc/`
- optional IPC mapping updates in `crates/parish-core/src/ipc/`

Suggested sequence:

1. Add/adjust NPC state types.
2. Update manager/tick orchestration.
3. Expose state in IPC only if UI/debug needs it.

### 3) New UI panel or UX feature

Primary frontend files:

- `apps/ui/src/routes/+page.svelte` for composition
- `apps/ui/src/lib/ipc.ts` for typed API wrappers

If backend work is required, mirror behavior across Tauri + web to preserve mode parity.

### 4) New REST or IPC endpoint

Use this pattern:

1. Add shared payload mapping/handler in `parish-core/src/ipc/`.
2. Expose on desktop (Tauri command/event).
3. Expose on web (`parish-server` route/ws).
4. Add frontend wrapper in `apps/ui/src/lib/ipc.ts`.

### 5) Content-only contribution (easiest first PR)

For a low-risk first contribution, update game data under `mods/rundale/`:

- world and location data
- NPC and encounter data
- prompts and configuration

This lets you improve game content without touching engine runtime behavior.

---

## Newcomer gotchas

1. **Do not duplicate shared gameplay logic in root `src/`.**
   Put reusable behavior in `crates/parish-core`.

2. **Parity across modes is intentional.**
   Check CLI/headless, Tauri, and web impact for feature changes.

3. **Config layering is real complexity.**
   Changes may interact with TOML config, env vars, and CLI args.

4. **UI transport is dual-mode by design.**
   Prefer extending existing IPC abstractions rather than adding one-off paths.

---

## Suggested learning path

Read these in order:

1. `README.md`
2. `docs/index.md`
3. `docs/design/overview.md`
4. `crates/parish-cli/src/main.rs` (startup + mode routing)
5. `crates/parish-cli/src/headless.rs` (single-turn runtime flow)
6. `crates/parish-core/src/world/`, `npc/`, and `inference/`
7. `crates/parish-cli/src/testing.rs` (GameTestHarness for controlled iteration)

---

## Good first issue ideas

If you want to practice repo conventions, start with one of these:

- **Backend:** add a lightweight read-only debug/system command with shared behavior + mode-specific transport wiring.
- **UI:** add a small panel that reads existing IPC state (no protocol changes).
- **Content:** expand one location + a few NPC schedule details in `mods/rundale`.

These are usually enough to learn repo layering without needing deep persistence or inference changes.
