# Parish Roadmap

> [Docs Index](../index.md)

> Last updated: 2026-03-25
> Current phase: **Phase 5 — Full LOD & Scale** (complete)

## Status Legend

- [ ] Not started
- [~] In progress
- [x] Complete

---

## Phase 1 — Core Loop

> [Detailed plan](../plans/phase-1-core-loop.md) | [Design: Architecture Overview](../design/overview.md)

- [x] Rust project scaffolding (Cargo.toml, module declarations, dependencies)
- [x] Error type definitions (`ParishError` with thiserror)
- [x] Tokio runtime + tracing initialization
- [x] `GameClock`, `TimeOfDay`, `Season` enums in `world/time.rs`
- [x] Basic `Location` struct and `WorldState` in `world/mod.rs`
- [x] TUI terminal init/restore with crossterm panic hook
- [x] Main render loop: top bar, main text panel, input prompt
- [x] Day/night color palette system (dawn → midnight RGB gradients)
- [x] `Command` enum and `/` prefix parsing in `input/mod.rs`
- [x] `OllamaClient` in `inference/client.rs` (reqwest + timeout)
- [x] `InferenceRequest`/`InferenceResponse` types and tokio mpsc queue
- [x] Basic `Npc` struct with identity, personality, location
- [x] NPC context construction for Tier 1
- [x] Player intent parsing via Ollama (natural language → structured JSON)
- [x] Main game loop wiring: input → parse → inference → response → render

## Phase 2 — World Graph

> [Detailed plan](../plans/phase-2-world-graph.md) | [Design: World & Geography](../design/world-geography.md)

- [x] Full `Location` struct with connections and properties
- [x] `WorldGraph` struct (adjacency list with BFS pathfinding)
- [x] Hand-authored test parish JSON (15 Kiltoom locations, starting at Kilteevan Village)
- [ ] OSM data extraction tool (stretch goal)
- [x] Movement command handling ("go to X", fuzzy matching, traversal time)
- [x] Time advancement during traversal
- [x] En-route encounter system (probability-based, time-of-day weighted)
- [x] Dynamic location descriptions (template interpolation with time/weather/NPCs)

## Phase 3 — Multiple NPCs & Simulation

> [Detailed plan](../plans/phase-3-npcs-simulation.md) | [Design: NPC System](../design/npc-system.md), [Cognitive LOD](../design/cognitive-lod.md)

- [x] Full NPC entity model (schedule, relationships, memory)
- [x] `NpcManager` with tier assignment and tick dispatch
- [x] Tier 1 enhanced inference with memory/relationships
- [x] Tier 2 lighter inference tick
- [x] NPC schedule-driven movement
- [x] Short-term memory system (20-entry ring buffer)
- [x] Initial NPC data (8 NPCs for test parish)
- [x] "Overhear" mechanic for Tier 2 interactions

## Phase 4 — Persistence

> [Detailed plan](../plans/phase-4-persistence.md) | [Design: Persistence](../design/persistence.md)

- [x] SQLite schema design and migrations
- [x] Journal system (append-only event log)
- [x] Periodic snapshot compaction (autosave every 45s)
- [x] `/save` command
- [x] `/quit` with autosave and clean shutdown
- [x] `/load <name>` — load branch head
- [x] `/fork <name>` — create new branch
- [x] `/branches` and `/log` commands

## Phase 5 — Full LOD & Scale

> [Detailed plan](../plans/phase-5-full-lod-scale.md) | [Design: Cognitive LOD](../design/cognitive-lod.md), [Weather](../design/weather-system.md)

- [x] Tier 3 batch inference
- [x] Tier 4 CPU-only rules engine
- [x] Tier transition: inflate/deflate NPC state
- [x] Event bus across tier boundaries
- [x] Expand world graph beyond starting parish
- [x] Weather state machine
- [x] Seasonal cycle effects on NPCs
- [x] Gossip/information propagation
- [x] NPC long-term memory with retrieval

## Phase 6 — Polish & Mythology Hooks

> [Detailed plan](../plans/phase-6-polish-mythology.md) | [Design: Mythology Hooks](../design/mythology-hooks.md)

- [ ] `/help` command
- [ ] `/map` command (ASCII rendering)
- [ ] `/status`, `/log`, `/branches` UI
- [ ] `mythological_significance` location property
- [ ] Festival event hooks in time system
- [ ] Night-time atmosphere differentiation
- [ ] NPC belief/superstition knowledge fields

## Phase 7 — Web & Mobile Apps

> [Detailed plan](../plans/phase-7-web-mobile.md)

- [ ] Client-server protocol definition (`ClientMessage` / `ServerMessage`)
- [ ] `GameSession` extraction (decouple game engine from UI)
- [ ] axum game server with WebSocket support
- [ ] Session management (create, resume, idle timeout)
- [ ] Web client: Svelte SPA deployed to static hosting
- [ ] Web client: WebSocket networking layer
- [ ] Mobile client: Tauri v2 project (iOS + Android)
- [ ] Mobile-specific adaptations (touch input, responsive layout)
- [ ] Authentication (session tokens)
- [ ] Server deployment (Docker, health checks)
- [ ] Monitoring and rate limiting

## Phase 8 — Tauri GUI Rewrite

> [Detailed plan](../plans/phase-8-tauri-gui.md) | [ADR-015](../adr/015-tauri-svelte-gui.md)

- [x] Convert Cargo.toml to workspace (root + crates/parish-core + src-tauri)
- [x] Extract pure game logic to `crates/parish-core` library crate
- [x] Delete `src/gui/` (egui); clean root `lib.rs` and `main.rs`
- [x] Scaffold `src-tauri/` Tauri 2 backend with `AppState`, IPC commands, streaming events
- [x] Scaffold `ui/` Svelte 5 + SvelteKit frontend (static adapter)
- [x] IPC types (`ui/src/lib/types.ts`), command wrappers (`ipc.ts`), Svelte stores
- [x] Svelte components: StatusBar, ChatPanel, MapPanel, Sidebar, InputField
- [x] CSS theme via CSS custom properties (`var(--color-*)`) driven by Rust theme-tick events
- [x] Add `lat`/`lon` to `LocationData` for SVG map projection
- [x] Frontend component tests (Vitest + @testing-library/svelte, 22 tests)
- [ ] Screenshot replacement via `WebviewWindow::capture_image()`

## Open Questions

> [Detailed analysis](../plans/open-questions.md) — **All resolved.**

- [x] Exact parish location near Roscommon → **Kiltoom** (Barony of Athlone South)
- [x] Player character model → **Newcomer / "blow-in"**
- [x] Goal/quest structure → **Purely emergent** (prototype); hybrid later
- [x] Story and lore → **Mundane surface** with mythology hooks in Phase 6
- [x] Command prefix UX → **`/` prefix** through Phase 5; hybrid in Phase 6
- [x] Mythology content scope → **Moderate / behavioral** via NPC prompt modification
- [x] Player verb set → **Phased rollout** starting minimal (Move, Talk, Look, Examine)
