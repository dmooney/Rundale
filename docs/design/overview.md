# Architecture Overview

> [Docs Index](../index.md)

## Project Overview

Parish is a text-based interactive fiction game set in rural Ireland. The player spawns in a small parish near Roscommon, County Roscommon. The entire game world is the island of Ireland, built on real geography with fictional people and businesses.

The core innovation is a cognitive level-of-detail (LOD) system: NPCs near the player are driven by full LLM inference for rich, emergent behavior. Distant NPCs are simulated at progressively lower fidelity. The result is a living world where hundreds of NPCs have ongoing lives, relationships, and conversations — whether or not the player is watching.

**This is a prototype. No story or quest system yet. The goal is to get the simulation loop, TUI, movement, NPC interaction, and persistence working end-to-end.**

## Tech Stack

| Component     | Technology                               | Purpose                                            |
|---------------|------------------------------------------|----------------------------------------------------|
| Language      | **Rust**                                 | Core game engine, simulation, TUI                  |
| Async Runtime | **Tokio**                                | Concurrent simulation tiers, async inference calls |
| TUI           | **Ratatui + Crossterm**                  | Terminal UI with 24-bit true color                 |
| LLM Inference | **Ollama** (local, via REST API)         | NPC cognition, natural language parsing            |
| HTTP Client   | **Reqwest**                              | Communication with Ollama at `localhost:11434`     |
| Serialization | **Serde** (JSON)                         | World state, LLM structured output                 |
| Persistence   | **SQLite** (via rusqlite)                | Save system, NPC memory, world events              |
| Entity System | **Bevy ECS** (standalone) or hand-rolled | World simulation data model                        |

## Hardware Assumptions

- **GPU**: RX 9070 16GB — dedicated to LLM inference via Ollama/ROCm
- **CPU**: Intel i9-13900KS — handles game logic, TUI rendering, background simulation on E-cores
- **Models**: Qwen3 14B for close-proximity NPCs, smaller model (8B/3B) for nearby tier

## Core Loop

```
Player Input → Command Detection → [System Command OR Game Input]
                                          ↓
                                   World State Context + NPC Context
                                          ↓
                                   Inference Queue (Tokio channel)
                                          ↓
                                   Ollama API (localhost:11434)
                                          ↓
                                   Structured JSON Response
                                          ↓
                                   World State Update
                                          ↓
                                   Text Rendering → TUI
```

## Module Tree

```
src/
├── main.rs              # Entry point, CLI args (clap), mode routing
├── lib.rs               # Module declarations
├── error.rs             # ParishError (thiserror)
├── headless.rs          # Headless stdin/stdout REPL for testing
├── tui/                 # Ratatui terminal UI
├── world/               # World state, location graph, time system
│   ├── mod.rs           # WorldState, Location, LocationId
│   ├── graph.rs         # WorldGraph, BFS, fuzzy search, validation
│   ├── movement.rs      # resolve_movement(), narration
│   ├── encounter.rs     # Probability-based random encounters
│   ├── description.rs   # Template interpolation for locations
│   └── time.rs          # GameClock, seasons, festivals
├── npc/                 # NPC data model, behavior, cognition tiers
│   ├── mod.rs           # Npc, NpcId, NpcAction, prompt builders
│   ├── manager.rs       # NpcManager, CogTier, tier assignment
│   ├── memory.rs        # ShortTermMemory ring buffer
│   ├── relationship.rs  # Relationship, RelationshipKind
│   ├── schedule.rs      # DailySchedule, NpcState, ScheduleEntry
│   ├── tier.rs          # Tier 1/2 tick functions, Tier2Event
│   └── overhear.rs      # Overhear mechanic for nearby events
├── inference/
│   ├── client.rs        # Ollama HTTP client, process management
│   ├── setup.rs         # GPU detection, model selection, auto-pull
│   └── mod.rs           # Inference queue, worker task
├── persistence/         # SQLite save/load, WAL journal
└── input/               # Player input parsing, command detection
```

## Subsystem Deep-Dives

- [Cognitive LOD](cognitive-lod.md) — Four-tier NPC simulation fidelity system
- [World & Geography](world-geography.md) — Location graph, real Irish geography, map data sources
- [Time System](time-system.md) — Day/night cycle, seasons, Irish calendar festivals
- [Weather System](weather-system.md) — Weather as simulation driver, effects on NPCs and atmosphere
- [TUI Design](tui-design.md) — Terminal UI layout, 24-bit true color palettes, time/weather visuals
- [Player Input](player-input.md) — Natural language parsing, system commands
- [Persistence](persistence.md) — Save system, WAL journal, git-like branching
- [NPC System](npc-system.md) — Entity data model, context construction, gossip propagation
- [Inference Pipeline](inference-pipeline.md) — Ollama integration, queue architecture, throughput
- [Debug System](debug-system.md) — Debug commands, live TUI panel, metrics collection (feature-gated)
- [Mythology Hooks](mythology-hooks.md) — Future mythology layer data model hooks

## Related

- [ADR Index](../adr/README.md)

## Ollama Bootstrap & GPU Detection

On startup, Parish runs a self-contained setup sequence (see `src/inference/setup.rs`):

1. **Detect Ollama** — checks if the `ollama` binary is on PATH
2. **Auto-install** — if missing, runs the official install script (`https://ollama.com/install.sh`) which auto-detects GPU vendor (CUDA/ROCm/CPU)
3. **Start server** — spawns `ollama serve` if not already running; kills on exit
4. **Detect GPU/VRAM** — queries `nvidia-smi` or `rocm-smi` for VRAM info; falls back to CPU-only
5. **Select model** — picks the best model for available VRAM:
   - ≥12GB → `qwen3:14b` (Tier 1)
   - ≥6GB → `qwen3:8b` (Tier 2)
   - ≥3GB → `qwen3:3b` (Tier 3)
   - <3GB/CPU → `qwen3:1.5b` (Tier 4)
6. **Auto-pull** — downloads the model via Ollama's `/api/pull` if not already local

The `PARISH_MODEL` env var or `--model` CLI flag overrides auto-selection.

## Headless Mode

Run `cargo run -- --headless` for a plain stdin/stdout REPL without the TUI. Uses identical game logic (NPC inference, intent parsing, system commands). Useful for development testing and scripted interaction.

## Source Modules

- [`src/main.rs`](../../src/main.rs) — Entry point, CLI parsing, mode routing
- [`src/lib.rs`](../../src/lib.rs)
- [`src/error.rs`](../../src/error.rs)
- [`src/headless.rs`](../../src/headless.rs) — Headless REPL mode
- [`src/tui/`](../../src/tui/)
- [`src/world/`](../../src/world/)
- [`src/npc/`](../../src/npc/)
- [`src/inference/`](../../src/inference/) — Client, queue, setup/bootstrap
- [`src/persistence/`](../../src/persistence/)
- [`src/input/`](../../src/input/)
