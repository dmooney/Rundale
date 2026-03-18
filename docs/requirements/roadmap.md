# Parish Roadmap

> [Docs Index](../index.md)

> Last updated: 2026-03-18
> Current phase: **Phase 1 — Core Loop**

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

- [ ] Full `Location` struct with connections and properties
- [ ] `WorldGraph` struct (adjacency list)
- [ ] Hand-authored test parish JSON (~10-15 locations)
- [ ] OSM data extraction tool
- [ ] Movement command handling ("go to X", traversal time)
- [ ] Time advancement during traversal
- [ ] En-route encounter system
- [ ] Dynamic location descriptions (LLM-enriched templates)

## Phase 3 — Multiple NPCs & Simulation

> [Detailed plan](../plans/phase-3-npcs-simulation.md) | [Design: NPC System](../design/npc-system.md), [Cognitive LOD](../design/cognitive-lod.md)

- [ ] Full NPC entity model (schedule, relationships, memory)
- [ ] `NpcManager` with tier assignment and tick dispatch
- [ ] Tier 1 full inference tick
- [ ] Tier 2 lighter inference tick
- [ ] NPC schedule-driven movement
- [ ] Short-term memory system
- [ ] Initial NPC data (5-10 NPCs for test parish)
- [ ] "Overhear" mechanic for Tier 2 interactions

## Phase 4 — Persistence

> [Detailed plan](../plans/phase-4-persistence.md) | [Design: Persistence](../design/persistence.md)

- [ ] SQLite schema design and migrations
- [ ] Journal system (append-only event log)
- [ ] Periodic snapshot compaction (background task)
- [ ] `/save` command
- [ ] `/quit` with autosave and clean shutdown
- [ ] `/load <name>` — load branch head
- [ ] `/fork <name>` — create new branch
- [ ] `/branches` and `/log` commands

## Phase 5 — Full LOD & Scale

> [Detailed plan](../plans/phase-5-full-lod-scale.md) | [Design: Cognitive LOD](../design/cognitive-lod.md), [Weather](../design/weather-system.md)

- [ ] Tier 3 batch inference
- [ ] Tier 4 CPU-only rules engine
- [ ] Tier transition: inflate/deflate NPC state
- [ ] Event bus across tier boundaries
- [ ] Expand world graph beyond starting parish
- [ ] Weather state machine
- [ ] Seasonal cycle effects on NPCs
- [ ] Gossip/information propagation
- [ ] NPC long-term memory with retrieval

## Phase 6 — Polish & Mythology Hooks

> [Detailed plan](../plans/phase-6-polish-mythology.md) | [Design: Mythology Hooks](../design/mythology-hooks.md)

- [ ] `/help` command
- [ ] `/map` command (ASCII rendering)
- [ ] `/status`, `/log`, `/branches` UI
- [ ] `mythological_significance` location property
- [ ] Festival event hooks in time system
- [ ] Night-time atmosphere differentiation
- [ ] NPC belief/superstition knowledge fields

## Open Questions

> [Detailed analysis](../plans/open-questions.md)

- [ ] Exact parish location near Roscommon
- [ ] Player character model (named local vs. newcomer vs. observer)
- [ ] Goal/quest structure
- [ ] Story and lore
- [ ] Command prefix UX (keep `/` or go prefix-free)
- [ ] Mythology content scope
- [ ] Player verb set (minimal, moderate, or expansive)
