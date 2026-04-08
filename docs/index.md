# Parish Documentation Index

> Back to [README](../README.md) | [CLAUDE.md](../CLAUDE.md) (agent quick reference)

This is the documentation hub for Parish, an Irish Living World Text Adventure set in 1820. Start here to find any document.

## Project Status

**Phases 1–3 complete.** Next up: **Phase 4 — Persistence.**

See [Roadmap](requirements/roadmap.md) for detailed per-item status tracking.

| Phase | Name | Status | Key Design Docs |
|-------|------|--------|-----------------|
| 1 | [Core Loop](plans/phase-1-core-loop.md) | **Complete** | [Architecture](design/overview.md) |
| 2 | [World Graph](plans/phase-2-world-graph.md) | **Complete** | [Geography](design/world-geography.md), [Time](design/time-system.md) |
| 3 | [NPCs & Simulation](plans/phase-3-npcs-simulation.md) | **Complete** | [NPC System](design/npc-system.md), [Cognitive LOD](design/cognitive-lod.md) |
| 4 | [Persistence](plans/phase-4-persistence.md) | **Next** | [Persistence](design/persistence.md) |
| 5 | [Full LOD & Scale](plans/phase-5-full-lod-scale.md) | Planned | [Cognitive LOD](design/cognitive-lod.md), [Weather](design/weather-system.md) |
| 6 | [Polish & Mythology](plans/phase-6-polish-mythology.md) | Planned | [Mythology Hooks](design/mythology-hooks.md) |
| 7 | [Web & Mobile Apps](plans/phase-7-web-mobile.md) | Planned | [ADR-014](adr/014-web-mobile-architecture.md) |

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
| [GUI Design](design/gui-design.md) | Tauri 2 + Svelte 5 desktop GUI with map, chat, and sidebars | — |
| [Map Evolution](design/map-evolution.md) | Brainstorm: minimap, OSM tiles, fog of war, label fixes | RFC |
| [Player Input](design/player-input.md) | Natural language input, system commands, @mention targeting | [ADR-006](adr/006-natural-language-input.md) |
| [Input Enrichment Ideas](design/input-enrichment-ideas.md) | Brainstorm: slash autocomplete, emotes, history, whispers, reactions | RFC |
| [Persistence](design/persistence.md) | WAL journal, snapshots, branching saves | [ADR-003](adr/003-sqlite-wal-persistence.md), [ADR-004](adr/004-git-like-branching-saves.md) |
| [NPC System](design/npc-system.md) | Entity model, context construction, gossip | [ADR-008](adr/008-structured-json-llm-output.md) |
| [Inference Pipeline](design/inference-pipeline.md) | LLM integration, queue, model selection | [ADR-005](adr/005-ollama-local-inference.md), [ADR-010](adr/010-prompt-injection-defenses.md), [ADR-015](adr/015-per-category-inference-providers.md) |
| [Debug System](design/debug-system.md) | Debug commands, metrics (feature-gated) | — |
| [Debug UI](design/debug-ui.md) | Tabbed debug panel for Tauri GUI (state inspector) | — |
| [Testing Harness](design/testing.md) | GameTestHarness, script mode, query APIs | — |
| [Geo-Tool](design/geo-tool.md) | OSM geographic data conversion tool | [ADR-011](adr/011-geo-tool-osm-pipeline.md) |
| [Mythology Hooks](design/mythology-hooks.md) | Future hooks for Irish mythology layer | — |
| [Game Ideas Brainstorm](design/game-ideas-brainstorm.md) | 20 gameplay ideas across social, economic, mythology, and political themes | — |
| [Ambient Sound](design/ambient-sound.md) | Location-aware audio playback via rodio | [ADR-015](adr/015-ambient-sound-system.md) |

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
| [012](adr/012-documentation-hierarchy.md) | Hierarchical documentation organization | Accepted |
| [013](adr/013-cloud-llm-dialogue.md) | Cloud LLM for player dialogue | Accepted |
| [014](adr/014-web-mobile-architecture.md) | Web & mobile thin-client architecture | Accepted |
| [015](adr/015-ambient-sound-system.md) | Ambient sound system via rodio (GUI-only) | Accepted |
| [016](adr/016-tauri-svelte-gui.md) | Replace egui with Tauri 2 + Svelte GUI | Accepted |
| [017](adr/017-per-category-inference-providers.md) | Per-category inference providers | Accepted |

## Requirements & Status

| Document | Description |
|----------|-------------|
| [Roadmap](requirements/roadmap.md) | All 6 phases with per-item status checkboxes |
| [Open Questions](plans/open-questions.md) | 7 deferred design decisions — all resolved |

## Implementation Plans

Detailed, implementation-ready plans for each development phase.

| Plan | Phase | Status |
|------|-------|--------|
| [Phase 1: Core Loop](plans/phase-1-core-loop.md) | Core game loop, single NPC | **Complete** |
| [Phase 2: World Graph](plans/phase-2-world-graph.md) | Location graph, movement, encounters | **Complete** |
| [Phase 3: NPCs & Simulation](plans/phase-3-npcs-simulation.md) | Multiple NPCs, schedules, tiers 1-2 | **Complete** |
| [Phase 4: Persistence](plans/phase-4-persistence.md) | SQLite, journal, snapshots, branching | **Next** |
| [Phase 5: Full LOD & Scale](plans/phase-5-full-lod-scale.md) | Tiers 3-4, weather, gossip, memory | Planned |
| [Phase 6: Polish & Mythology](plans/phase-6-polish-mythology.md) | Commands UI, mythology data hooks | Planned |
| [Phase 7: Web & Mobile Apps](plans/phase-7-web-mobile.md) | Web (WASM) + Tauri mobile clients, game server | Planned |

## Getting Started

| Document | Description |
|----------|-------------|
| [macOS Setup](macos-setup.md) | Native macOS setup, Apple Silicon GPU, terminal tips |
| [Linux Setup](linux-setup.md) | Native Linux setup, NVIDIA/AMD GPU, headless screenshots |
| [Windows Setup](windows-setup.md) | Native Windows setup, terminal tips, troubleshooting |

## Development

| Document | Description |
|----------|-------------|
| [Development Journal](journal.md) | Cross-session notes, observations, recommendations |
| [Known Issues](known-issues.md) | Active bugs and UX issues |
| [First Contribution Guide](development/first-contribution-guide.md) | Newcomer-oriented architecture map and where to implement common changes |
| [Maybe Bad Ideas](maybe-bad-ideas.md) | Ideas under consideration — may or may not be worth pursuing |

## Research

Background research on 1820s Ireland informing world-building, NPC design, and game mechanics. See [Research Overview](research/README.md) for the full hub, cross-reference matrix, and suggested reading order.

### Core Society & People

| Document | Description |
|----------|-------------|
| [Irish Language](research/irish-language.md) | Bilingual landscape, dialects, code-switching, place-name anglicisation |
| [Demographics & Social Structure](research/demographics-social-structure.md) | Population, landlord-tenant hierarchy, religious demographics |
| [Family Life](research/family-life.md) | Household structure, matchmaking, inheritance, kinship networks |
| [Names & Naming Conventions](research/names-naming-conventions.md) | Gaelic surname system, patronymics, townland name meanings |

### Daily Life & Material Culture

| Document | Description |
|----------|-------------|
| [Culture & Daily Life](research/culture-daily-life.md) | Daily routines, hospitality, wakes, fairs, seasonal calendar |
| [Food & Drink](research/food-drink.md) | Potato dependency, poitín, hearth cooking, the butter trade |
| [Clothing & Textiles](research/clothing-textiles.md) | Frieze coats, red petticoats, homespun, linen/wool production |
| [Architecture & Housing](research/architecture-housing.md) | Cabins, farmhouses, Big Houses, building materials, the hearth |

### Economy & Work

| Document | Description |
|----------|-------------|
| [Economy & Trade](research/economy-trade.md) | Rent system, market towns, cottage industry, smuggling |
| [Farming & Agriculture](research/farming-agriculture.md) | Rundale system, spade cultivation, seasonal farming calendar |
| [Technology & Crafts](research/technology-crafts.md) | Blacksmithing, thatching, turf cutting, spinning, milling |
| [Transportation](research/transportation.md) | Walking, jaunting cars, stage coaches, canals, road conditions |

### Power & Institutions

| Document | Description |
|----------|-------------|
| [Law & Governance](research/law-governance.md) | Grand Jury system, magistrates, tithe system, policing |
| [Politics & Movements](research/politics-movements.md) | O'Connell, Catholic emancipation, Orange Order, memory of 1798 |
| [Crime & Secret Societies](research/crime-secret-societies.md) | Whiteboys, Ribbonmen, faction fighting, community vs crown justice |

### Spiritual & Intellectual Life

| Document | Description |
|----------|-------------|
| [Religion & Spirituality](research/religion-spirituality.md) | Catholic/Protestant dynamics, holy wells, folk-Catholic syncretism |
| [Mythology & Folklore](research/mythology-folklore.md) | Fairy faith, sídhe, seasonal festivals, the Otherworld |
| [Education & Literacy](research/education-literacy.md) | Hedge schools, oral tradition, literacy rates, scribal culture |
| [Music & Entertainment](research/music-entertainment.md) | Instruments, sean-nós, storytelling, crossroads dances, hurling |

### Health & Environment

| Document | Description |
|----------|-------------|
| [Medicine & Health](research/medicine-health.md) | Folk healers, holy well cures, disease, dispensary system |
| [Flora, Fauna & Landscape](research/flora-fauna-landscape.md) | Bogs, wildlife, seasonal changes, hedgerows, deforestation |

### Historical Context

| Document | Description |
|----------|-------------|
| [Recent History (Pre-1820)](research/recent-history-pre1820.md) | 1798 Rebellion, Act of Union, Napoleonic Wars, population explosion |
| [Forthcoming Decades](research/forthcoming-decades.md) | Catholic Emancipation, Great Famine, mass emigration — for foreshadowing |

## Claude Code Skills

Custom slash commands for common development workflows. Run these from any Claude Code session.

| Skill | Description |
|-------|-------------|
| `/check` | Run the full cargo quality gate — fmt, clippy, and tests |
| `/game-test [script]` | Run the GameTestHarness to verify game behavior |
| `/verify` | Full pre-push checklist (fmt + clippy + tests + harness) |
| `/screenshot` | Regenerate GUI screenshots after UI changes |
| `/fix-issue <number>` | Work through a GitHub issue end-to-end |

Skill definitions live in `.claude/skills/`.

## Reference

| Document | Description |
|----------|-------------|
| [CLAUDE.md](../CLAUDE.md) | Build commands, code style, gotchas, dependencies |
| [DESIGN.md](archive/DESIGN.md) | Original monolithic design document (archival — superseded by `docs/design/`) |
| [README.md](../README.md) | Project overview, quick start |
