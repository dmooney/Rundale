# Rundale Roadmap

> [Docs Index](../index.md)

> Last updated: 2026-04-09
> Current phase: **Phase 5 — Full LOD & Scale** (in progress; 5A/5B/5C/5D/5E done including Tier 2 wiring, priority queue redesign, and debug panel visibility; 5F not started)

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
>
> Broken into six independently workable sub-phases with explicit dependency ordering.

### Phase 5A — Event Bus & Tier Transitions

> [Detailed plan](../plans/phase-5a-event-bus-tier-transitions.md) | **Foundation — do first**

- [x] `WorldEvent` enum and `EventBus` (tokio broadcast)
- [x] Tier inflation: build narrative context on NPC promotion (distant → close)
- [x] Tier deflation: compact short-term memory on NPC demotion (close → distant)
- [x] Wire transitions into `NpcManager::assign_tiers`
- [x] Event journal bridge (persistence subscriber)

### Phase 5B — Weather State Machine

> [Detailed plan](../plans/phase-5b-weather-state-machine.md) | Depends on: 5A

- [x] Expand `Weather` enum (add PartlyCloudy, LightRain, HeavyRain)
- [x] `WeatherEngine` with seasonal transition probabilities
- [x] Weather affects NPC schedules (seek shelter in rain)
- [x] Weather context in Tier 2 prompts
- [x] Palette tinting for new weather variants
- [x] Publish `WeatherChanged` events via event bus

### Phase 5C — NPC Long-Term Memory & Gossip

> [Detailed plan](../plans/phase-5c-memory-gossip.md) | Depends on: 5A
>
> **Runtime status:** wired in all entry points. `propagate_gossip_at_location` and `apply_tier1_response` are called from `parish-tauri`, `parish-server`, and `parish-cli/headless`.

- [x] `LongTermMemory` with keyword-based retrieval
- [x] Short-term → long-term promotion on eviction (importance threshold)
- [x] Long-term memory recall in Tier 1 context construction
- [x] `GossipNetwork` with probabilistic propagation (60% transfer, 20% distortion)
- [x] Gossip creation from world events
- [x] Gossip injection into Tier 1 dialogue context
- [x] `ConversationLog` for per-location exchange history (scene awareness)
- [x] Witness memory system — bystander NPCs overhear conversations
- [x] Named relationships in prompts (by name, not NPC ID)
- [x] Scene continuity cues (no re-greeting mid-conversation)
- [x] `apply_tier1_response` wired in all modes (mood + memory updates)
- [x] Conversation log persisted in `GameSnapshot`

### Phase 5D — Tier 3 Batch Inference

> [Detailed plan](../plans/phase-5d-tier3-batch-inference.md) | Depends on: 5A, 5C
>
> **Runtime status:** wired in all three entry points. `tick_tier3` is dispatched from the background tick loops in parish-tauri (spawned task), parish-server (spawned task), and parish-cli/headless (inline await). The `tier3_in_flight` flag prevents double-dispatch.

- [x] `Tier3Update` / `Tier3Response` types
- [x] Batch prompt construction (8-10 NPCs per call) — `TIER3_BATCH_SIZE = 10`
- [x] Tier 3 tick function (every 1 in-game day) — wired into all three entry points
- [x] Priority queue: Tier 1 > Tier 2 > Tier 3 — multi-lane `InferenceQueue` (Interactive/Background/Batch) with `tokio::select! biased;` worker. Tier 3 routed through Batch lane via `submit_json`. Per-category simulation client routing deferred (TODO in call sites)
- [x] Skip overdue ticks (don't queue) — `tier3_in_flight` flag gates re-entry while a batch is awaited
- [x] Tier 3 vs Tier 4 distance split in `assign_tiers`

### Phase 5E — Tier 4 Rules Engine & Seasonal Effects

> [Detailed plan](../plans/phase-5e-tier4-seasonal-effects.md) | Depends on: 5A, 5B, 5D
>
> **Runtime status:** wired in all three entry points. `tick_tier4` runs inline (sub-ms CPU) inside the background tick scope, gated on `needs_tier4_tick`, with returned `Tier4Event`s passed through `apply_tier4_events` and published on `world.event_bus`.

- [x] `Tier4Event` enum and `tick_tier4` CPU-only rules engine
- [x] Life event probabilities (illness, death, birth, trade)
- [x] Seasonal schedule overrides (farmers, teachers, publicans)
- [x] Festival event hooks (Imbolc, Bealtaine, Lughnasa, Samhain)
- [x] NPC health state tracking
- [x] Run on `spawn_blocking`

### Phase 5F — World Graph Expansion

> [Detailed plan](../plans/phase-5f-world-expansion.md) | Depends on: 5D

- [ ] Roscommon town data (~10 locations)
- [ ] Athlone data (~5 locations)
- [ ] Dublin data (~5 locations)
- [ ] Inter-region connections with realistic travel times
- [ ] Multi-file graph loading
- [ ] New NPCs for expanded locations
- [ ] Long-journey travel narration

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
- [x] Scaffold `crates/parish-tauri/` Tauri 2 backend with `AppState`, IPC commands, streaming events
- [x] Scaffold `apps/ui/` Svelte 5 + SvelteKit frontend (static adapter)
- [x] IPC types (`apps/ui/src/lib/types.ts`), command wrappers (`ipc.ts`), Svelte stores
- [x] Svelte components: StatusBar, ChatPanel, MapPanel, Sidebar, InputField
- [x] CSS theme via CSS custom properties (`var(--color-*)`) driven by Rust theme-tick events
- [x] Add `lat`/`lon` to `LocationData` for SVG map projection
- [x] Frontend component tests (Vitest + @testing-library/svelte, 22 tests)
- [ ] Screenshot replacement via `WebviewWindow::capture_image()`

## Phase 9 — Gameplay-AI Quality

> Tracks the deferred items from the harness-engineering plan. These are
> design-stage; each links to a plan or ADR before any code lands.

- [ ] LLM-as-judge eval suite over Tier 1 / Tier 2 / Intent prompts → [Plan](../plans/llm-quality-evals.md)
- [ ] Function-calling / tool-use output for NPC dialogue → [ADR-020](../adr/020-npc-tool-use.md)
- [ ] Embedding-based long-term NPC memory retrieval → [ADR-021](../adr/021-npc-memory-retrieval.md)

## Open Questions

> [Detailed analysis](../plans/open-questions.md) — **All resolved.**

- [x] Exact parish location near Roscommon → **Kiltoom** (Barony of Athlone South)
- [x] Player character model → **Newcomer / "blow-in"**
- [x] Goal/quest structure → **Purely emergent** (prototype); hybrid later
- [x] Story and lore → **Mundane surface** with mythology hooks in Phase 6
- [x] Command prefix UX → **`/` prefix** through Phase 5; hybrid in Phase 6
- [x] Mythology content scope → **Moderate / behavioral** via NPC prompt modification
- [x] Player verb set → **Phased rollout** starting minimal (Move, Talk, Look, Examine)
