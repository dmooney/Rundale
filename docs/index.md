# Parish Documentation Index

> Back to [README](../README.md) | [CLAUDE.md](../CLAUDE.md) (agent quick reference)

This is the documentation hub for Parish, an Irish Living World Text Adventure set in 1820. Start here to find any document.

## Project Status

**Current phase: Phase 2 — World Graph** (in progress). Phase 1 (Core Loop) is complete.

See [Roadmap](requirements/roadmap.md) for detailed per-item status tracking.

| Phase | Name | Status |
|-------|------|--------|
| 1 | [Core Loop](plans/phase-1-core-loop.md) | **Complete** |
| 2 | [World Graph](plans/phase-2-world-graph.md) | **In Progress** |
| 3 | [NPCs & Simulation](plans/phase-3-npcs-simulation.md) | Planned |
| 4 | [Persistence](plans/phase-4-persistence.md) | Planned |
| 5 | [Full LOD & Scale](plans/phase-5-full-lod-scale.md) | Planned |
| 6 | [Polish & Mythology](plans/phase-6-polish-mythology.md) | Planned |

---

## Design Documents

High-level architecture and detailed subsystem designs. Start with [Architecture Overview](design/overview.md) and follow links to subsystems.

| Document | Description | Related ADRs |
|----------|-------------|-------------|
| [Architecture Overview](design/overview.md) | Tech stack, core loop, module tree, LLM provider support | — |
| [Cognitive LOD](design/cognitive-lod.md) | 4-tier NPC simulation fidelity system | [ADR-002](adr/002-cognitive-lod-tiers.md) |
| [World & Geography](design/world-geography.md) | Location graph, real Irish geography, map data | [ADR-001](adr/001-graph-based-world.md), [ADR-009](adr/009-real-geography-fictional-people.md) |
| [Time System](design/time-system.md) | Day/night cycle, seasons, Irish festivals | [ADR-007](adr/007-time-scale-20min-day.md) |
| [Weather System](design/weather-system.md) | Weather states and simulation effects | — |
| [TUI Design](design/tui-design.md) | Layout, color palettes, terminal compatibility | — |
| [GUI Design](design/gui-design.md) | Windowed egui GUI with map, chat, and sidebars | — |
| [Player Input](design/player-input.md) | Natural language input, system commands | [ADR-006](adr/006-natural-language-input.md) |
| [Persistence](design/persistence.md) | WAL journal, snapshots, branching saves | [ADR-003](adr/003-sqlite-wal-persistence.md), [ADR-004](adr/004-git-like-branching-saves.md) |
| [NPC System](design/npc-system.md) | Entity model, context construction, gossip | [ADR-008](adr/008-structured-json-llm-output.md) |
| [Inference Pipeline](design/inference-pipeline.md) | LLM integration, queue, model selection | [ADR-005](adr/005-ollama-local-inference.md), [ADR-010](adr/010-prompt-injection-defenses.md) |
| [Debug System](design/debug-system.md) | Debug commands, live TUI panel, metrics (feature-gated) | — |
| [Testing Harness](design/testing.md) | GameTestHarness, script mode, query APIs | — |
| [Geo-Tool](design/geo-tool.md) | OSM geographic data conversion tool | [ADR-011](adr/011-geo-tool-osm-pipeline.md) |
| [Mythology Hooks](design/mythology-hooks.md) | Future hooks for Irish mythology layer | — |

## Architecture Decision Records (ADRs)

Key decisions with rationale and alternatives considered. See [ADR Index](adr/README.md) for the full list and template.

| ADR | Decision | Status |
|-----|----------|--------|
| [001](adr/001-graph-based-world.md) | Graph-based world (not coordinate grid) | Accepted |
| [002](adr/002-cognitive-lod-tiers.md) | 4-tier cognitive level-of-detail system | Accepted |
| [003](adr/003-sqlite-wal-persistence.md) | SQLite WAL for persistence | Accepted |
| [004](adr/004-git-like-branching-saves.md) | Git-like branching save system | Accepted |
| [005](adr/005-ollama-local-inference.md) | Ollama for local LLM inference | Accepted |
| [006](adr/006-natural-language-input.md) | Natural language input via LLM | Accepted |
| [007](adr/007-time-scale-20min-day.md) | 20 real minutes = 1 game day | Accepted |
| [008](adr/008-structured-json-llm-output.md) | Structured JSON output from LLM | Accepted |
| [009](adr/009-real-geography-fictional-people.md) | Real Irish geography, fictional people | Accepted |
| [010](adr/010-prompt-injection-defenses.md) | 5-layer prompt injection defense strategy | Accepted |
| [011](adr/011-geo-tool-osm-pipeline.md) | Geo-tool OSM pipeline for automated world generation | Accepted |

## Requirements & Status

| Document | Description |
|----------|-------------|
| [Roadmap](requirements/roadmap.md) | All 6 phases with per-item status checkboxes |
| [Open Questions](plans/open-questions.md) | 7 deferred design decisions — all resolved |

## Implementation Plans

Detailed, implementation-ready plans for each development phase.

| Plan | Phase | Status |
|------|-------|--------|
| [Phase 1: Core Loop](plans/phase-1-core-loop.md) | Core game loop, TUI, single NPC | **Complete** |
| [Phase 2: World Graph](plans/phase-2-world-graph.md) | Location graph, movement, OSM data | **In Progress** |
| [Phase 3: NPCs & Simulation](plans/phase-3-npcs-simulation.md) | Multiple NPCs, schedules, tiers 1-2 | Planned |
| [Phase 4: Persistence](plans/phase-4-persistence.md) | SQLite, journal, snapshots, branching | Planned |
| [Phase 5: Full LOD & Scale](plans/phase-5-full-lod-scale.md) | Tiers 3-4, weather, gossip, memory | Planned |
| [Phase 6: Polish & Mythology](plans/phase-6-polish-mythology.md) | Commands UI, mythology data hooks | Planned |

## Getting Started

| Document | Description |
|----------|-------------|
| [Windows Setup](windows-setup.md) | Native Windows setup, terminal tips, troubleshooting |

## Development

| Document | Description |
|----------|-------------|
| [Development Journal](journal.md) | Cross-session notes, observations, recommendations |
| [Known Issues](known-issues.md) | Active bugs and UX issues |
| [Maybe Bad Ideas](maybe-bad-ideas.md) | Ideas under consideration — may or may not be worth pursuing |

## Reference

| Document | Description |
|----------|-------------|
| [CLAUDE.md](../CLAUDE.md) | Build commands, code style, gotchas, dependencies |
| [DESIGN.md](../DESIGN.md) | Original monolithic design document (archival — superseded by `docs/design/`) |
| [README.md](../README.md) | Project overview, quick start |
