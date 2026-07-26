# Rundale Documentation — Parish Engine

> Back to [README](../README.md) | [AGENTS.md](../AGENTS.md) / [CLAUDE.md](../CLAUDE.md) (agent quick reference)

This is the documentation hub for **Rundale**, an Irish Living World Text
Adventure set in 1820, built on the **Parish** engine. Start here to find the
right document. The tables below link the active and durable material; focused
subcollections keep their own indexes so large evidence corpora stay navigable.

## Project status

Rundale ships features across many subsystems in parallel rather than along a
single linear phase. The authoritative status view is the **feature-status
matrix** in the [Roadmap](requirements/roadmap.md).

Quick orientation: the core simulation (world graph, time/weather, cognitive
LOD tiers 1–4, NPC memory/gossip, branching persistence, natural-language
input), the Tauri + Svelte desktop GUI, the web server, per-category + cloud +
MLX inference, the Parish Designer editor, and the rundale-bench harness are all
shipped. Active design work centres on world expansion, the save/load UI,
mythology hooks, and dialogue-quality evals.

## How docs are organised

| Folder           | Contains                                                        | Status vocabulary                |
| ---------------- | --------------------------------------------------------------- | -------------------------------- |
| `design/`        | Durable subsystem reference — how a shipped/extant system works | Implemented · Partial            |
| `design/ideas/`  | Brainstorms, RFCs, speculative proposals                        | Brainstorm · Proposed            |
| `plans/`         | Active implementation plans                                     | In progress · Proposed · Planned |
| `plans/archive/` | Completed or historical plans (incl. linear phases)             | Complete                         |
| `adr/`           | Architecture Decision Records                                   | Accepted · Proposed              |

Every design/plan doc carries a `> Status: …` header. See
[ADR-024](adr/024-documentation-reorg-v2.md) for the organising rules.

---

## Design — durable subsystem reference

| Document                                                                     | Status                               | Related ADRs                                                                                                                                                                    |
| ---------------------------------------------------------------------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Architecture Overview](design/overview.md)                                  | Implemented                          | —                                                                                                                                                                               |
| [Cognitive LOD](design/cognitive-lod.md)                                     | Implemented                          | [002](adr/002-cognitive-lod-tiers.md)                                                                                                                                           |
| [World & Geography](design/world-geography.md)                               | Implemented                          | [001](adr/001-graph-based-world.md), [009](adr/009-real-geography-fictional-people.md)                                                                                          |
| [Time System](design/time-system.md)                                         | Implemented                          | [007](adr/007-time-scale-20min-day.md)                                                                                                                                          |
| [Weather System](design/weather-system.md)                                   | Implemented                          | —                                                                                                                                                                               |
| [NPC System](design/npc-system.md)                                           | Implemented                          | [008](adr/008-structured-json-llm-output.md), [018](adr/018-npc-intelligence-dimensions.md)                                                                                     |
| [Inference Pipeline](design/inference-pipeline.md)                           | Implemented                          | [005](adr/005-ollama-local-inference.md), [010](adr/010-prompt-injection-defenses.md), [013](adr/013-cloud-llm-dialogue.md), [017](adr/017-per-category-inference-providers.md) |
| [Player Input](design/player-input.md)                                       | Implemented                          | [006](adr/006-natural-language-input.md)                                                                                                                                        |
| [Persistence & Save System](design/persistence.md)                           | Implemented                          | [003](adr/003-sqlite-wal-persistence.md), [004](adr/004-git-like-branching-saves.md)                                                                                            |
| [GUI Design](design/gui-design.md)                                           | Implemented                          | [016](adr/016-tauri-svelte-gui.md)                                                                                                                                              |
| [Illustrated Notebook Real Play Screen](design/illustrated-notebook-real.md) | Retired historical experiment        | [Chat-first stabilization contract](../parish/apps/ui/CHAT_FIRST_STABILIZATION.md)                                                                                              |
| [Parish Notebook UI](design/parish-notebook-ui.md)                           | Proposed (earlier Svelte direction)  | [Illustrated Notebook plan](plans/illustrated-notebook-real.md)                                                                                                                 |
| [Godot-Based Rundale](design/godot-parish-game-plan.md)                      | Proposed (separate client direction) | [Interactive Parish Diorama](design/ideas/parish-diorama.md)                                                                                                                    |
| [Parish Designer (GUI editor)](design/designer-editor.md)                    | Implemented                          | —                                                                                                                                                                               |
| [Debug System](design/debug-system.md)                                       | Implemented                          | —                                                                                                                                                                               |
| [Debug UI](design/debug-ui.md)                                               | Implemented                          | —                                                                                                                                                                               |
| [Ambient Sound](design/ambient-sound.md)                                     | Implemented                          | [015](adr/015-ambient-sound-system.md)                                                                                                                                          |
| [Geo-Tool](design/geo-tool.md)                                               | Implemented                          | [011](adr/011-geo-tool-osm-pipeline.md)                                                                                                                                         |
| [Testing Harness](design/testing.md)                                         | Implemented                          | —                                                                                                                                                                               |
| [Scalable NPC Data Design](design/scalable-npc-data-design.md)               | Partial                              | —                                                                                                                                                                               |

**Design sub-collections** (each has its own index):

- [AI Techniques](design/ai-techniques/README.md) — 12 technique surveys (semantic memory/RAG, structured generation, drama manager, social simulation, …)
- [Input Enrichment](design/input-enrichment/README.md) — per-feature designs feeding the (completed) [implementation plan](plans/archive/input-enrichment-implementation.md)

## Visual client and graphics research

The visual work has three related but distinct tracks. The active default client
is the semantic chat-first shell with responsive DOM art. The retired Pixi
notebook, Diorama, and Godot documents remain historical or exploratory records.

| Need                                                                  | Start here                                                                         | Follow with                                                           |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Default visual play surface                                           | [Chat-first stabilization contract](../parish/apps/ui/CHAT_FIRST_STABILIZATION.md) | [GUI features](features.md#chat-first-illustrated-viewport)           |
| Concept art, exterior pipeline, interiors, portraits, or map evidence | [Graphics V2 research index](graphics-v2/README.md)                                | Its task-oriented links and scoped guidance                           |
| Runtime-composed visual scene system                                  | [Interactive Parish Diorama RFC](design/ideas/parish-diorama.md)                   | [Diorama implementation plan](plans/parish-diorama-implementation.md) |
| Separate Godot presentation client                                    | [Godot-Based Rundale plan](design/godot-parish-game-plan.md)                       | Treat as an exploratory alternative client                            |

## Design ideas / RFCs

Speculative and forward-looking — not (yet) committed work.

| Document                                                                                       | Status     |
| ---------------------------------------------------------------------------------------------- | ---------- |
| [Independent NPC Agents](design/independent-npc-agents.md)                                     | Proposed   |
| [Interactive Parish Diorama (runtime-composed scene graphics)](design/ideas/parish-diorama.md) | Proposed   |
| [RAG Lore Recall](design/ideas/rag-lore-recall.md)                                             | Proposed   |
| [NPC Sleep & Dream Consolidation](design/ideas/npc-sleep-dream-consolidation.md)               | Proposed   |
| [Mythology Layer (Future Hooks)](design/ideas/mythology-hooks.md)                              | Proposed   |
| [Visual Effects System](design/ideas/visual-effects-system.md)                                 | Proposed   |
| [Debt Shield](design/ideas/debt-shield.md)                                                     | Proposed   |
| [iOS Port (on-device)](design/ideas/ios-port.md)                                               | Proposed   |
| [Cloud Run Hosting](design/ideas/cloud-run-hosting.md)                                         | Proposed   |
| [Emotion-Driven Dialogue & Simulation](design/ideas/emotion-driven-dialogue-and-simulation.md) | Brainstorm |
| [Graphical World View (pixel scenes)](design/ideas/graphical-world-view.md)                    | Superseded |
| [Map Panel Evolution](design/ideas/map-evolution.md)                                           | Brainstorm |
| [Input Line Enrichment Ideas](design/ideas/input-enrichment-ideas.md)                          | Brainstorm |
| [NPC Prompt Immersion Ideas](design/ideas/npc-prompt-immersion-ideas.md)                       | Brainstorm |
| [Game Ideas Brainstorm](design/ideas/game-ideas-brainstorm.md)                                 | Brainstorm |
| [Game Mechanics Brainstorm](design/ideas/game-mechanics-brainstorm.md)                         | Brainstorm |
| [Music & Sound: Creative Vision](design/ideas/music-sound-brainstorm.md)                       | Brainstorm |
| [Night Visions](design/ideas/night-visions.md)                                                 | Parked     |

## Plans — active

| Plan                                                                                                     | Status      |
| -------------------------------------------------------------------------------------------------------- | ----------- |
| [Interactive Parish Diorama — Runtime Compositor Implementation](plans/parish-diorama-implementation.md) | Proposed    |
| [Illustrated Notebook Real Play Screen](plans/illustrated-notebook-real.md)                              | Retired     |
| [Illustrated Notebook UI Roadmap](plans/illustrated-notebook-roadmap.md)                                 | Closed      |
| [Phase 5F — World Graph Expansion](plans/phase-5f-world-expansion.md)                                    | Planned     |
| [Phase 6 — Polish & Mythology Hooks](plans/phase-6-polish-mythology.md)                                  | Planned     |
| [Phase 7 — Web & Mobile Apps](plans/phase-7-web-mobile.md)                                               | Partial     |
| [Save/Load UI Plan](plans/phase-9-save-load-ui.md)                                                       | Planned     |
| [Rundale-Bench](plans/rundale-bench.md)                                                                  | In progress |
| [LLM Quality Evals](plans/llm-quality-evals.md)                                                          | Proposed    |
| [Promptfoo Pentest](plans/promptfoo-pentest-plan.md)                                                     | Proposed    |
| [Gemma 4 Hiberno-English Training](plans/gemma4-rundale-training-plan.md)                                | Proposed    |
| [Talkie Methodology Port](plans/talkie-methodology-port.md)                                              | Proposed    |

## Supporting implementation records

These focused plans and design records document shipped infrastructure or an
earlier approach. Consult them when working in the named subsystem; they are
not the top-level product roadmap.

| Topic                            | Record                                                                                                                                                                               |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Bug-report flow                  | [design](design/bug-report-tool.md) · [plan](plans/bug-report-tool.md)                                                                                                               |
| Game quality harness             | [architecture](design/game-quality-harness-architecture.md) · [plan](plans/game-quality-harness.md)                                                                                  |
| Harness parity and ingest        | [mock/shadow design](design/harness-mock-shadow.md) · [plan](plans/harness-mock-shadow.md) · [ingest design](design/harness-skill-ingest.md) · [plan](plans/harness-skill-ingest.md) |
| NPC arrival greetings            | [design](design/npc-arrival-greetings.md) · [plan](plans/npc-arrival-greetings.md)                                                                                                   |
| MCP cold registration            | [design](design/parish-mcp-cold-register.md) · [plan](plans/parish-mcp-cold-register.md)                                                                                             |
| Earlier Svelte notebook proposal | [design](design/parish-notebook-ui.md) · [plan](plans/parish-notebook-ui.md)                                                                                                         |
| Dialogue turn seam               | [design record](design/1172-1173-dialogue-seam.md)                                                                                                                                   |
| IPC type contract                | [decision record](design/frontend-ipc-types.md)                                                                                                                                      |

## Plans — archived (complete / historical)

| Plan                                                                                            |                |
| ----------------------------------------------------------------------------------------------- | -------------- |
| [Phase 1 — Core Loop](plans/archive/phase-1-core-loop.md)                                       | Complete       |
| [Phase 2 — World Graph](plans/archive/phase-2-world-graph.md)                                   | Complete       |
| [Phase 3 — NPCs & Simulation](plans/archive/phase-3-npcs-simulation.md)                         | Complete       |
| [Phase 4 — Persistence](plans/archive/phase-4-persistence.md)                                   | Complete       |
| [Phase 5 — Full LOD & Scale](plans/archive/phase-5-full-lod-scale.md)                           | 5A–5E complete |
| [Phase 5A — Event Bus & Tier Transitions](plans/archive/phase-5a-event-bus-tier-transitions.md) | Complete       |
| [Phase 5B — Weather State Machine](plans/archive/phase-5b-weather-state-machine.md)             | Complete       |
| [Phase 5C — Memory & Gossip](plans/archive/phase-5c-memory-gossip.md)                           | Complete       |
| [Phase 5D — Tier 3 Batch Inference](plans/archive/phase-5d-tier3-batch-inference.md)            | Complete       |
| [Phase 5E — Tier 4 Seasonal Effects](plans/archive/phase-5e-tier4-seasonal-effects.md)          | Complete       |
| [Phase 8 — Tauri GUI Rewrite](plans/archive/phase-8-tauri-gui.md)                               | Complete       |
| [Engine / Game Data Separation](plans/archive/engine-game-data-separation.md)                   | Complete       |
| [Input Enrichment Implementation](plans/archive/input-enrichment-implementation.md)             | Complete       |
| [LLM Demo / Auto-Player Mode](plans/archive/demo-mode.md)                                       | Complete       |
| [Automated Chrome Test Plan](plans/archive/chrome-test-plan.md)                                 | Complete       |
| [Open Questions](plans/archive/open-questions.md)                                               | All resolved   |

## Architecture Decision Records (ADRs)

See the [ADR Index](adr/README.md) for the full table and template.

| ADR                                                       | Decision                                 | Status                    |
| --------------------------------------------------------- | ---------------------------------------- | ------------------------- |
| [001](adr/001-graph-based-world.md)                       | Graph-based world                        | Accepted                  |
| [002](adr/002-cognitive-lod-tiers.md)                     | 4-tier cognitive level-of-detail         | Accepted                  |
| [003](adr/003-sqlite-wal-persistence.md)                  | SQLite WAL persistence                   | Accepted                  |
| [004](adr/004-git-like-branching-saves.md)                | Git-like branching saves                 | Accepted                  |
| [005](adr/005-ollama-local-inference.md)                  | Ollama local inference                   | Accepted                  |
| [006](adr/006-natural-language-input.md)                  | Natural-language input                   | Accepted                  |
| [007](adr/007-time-scale-20min-day.md)                    | 20 real minutes = 1 game day             | Accepted                  |
| [008](adr/008-structured-json-llm-output.md)              | Structured JSON LLM output               | Accepted                  |
| [009](adr/009-real-geography-fictional-people.md)         | Real geography, fictional people         | Accepted                  |
| [010](adr/010-prompt-injection-defenses.md)               | Prompt-injection defenses                | Accepted                  |
| [011](adr/011-geo-tool-osm-pipeline.md)                   | parish-geo-tool OSM pipeline             | Accepted                  |
| [012](adr/012-documentation-hierarchy.md)                 | Hierarchical documentation organization  | Accepted (amended by 024) |
| [013](adr/013-cloud-llm-dialogue.md)                      | Cloud LLM for player dialogue            | Accepted                  |
| [014](adr/014-web-mobile-architecture.md)                 | Web & mobile thin-client architecture    | Accepted                  |
| [015](adr/015-ambient-sound-system.md)                    | Ambient sound system (rodio, GUI-only)   | Accepted                  |
| [016](adr/016-tauri-svelte-gui.md)                        | Replace egui with Tauri 2 + Svelte GUI   | Accepted                  |
| [017](adr/017-per-category-inference-providers.md)        | Per-category inference providers         | Accepted                  |
| [018](adr/018-npc-intelligence-dimensions.md)             | NPC multidimensional intelligence        | Accepted                  |
| [019](adr/019-json-structured-output-for-npc-dialogue.md) | JSON structured output for NPC dialogue  | Accepted                  |
| [020](adr/020-npc-tool-use.md)                            | NPC function-calling / tool-use output   | Proposed                  |
| [021](adr/021-npc-memory-retrieval.md)                    | Embedding-based NPC memory retrieval     | Proposed                  |
| [022](adr/022-engine-config-extraction.md)                | Extract engine tuning into configuration | Accepted                  |
| [023](adr/023-web-testing-server.md)                      | Web server mode for Chrome GUI testing   | Accepted                  |
| [024](adr/024-documentation-reorg-v2.md)                  | Documentation reorganization v2          | Accepted                  |

## Requirements & status

| Document                                          | Description                                               |
| ------------------------------------------------- | --------------------------------------------------------- |
| [Roadmap](requirements/roadmap.md)                | Feature-status matrix + historical phases (authoritative) |
| [Open Questions](plans/archive/open-questions.md) | Deferred design decisions — all resolved                  |

## Getting started

| Document                             | Description                                           |
| ------------------------------------ | ----------------------------------------------------- |
| [Setup Guide](setup.md)              | Platform-specific setup for macOS, Linux, and Windows |
| [Google OAuth Setup](oauth-setup.md) | Google credentials for the web server's sign-in flow  |

## Development

| Document                                                            | Description                                                     |
| ------------------------------------------------------------------- | --------------------------------------------------------------- |
| [Feature List](features.md)                                         | Player-facing and engine feature inventory                      |
| [Development Journal](archive/journal.md)                           | Cross-session notes, observations, recommendations (archived)   |
| [Known Issues](archive/known-issues.md)                             | Active bugs and UX issues (archived)                            |
| [First Contribution Guide](development/first-contribution-guide.md) | Newcomer architecture map and where to implement common changes |
| [Test Coverage Analysis](archive/test-coverage-analysis.md)         | Coverage snapshot and gaps (archived — from pre-workspace era)  |
| [Releasing Rundale](release.md)                                     | Tag-driven release process                                      |
| [Maybe Bad Ideas](maybe-bad-ideas.md)                               | Ideas under consideration — may or may not be worth pursuing    |
| [Repository Layout](repository-layout.md)                           | Top-level directory tree and crate index                        |
| [Troubleshooting](troubleshooting.md)                               | Bug reporting and inference-log artefact guide                  |

## Research

Background research on 1820s Ireland informing world-building, NPC design, and
game mechanics. See [Research Overview](research/README.md) for the full hub,
cross-reference matrix, and suggested reading order.

### Core Society & People

| Document                                                                     | Description                                                             |
| ---------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| [Irish Language](research/irish-language.md)                                 | Bilingual landscape, dialects, code-switching, place-name anglicisation |
| [Demographics & Social Structure](research/demographics-social-structure.md) | Population, landlord-tenant hierarchy, religious demographics           |
| [Family Life](research/family-life.md)                                       | Household structure, matchmaking, inheritance, kinship networks         |
| [Names & Naming Conventions](research/names-naming-conventions.md)           | Gaelic surname system, patronymics, townland name meanings              |

### Daily Life & Material Culture

| Document                                                   | Description                                                    |
| ---------------------------------------------------------- | -------------------------------------------------------------- |
| [Culture & Daily Life](research/culture-daily-life.md)     | Daily routines, hospitality, wakes, fairs, seasonal calendar   |
| [Food & Drink](research/food-drink.md)                     | Potato dependency, poitín, hearth cooking, the butter trade    |
| [Clothing & Textiles](research/clothing-textiles.md)       | Frieze coats, red petticoats, homespun, linen/wool production  |
| [Architecture & Housing](research/architecture-housing.md) | Cabins, farmhouses, Big Houses, building materials, the hearth |

### Economy & Work

| Document                                                 | Description                                                    |
| -------------------------------------------------------- | -------------------------------------------------------------- |
| [Economy & Trade](research/economy-trade.md)             | Rent system, market towns, cottage industry, smuggling         |
| [Farming & Agriculture](research/farming-agriculture.md) | Rundale system, spade cultivation, seasonal farming calendar   |
| [Technology & Crafts](research/technology-crafts.md)     | Blacksmithing, thatching, turf cutting, spinning, milling      |
| [Transportation](research/transportation.md)             | Walking, jaunting cars, stage coaches, canals, road conditions |

### Power & Institutions

| Document                                                       | Description                                                        |
| -------------------------------------------------------------- | ------------------------------------------------------------------ |
| [Law & Governance](research/law-governance.md)                 | Grand Jury system, magistrates, tithe system, policing             |
| [Politics & Movements](research/politics-movements.md)         | O'Connell, Catholic emancipation, Orange Order, memory of 1798     |
| [Crime & Secret Societies](research/crime-secret-societies.md) | Whiteboys, Ribbonmen, faction fighting, community vs crown justice |

### Spiritual & Intellectual Life

| Document                                                     | Description                                                        |
| ------------------------------------------------------------ | ------------------------------------------------------------------ |
| [Religion & Spirituality](research/religion-spirituality.md) | Catholic/Protestant dynamics, holy wells, folk-Catholic syncretism |
| [Mythology & Folklore](research/mythology-folklore.md)       | Fairy faith, sídhe, seasonal festivals, the Otherworld             |
| [Education & Literacy](research/education-literacy.md)       | Hedge schools, oral tradition, literacy rates, scribal culture     |
| [Music & Entertainment](research/music-entertainment.md)     | Instruments, sean-nós, storytelling, crossroads dances, hurling    |

### Health & Environment

| Document                                                      | Description                                                |
| ------------------------------------------------------------- | ---------------------------------------------------------- |
| [Medicine & Health](research/medicine-health.md)              | Folk healers, holy well cures, disease, dispensary system  |
| [Flora, Fauna & Landscape](research/flora-fauna-landscape.md) | Bogs, wildlife, seasonal changes, hedgerows, deforestation |

### Historical Context

| Document                                                                   | Description                                                              |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| [Recent History (Pre-1820)](research/recent-history-pre1820.md)            | 1798 Rebellion, Act of Union, Napoleonic Wars, population explosion      |
| [Forthcoming Decades](research/forthcoming-decades.md)                     | Catholic Emancipation, Great Famine, mass emigration — for foreshadowing |
| [Irish-English 1820s Resources](research/Irish-English-1820s-resources.md) | Primary-source corpus for the Hiberno-English dialogue fine-tune         |
| [Ambient Sound Sources](research/ambient-sound-sources.md)                 | Source list for the ambient audio system                                 |

## Agent & contributor reference

| Document                                              | Description                                                                   |
| ----------------------------------------------------- | ----------------------------------------------------------------------------- |
| [Agent Docs Hub](agent/README.md)                     | Build, architecture, code style, gotchas, harness, scaling, skills            |
| [AGENTS.md](../AGENTS.md) / [CLAUDE.md](../CLAUDE.md) | Top-level agent quick reference                                               |
| [README.md](../README.md)                             | Project overview, quick start                                                 |
| [DESIGN.md](archive/DESIGN.md)                        | Original monolithic design document (archival — superseded by `docs/design/`) |

### Agent skills

Custom slash commands for common development workflows.

| Skill                   | Description                                                                                                      |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `/check`                | Quality gates — `just check` (pre-commit) and `just verify` (pre-push)                                           |
| `/parish-engine [mode]` | Run the engine to observe behaviour — script harness, `prove`, `play`, `rubric`, `demo`, `browser`, `screenshot` |
| `/backlog <mode>`       | GitHub issue lifecycle — `triage`, `fix-one <issue#>`, or `drain`                                                |
| `/techdebt [path]`      | Technical-debt loop; `crate-audit` mode for crate-layout refactors                                               |

Skill definitions live in `.agents/skills/`, with `.claude/skills/` as a compatibility symlink.
