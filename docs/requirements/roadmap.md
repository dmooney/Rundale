# Rundale Roadmap

> [Docs Index](../index.md)
>
> Last updated: 2026-07-14

This is the authoritative status view for Rundale + the Parish engine. The
project no longer tracks a single linear phase pointer — it ships features
across many subsystems in parallel. The **feature-status matrix** below is the
source of truth; the historical linear phases are preserved at the bottom for
provenance.

## Portfolio tracking

The [event-driven improvement drain](../agent/improvement-drain.md) defines how
work is promoted, bounded, proven, and landed. GitHub Issues are the executable
source of truth. The retired illustrated-notebook experiment is preserved in
historical design records; current benchmark signal work is tracked by
[#1685](https://github.com/dmooney/Rundale/issues/1685).
Proposed roadmap capabilities remain `Later` until they have a funded outcome,
decision-complete acceptance criteria, a proof strategy, and an activation
trigger.

## Status legend

- **Implemented** — shipped and exercised in a running build
- **Partial** — core shipped; named follow-ups outstanding
- **In progress** — actively landing, incremental PRs
- **Proposed** — designed/agreed, no code yet
- **Planned** — queued, design may be sketch-level

## Feature-status matrix

| Subsystem / Feature                                 | Status      | Primary design doc                                                                    | ADR(s)                                                                                                        |
| --------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Graph-based world & geography                       | Implemented | [World & Geography](../design/world-geography.md)                                     | [001](../adr/001-graph-based-world.md), [009](../adr/009-real-geography-fictional-people.md)                  |
| World graph expansion (Roscommon/Athlone/Dublin)    | Planned     | —                                                                                     | —                                                                                                             |
| Time, day/night & seasons                           | Implemented | [Time System](../design/time-system.md)                                               | [007](../adr/007-time-scale-20min-day.md)                                                                     |
| Weather state machine                               | Implemented | [Weather System](../design/weather-system.md)                                         | —                                                                                                             |
| Cognitive LOD tiers 1–4                             | Implemented | [Cognitive LOD](../design/cognitive-lod.md)                                           | [002](../adr/002-cognitive-lod-tiers.md)                                                                      |
| NPC simulation (memory, gossip, witness, schedules) | Implemented | [NPC System](../design/npc-system.md)                                                 | [018](../adr/018-npc-intelligence-dimensions.md)                                                              |
| NPC dialogue — structured JSON output               | Implemented | [Inference Pipeline](../design/inference-pipeline.md)                                 | [008](../adr/008-structured-json-llm-output.md), [019](../adr/019-json-structured-output-for-npc-dialogue.md) |
| NPC dialogue — function-calling / tool use          | Proposed    | —                                                                                     | [020](../adr/020-npc-tool-use.md)                                                                             |
| NPC memory — embedding-based retrieval              | Proposed    | —                                                                                     | [021](../adr/021-npc-memory-retrieval.md)                                                                     |
| Natural-language player input                       | Implemented | [Player Input](../design/player-input.md)                                             | [006](../adr/006-natural-language-input.md)                                                                   |
| Persistence & git-like branching saves              | Implemented | [Persistence](../design/persistence.md)                                               | [003](../adr/003-sqlite-wal-persistence.md), [004](../adr/004-git-like-branching-saves.md)                    |
| Inference pipeline — local (Ollama)                 | Implemented | [Inference Pipeline](../design/inference-pipeline.md)                                 | [005](../adr/005-ollama-local-inference.md)                                                                   |
| Inference — cloud dialogue & per-category providers | Implemented | [Inference Pipeline](../design/inference-pipeline.md)                                 | [013](../adr/013-cloud-llm-dialogue.md), [017](../adr/017-per-category-inference-providers.md)                |
| Engine tuning via configuration                     | Implemented | —                                                                                     | [022](../adr/022-engine-config-extraction.md)                                                                 |
| Prompt-injection defenses                           | Implemented | [Inference Pipeline](../design/inference-pipeline.md)                                 | [010](../adr/010-prompt-injection-defenses.md)                                                                |
| Tauri 2 + Svelte 5 desktop GUI                      | Implemented | [GUI Design](../design/gui-design.md)                                                 | [016](../adr/016-tauri-svelte-gui.md)                                                                         |
| Web server mode (Chrome testing / axum)             | Implemented | —                                                                                     | [014](../adr/014-web-mobile-architecture.md), [023](../adr/023-web-testing-server.md)                         |
| Full web & mobile client                            | Partial     | [Phase 7 plan](../plans/phase-7-web-mobile.md)                                        | [014](../adr/014-web-mobile-architecture.md)                                                                  |
| Chat-first illustrated play surface                 | Implemented | [Chat-first stabilization contract](../../parish/apps/ui/CHAT_FIRST_STABILIZATION.md) | —                                                                                                             |
| Save / load UI (GUI)                                | In progress | [Save/Load UI plan](../plans/phase-9-save-load-ui.md)                                 | —                                                                                                             |
| Parish Designer (in-GUI data editor)                | Implemented | [Designer Editor](../design/designer-editor.md)                                       | —                                                                                                             |
| Debug system & debug UI                             | Implemented | [Debug System](../design/debug-system.md), [Debug UI](../design/debug-ui.md)          | —                                                                                                             |
| Ambient sound                                       | Implemented | [Ambient Sound](../design/ambient-sound.md)                                           | [015](../adr/015-ambient-sound-system.md)                                                                     |
| parish-geo-tool (OSM pipeline)                      | Implemented | [Geo-Tool](../design/geo-tool.md)                                                     | [011](../adr/011-geo-tool-osm-pipeline.md)                                                                    |
| Testing harness                                     | Implemented | [Testing Harness](../design/testing.md)                                               | —                                                                                                             |
| rundale-bench model-quality benchmark               | In progress | [Rundale-Bench plan](../plans/rundale-bench.md)                                       | —                                                                                                             |
| LLM-as-judge quality evals                          | Partial     | [LLM Quality Evals plan](../plans/llm-quality-evals.md)                               | —                                                                                                             |
| Promptfoo pentest harness                           | Proposed    | [Promptfoo Pentest plan](../plans/promptfoo-pentest-plan.md)                          | —                                                                                                             |
| Hiberno-English dialogue fine-tune                  | Proposed    | [Gemma 4 training plan](../plans/gemma4-rundale-training-plan.md)                     | [005](../adr/005-ollama-local-inference.md)                                                                   |
| LLM demo / auto-player mode                         | Implemented | [Demo mode plan](../plans/archive/demo-mode.md)                                       | —                                                                                                             |
| Mythology layer                                     | Proposed    | [Mythology Hooks](../design/ideas/mythology-hooks.md)                                 | —                                                                                                             |

## Historical phases

The project was originally organised as linear phases 1–9. Those plans are kept
under [`docs/plans/archive/`](../plans/archive/) for provenance; their status is
reflected in the matrix above.

| Phase                  | Plan                                                   | Outcome                                                                                                  |
| ---------------------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| 1 — Core Loop          | [archive](../plans/archive/phase-1-core-loop.md)       | Complete                                                                                                 |
| 2 — World Graph        | [archive](../plans/archive/phase-2-world-graph.md)     | Complete                                                                                                 |
| 3 — NPCs & Simulation  | [archive](../plans/archive/phase-3-npcs-simulation.md) | Complete                                                                                                 |
| 4 — Persistence        | [archive](../plans/archive/phase-4-persistence.md)     | Complete                                                                                                 |
| 5 — Full LOD & Scale   | [archive](../plans/archive/phase-5-full-lod-scale.md)  | 5A–5E complete; [5F](../plans/phase-5f-world-expansion.md) planned                                       |
| 6 — Polish & Mythology | [active](../plans/phase-6-polish-mythology.md)         | Planned                                                                                                  |
| 7 — Web & Mobile       | [active](../plans/phase-7-web-mobile.md)               | Partial — web server shipped; egui-WASM approach superseded by [ADR-016](../adr/016-tauri-svelte-gui.md) |
| 8 — Tauri GUI Rewrite  | [archive](../plans/archive/phase-8-tauri-gui.md)       | Complete                                                                                                 |
| 9 — Save/Load UI       | [active](../plans/phase-9-save-load-ui.md)             | In progress — coordinated Ledger surface                                                                 |

## Open questions

> [Detailed analysis](../plans/archive/open-questions.md) — **all resolved.**

Parish location (Kilteevan), player model (newcomer/"blow-in"), emergent goal
structure, mundane-surface lore with mythology hooks, `/`-prefix command UX,
behavioral mythology scope, and a phased player verb set — all decided. See the
archived analysis for context and trade-offs.
