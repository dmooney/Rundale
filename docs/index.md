# Parish Documentation Index

> Back to [README](../README.md) | [CLAUDE.md](../CLAUDE.md) (agent quick reference)

This is the documentation hub for Parish, an Irish Living World Text Adventure. Start here to find any document.

## Project Status

**Current phase: Phase 1 — Core Loop**
See [Roadmap](requirements/roadmap.md) for detailed status tracking.

---

## Design Documents

High-level architecture and detailed subsystem designs. Extracted from the original [DESIGN.md](../DESIGN.md) and maintained as the canonical source.

| Document | Description |
|----------|-------------|
| [Architecture Overview](design/overview.md) | Tech stack, hardware, core loop, module tree |
| [Cognitive LOD](design/cognitive-lod.md) | 4-tier simulation fidelity system |
| [World & Geography](design/world-geography.md) | Map data sources, location graph, world structure |
| [Time System](design/time-system.md) | Day/night cycle, seasons, Irish festivals |
| [Weather System](design/weather-system.md) | Weather states and simulation effects |
| [TUI Design](design/tui-design.md) | Layout, color palettes, terminal compatibility |
| [Player Input](design/player-input.md) | Natural language input, system commands |
| [Persistence](design/persistence.md) | WAL journal, snapshots, branching saves |
| [NPC System](design/npc-system.md) | Entity model, context construction, gossip |
| [Inference Pipeline](design/inference-pipeline.md) | Ollama integration, queue, model selection |
| [Debug System](design/debug-system.md) | Debug commands, live TUI panel, metrics (feature-gated) |
| [Mythology Hooks](design/mythology-hooks.md) | Future hooks for Irish mythology layer |

## Architecture Decision Records (ADRs)

Key decisions with rationale and alternatives considered. See [ADR Index](adr/README.md) for the full list and template.

| ADR | Decision |
|-----|----------|
| [001](adr/001-graph-based-world.md) | Graph-based world (not coordinate grid) |
| [002](adr/002-cognitive-lod-tiers.md) | 4-tier cognitive level-of-detail system |
| [003](adr/003-sqlite-wal-persistence.md) | SQLite WAL for persistence |
| [004](adr/004-git-like-branching-saves.md) | Git-like branching save system |
| [005](adr/005-ollama-local-inference.md) | Ollama for local LLM inference |
| [006](adr/006-natural-language-input.md) | Natural language input via LLM |
| [007](adr/007-time-scale-20min-day.md) | 20 real minutes = 1 game day |
| [008](adr/008-structured-json-llm-output.md) | Structured JSON output from LLM |
| [009](adr/009-real-geography-fictional-people.md) | Real Irish geography, fictional people |
| [010](adr/010-prompt-injection-defenses.md) | 5-layer prompt injection defense strategy |

## Requirements & Status

| Document | Description |
|----------|-------------|
| [Roadmap](requirements/roadmap.md) | All 6 phases with per-item status checkboxes |

## Implementation Plans

Detailed, implementation-ready plans for each development phase.

| Plan | Phase | Status |
|------|-------|--------|
| [Phase 1: Core Loop](plans/phase-1-core-loop.md) | Core game loop, TUI, single NPC | Current |
| [Phase 2: World Graph](plans/phase-2-world-graph.md) | Location graph, movement, OSM data | Planned |
| [Phase 3: NPCs & Simulation](plans/phase-3-npcs-simulation.md) | Multiple NPCs, schedules, tiers 1-2 | Planned |
| [Phase 4: Persistence](plans/phase-4-persistence.md) | SQLite, journal, snapshots, branching | Planned |
| [Phase 5: Full LOD & Scale](plans/phase-5-full-lod-scale.md) | Tiers 3-4, weather, gossip, memory | Planned |
| [Phase 6: Polish & Mythology](plans/phase-6-polish-mythology.md) | Commands UI, mythology data hooks | Planned |
| [Open Questions](plans/open-questions.md) | Deferred decisions with analysis | Ongoing |

## Getting Started

| Document | Description |
|----------|-------------|
| [Windows Setup](windows-setup.md) | Native Windows setup, terminal tips, troubleshooting |

## Reference

| Document | Description |
|----------|-------------|
| [DESIGN.md](../DESIGN.md) | Original monolithic design document (archival) |
| [CLAUDE.md](../CLAUDE.md) | Build commands, code style, gotchas, dependencies |
| [Development Journal](journal.md) | Cross-session notes, observations, recommendations |
| [README.md](../README.md) | Project overview, quick start |
