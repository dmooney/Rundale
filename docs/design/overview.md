# Architecture Overview

> [Docs Index](../index.md)

## Project Overview

**Rundale** is a text-based interactive fiction game set in rural Ireland in the year 1820 — after the Acts of Union (1800) and before Catholic Emancipation (1829) or the Great Famine (1845). The player spawns in Kilteevan Village, in the parish of Kiltoom near Roscommon, County Roscommon. The entire game world is the island of Ireland, built on real geography with fictional people and businesses.

Rundale is built on the **Parish engine** — a generic Rust simulation framework. The engine knows nothing about any specific setting; all game-specific content lives in the `mods/rundale/` content package.

The game is committed to representing Irish people and culture with accuracy, respect, and sensitivity. Characters are portrayed with dignity and complexity. The historical setting reflects the real political and social landscape of early 19th-century Ireland.

The core innovation is a cognitive level-of-detail (LOD) system: NPCs near the player are driven by full LLM inference for rich, emergent behavior. Distant NPCs are simulated at progressively lower fidelity. The result is a living world where hundreds of NPCs have ongoing lives, relationships, and conversations — whether or not the player is watching.

**This is a prototype. No story or quest system yet. The goal is to get the simulation loop, movement, NPC interaction, and persistence working end-to-end.**

## Tech Stack

| Component     | Technology                               | Purpose                                            |
|---------------|------------------------------------------|----------------------------------------------------|
| Language      | **Rust**                                 | Core game engine, simulation                       |
| Async Runtime | **Tokio**                                | Concurrent simulation tiers, async inference calls |
| GUI           | **Tauri 2 + Svelte 5**                   | Desktop app with map, chat, and sidebars           |
| LLM Inference | **OpenAI-compatible API** (Ollama, LM Studio, OpenRouter, custom) | NPC cognition, natural language parsing |
| HTTP Client   | **Reqwest**                              | Communication with LLM provider via `/v1/chat/completions` |
| Serialization | **Serde** (JSON)                         | World state, LLM structured output                 |
| Persistence   | **SQLite** (via rusqlite)                | Save system, NPC memory, world events              |
| Entity System | Hand-rolled structs + manager pattern   | World simulation data model                        |

## Hardware Assumptions

- **GPU**: RX 9070 16GB — dedicated to LLM inference via Ollama/ROCm
- **CPU**: Intel i9-13900KS — handles game logic, background simulation on E-cores
- **Models**: Qwen3 14B for close-proximity NPCs, smaller model (8B/3B) for nearby tier

## Core Loop

```
Player Input → Command Detection → [System Command OR Game Input]
                                          ↓
                                   World State Context + NPC Context
                                   (relationships by name, scene history,
                                    witness memories, continuity cues)
                                          ↓
                                   Inference Queue (Tokio channel)
                                          ↓
                                   LLM Provider (OpenAI-compatible API)
                                          ↓
                                   Structured JSON Response
                                          ↓
                                   World State Update
                                   (mood, speaker memory, witness memories,
                                    conversation log recording)
                                          ↓
                                   Text Rendering → Headless REPL / GUI
```

## Engine / Game Data Separation (Mod System)

The Parish engine is generic and knows nothing about any specific setting. All Rundale game content (Irish place names, 1820 historical context, anachronism dictionary, festivals, loading phrases, system prompts) lives in a loadable data package called a "mod", inspired by Factorio's engine/base-game architecture.

A mod is a directory with a `mod.toml` manifest and data files:

```
mods/rundale/
├── mod.toml              # Manifest: name, version, start_date, start_location, period_year
├── world.json            # Location graph (from mods/rundale/world.json)
├── npcs.json             # NPC definitions (from mods/rundale/npcs.json)
├── prompts/
│   ├── tier1_system.txt  # Tier 1 system prompt template with {name}, {age}, etc.
│   ├── tier1_context.txt # Tier 1 context template
│   └── tier2_system.txt  # Tier 2 background simulation prompt
├── anachronisms.json     # Period enforcement dictionary
├── festivals.json        # Calendar events (Imbolc, Bealtaine, Lughnasa, Samhain)
├── encounters.json       # Encounter flavour text keyed by time-of-day
├── loading.toml          # Loading spinner frames, phrases, colours
├── ui.toml               # UI customisation: sidebar labels, accent colour
└── pronunciations.json   # Name pronunciation hints (optional)
```

The engine loads a `GameMod` at startup (via `--game-mod <dir>` or auto-detected from `mods/rundale/`) and passes it through the application:

- `WorldState::from_mod(&game_mod)` — loads world graph and start date from mod
- `LoadingAnimation::from_config(&game_mod.loading)` — configurable spinner
- `check_encounter_with_table()` — mod-provided encounter text
- `GameClock::check_festival_data(&game_mod.festivals)` — data-driven festivals
- `check_input_from_mod_data()` — loaded anachronism dictionary
- `interpolate_template()` — `{placeholder}` interpolation for prompt templates
- `get_ui_config` IPC command — sidebar labels and theme from mod
- `name_hints_for()` — contextual name pronunciation hints matched against NPCs and locations

See [Engine / Game Data Separation Plan](../plans/engine-game-data-separation.md) for the full design.

## Module Tree

```
src/
├── main.rs              # Entry point, CLI args (clap), mode routing (--game-mod flag)
├── lib.rs               # Module declarations
├── app.rs               # Core application state (App, ScrollState, GameMod)
├── error.rs             # ParishError (thiserror)
├── config.rs            # Provider configuration (TOML + env + CLI) + engine tuning
├── headless.rs          # Headless stdin/stdout REPL (default mode)
├── testing.rs           # GameTestHarness for automated script-based testing
├── debug.rs             # Debug commands and metrics (feature-gated)
├── input/
│   └── mod.rs           # Player input parsing, command detection, @mention extraction
├── world/
│   ├── mod.rs           # WorldState, Weather enum, location types
│   ├── graph.rs         # WorldGraph (adjacency list, BFS pathfinding, alias-aware name matching)
│   ├── time.rs          # GameClock, TimeOfDay, Season
│   ├── palette.rs       # Smooth color interpolation engine (time/season/weather)
│   ├── movement.rs      # Movement resolution, fuzzy destination matching
│   ├── encounter.rs     # En-route encounter system
│   └── description.rs   # Dynamic location description templates
├── npc/
│   ├── mod.rs           # Npc struct, NpcId, prompt builders
│   ├── types.rs         # Relationship, DailySchedule, NpcState, CogTier
│   ├── manager.rs       # NpcManager (tier assignment, tick dispatch)
│   ├── ticks.rs         # Tier 1 & 2 inference ticks, witness memories, response processing
│   ├── memory.rs        # ShortTermMemory (ring buffer), LongTermMemory (keyword retrieval)
│   ├── conversation.rs  # ConversationLog (per-location exchange history for scene awareness)
│   ├── overhear.rs      # Atmospheric overhear messages for nearby Tier 2
│   ├── gossip.rs        # GossipNetwork (probabilistic propagation)
│   ├── anachronism.rs   # Anachronism detection for player input (1820 period)
│   ├── mood.rs          # Mood-to-emoji mapping for NPC emotional state display
│   ├── reactions.rs     # Emoji reaction log for player feedback
│   ├── transitions.rs   # NPC tier transition summaries (inflate/deflate)
│   └── data.rs          # NPC data loader (JSON)
├── inference/
│   ├── mod.rs           # Inference queue, worker task
│   ├── openai_client.rs # OpenAI-compatible HTTP client (all providers)
│   ├── client.rs        # Ollama process management
│   └── setup.rs         # GPU detection, model selection, auto-pull (Ollama)
├── persistence/
│   ├── mod.rs           # Module root, re-exports
│   ├── database.rs      # Database + AsyncDatabase (SQLite WAL, schema, CRUD)
│   ├── snapshot.rs      # GameSnapshot, ClockSnapshot, NpcSnapshot
│   └── journal.rs       # WorldEvent enum, replay logic
├── gui/
│   ├── mod.rs           # ParishGui, eframe integration
│   ├── theme.rs         # Time-of-day color theming (smooth interpolation)
│   ├── chat_panel.rs    # Chat/dialogue display
│   ├── map_panel.rs     # Interactive parish map
│   ├── sidebar.rs       # Irish word pronunciation sidebar
│   ├── status_bar.rs    # Time, location, weather status
│   ├── input_field.rs   # Text input widget
│   └── screenshot.rs    # Automated screenshot capture
└── ../parish-geo-tool/  # OSM geographic data extraction tool (separate workspace crate)
    └── src/
        ├── main.rs      # CLI entry point
        ├── pipeline.rs  # End-to-end extraction pipeline
        ├── overpass.rs   # Overpass API queries
        ├── extract.rs   # OSM data extraction logic
        ├── osm_model.rs # OSM data types
        ├── connections.rs # Connection generation
        ├── descriptions.rs # Location description generation
        ├── lod.rs       # Level-of-detail assignment
        ├── merge.rs     # Data merging
        ├── cache.rs     # Query result caching
        └── output.rs    # JSON output formatting
```

## Subsystem Deep-Dives

- [Cognitive LOD](cognitive-lod.md) — Four-tier NPC simulation fidelity system
- [World & Geography](world-geography.md) — Location graph, real Irish geography, map data sources
- [Time System](time-system.md) — Day/night cycle, seasons, Irish calendar festivals
- [Weather System](weather-system.md) — Weather as simulation driver, effects on NPCs and atmosphere
- [GUI Design](gui-design.md) — Tauri 2 + Svelte 5 desktop GUI with map, chat panel, sidebars, and color theming
- [Player Input](player-input.md) — Natural language parsing, system commands
- [Persistence](persistence.md) — Save system, WAL journal, git-like branching
- [NPC System](npc-system.md) — Entity data model, context construction, gossip propagation
- [Inference Pipeline](inference-pipeline.md) — Ollama integration, queue architecture, throughput
- [Debug System](debug-system.md) — Debug commands, metrics collection (feature-gated)
- [Debug UI](debug-ui.md) — Tabbed debug panel for Tauri GUI (full game state inspector)
- [Mythology Hooks](mythology-hooks.md) — Future mythology layer data model hooks
- [parish-geo-tool](geo-tool.md) — OSM geographic data conversion pipeline
- [Testing Harness](testing.md) — GameTestHarness, script mode, automated regression testing

## Related

- [ADR Index](../adr/README.md) — Architecture decision records with rationale
- [Roadmap](../requirements/roadmap.md) — Phase status tracking
- [Implementation Plans](../plans/) — Detailed per-phase plans

## Multi-Provider LLM Support

The Parish engine supports any OpenAI-compatible LLM provider via the `/v1/chat/completions` API:

| Provider | Type | Notes |
|----------|------|-------|
| **Ollama** (default) | Local | Auto-start, GPU detection, model pulling |
| **LM Studio** | Local | Bring your own model |
| **OpenRouter** | Cloud | Access to Claude, GPT-4, Gemini, etc. Requires API key |
| **Custom** | Any | Any OpenAI-compatible endpoint |

### Configuration

Provider is configured via `parish.toml`, env vars, or CLI flags (later overrides earlier):

```toml
[provider]
name = "openrouter"
api_key = "sk-or-..."
model = "anthropic/claude-sonnet-4-20250514"
```

CLI: `--provider`, `--base-url`, `--api-key`, `--model`
Env: `PARISH_PROVIDER`, `PARISH_BASE_URL`, `PARISH_API_KEY`, `PARISH_MODEL`

### Per-Category Provider Routing

Each inference category can use a different provider, model, and endpoint. Categories without explicit overrides inherit from the base `[provider]` config:

| Category | Purpose | Default |
|---|---|---|
| **Dialogue** | Player-facing NPC conversation (Tier 1, streaming) | Base provider |
| **Simulation** | Background NPC group interactions (Tier 2, JSON) | Base provider |
| **Intent** | Player input parsing (JSON, low-latency) | Base provider |

Per-category overrides in TOML:

```toml
[provider]
name = "ollama"
model = "gemma4:e4b"

[provider.dialogue]
name = "openrouter"
base_url = "https://openrouter.ai/api"
api_key = "sk-or-..."
model = "anthropic/claude-sonnet-4-20250514"

[provider.simulation]
model = "gemma4:e4b"

[provider.intent]
model = "gemma4:e2b"
```

Per-category CLI flags: `--dialogue-provider`, `--dialogue-model`, `--simulation-model`, `--intent-model`, etc.
Per-category env vars: `PARISH_DIALOGUE_PROVIDER`, `PARISH_DIALOGUE_MODEL`, `PARISH_SIMULATION_MODEL`, `PARISH_INTENT_MODEL`, etc.

**Legacy support**: The `[cloud]` TOML section, `--cloud-*` CLI flags, and `PARISH_CLOUD_*` env vars still work and map to the dialogue category. Explicit `[provider.dialogue]` overrides take precedence over `[cloud]`.

Runtime commands: `/cloud`, `/cloud model <name>`, `/cloud key <key>`, `/cloud provider <name>`

The `InferenceClients` struct (in `src/inference/mod.rs`) routes requests via `dialogue_client()`, `simulation_client()`, and `intent_client()` methods, falling back to the base provider when no per-category override exists.

### Engine Configuration

Beyond provider settings, `parish.toml` supports an `[engine]` section for runtime tuning of engine parameters. All fields use `#[serde(default)]` so existing deployments work unchanged. See `parish.example.toml` for all available settings.

| Section | What it configures |
|---|---|
| `[engine.inference]` | Timeouts (request, streaming, reachability, download, loading) |
| `[engine.speeds]` | Speed presets (Slow/Normal/Fast/Fastest/Ludicrous) |
| `[engine.encounters]` | Per-time-of-day encounter probabilities |
| `[engine.npc]` | Memory capacity, holdback, truncation limits |
| `[engine.npc.cognitive_tiers]` | Tier distance thresholds, Tier 2 tick interval |
| `[engine.npc.relationship_labels]` | Relationship strength label thresholds |
| `[engine.palette]` | Contrast thresholds |

Config structs live in `crates/parish-core/src/config/engine.rs`.

### Ollama Bootstrap & GPU Detection (Default Path)

When using the Ollama provider (the default), the Parish engine runs a self-contained setup sequence (see `src/inference/setup.rs`):

1. **Detect Ollama** — checks if the `ollama` binary is on PATH
2. **Auto-install** — if missing, runs the official install script
3. **Start server** — spawns `ollama serve` if not already running; kills on exit
4. **Detect GPU/VRAM** — queries `nvidia-smi`, `rocm-smi`, or `sysctl hw.memsize` (Apple Silicon unified memory) for VRAM info
5. **Select model** — picks the best gemma4 tier for available VRAM / unified memory:
   - ≥25 GB → `gemma4:31b` (Tier 1, dense)
   - ≥17 GB → `gemma4:26b` (Tier 2, MoE — 4B active)
   - ≥11 GB → `gemma4:e4b` (Tier 3, edge 4.5B)
   - <11 GB or CPU-only → `gemma4:e2b` (Tier 4, edge 2.3B)
6. **Auto-pull** — downloads the model via Ollama's `/api/pull` if not already local

The `PARISH_MODEL` env var or `--model` CLI flag overrides auto-selection.

For non-Ollama providers, none of these steps run — the user provides the endpoint and model name directly.

## Headless Mode

Run `cargo run` for a plain stdin/stdout REPL. This is the default mode. Uses identical game logic (NPC inference, intent parsing, system commands). Useful for development testing and scripted interaction.

## Source Modules

- [`crates/parish-cli/src/main.rs`](../../src/main.rs) — Entry point, CLI parsing, mode routing
- [`src/lib.rs`](../../src/lib.rs)
- [`src/error.rs`](../../src/error.rs)
- [`crates/parish-cli/src/app.rs`](../../src/app.rs) — Core application state (App, ScrollState)
- [`crates/parish-cli/src/headless.rs`](../../src/headless.rs) — Headless REPL mode (default)
- [`src/world/`](../../src/world/)
- [`src/npc/`](../../src/npc/)
- [`src/inference/`](../../src/inference/) — Client, queue, setup/bootstrap
- [`src/persistence/`](../../src/persistence/)
- [`src/input/`](../../src/input/)

### Shared IPC Layer (`crates/parish-core/src/ipc/`)

All four backends (Tauri, web server, headless CLI, test harness) delegate shared
logic to the `parish_core::ipc` module. This avoids duplicating command handling,
streaming, and NPC conversation setup across backends.

- **`commands.rs`** — `handle_command()` processes ~30 system command variants,
  returning `CommandResult` (response text + `CommandEffect` side effects).
  Each backend dispatches effects through its own mechanism (Tauri events,
  EventBus, stdout, test assertions).
- **`config.rs`** — `GameConfig` struct holding mutable runtime configuration
  (provider, model, API key, cloud settings, per-category overrides).
  `resolve_category_client()` builds the correct `OpenAiClient` + model for any
  inference category, respecting per-category provider/key/URL overrides.
- **`handlers.rs`** — Pure functions: `snapshot_from_world`, `build_map_data`,
  `build_theme`, `build_npcs_here`, `build_travel_start`, `text_log`,
  `capitalize_first`, `prepare_npc_conversation` (includes anachronism checking),
  `compute_name_hints`, `mask_key`, `render_look_text`. Shared constants:
  `IDLE_MESSAGES`, `INFERENCE_FAILURE_MESSAGES` (Irish-themed canned fallbacks).
- **`streaming.rs`** — `stream_npc_tokens()` extracts the `dialogue` field
  incrementally from streaming JSON responses via `extract_dialogue_from_partial_json()`.
- **`types.rs`** — Serializable IPC types: `WorldSnapshot`, `MapData`, `NpcInfo`,
  `ThemePalette`, `TextLogPayload`, `StreamTokenPayload`, `StreamEndPayload`,
  `NpcReactionPayload`, `LoadingPayload` (with spinner/phrase/color animation),
  `TravelStartPayload`, `ReactRequest`, etc.

#### Backend parity

All backends share these features through core:
- NPC conversations with anachronism checking and pronunciation hints
- LLM-based intent parsing (local keywords first, LLM fallback for ambiguous input)
- Per-category inference client/model resolution (dialogue, simulation, intent, reaction)
- System command handling via `CommandEffect` dispatch
- Token streaming with incremental JSON dialogue extraction
- Loading animation (Celtic cross spinner + Irish phrases) including `/spinner` command
- Game clock pause/resume during inference (with world-update notification)
- Weather ticking, NPC schedule ticking, tier assignment
- Gossip propagation between co-located Tier 2 NPCs
- Autosave (periodic snapshots)
- Save/load/branch persistence commands
- `@mention` NPC targeting for multi-NPC locations
- Shared view helpers: `build_npcs_here`, `build_theme`, `render_look_text`

#### Intentional backend differences

- **Headless** uses `resolve_movement()` (older API) with custom arrival reactions
  including LLM-generated greetings; Tauri/server use `apply_movement()` with
  `MoveEffects`. Headless also prints NPC schedule events to stdout.
- **Debug commands** are CLI-only; GUI/web backends show a message.
- **Persistence** uses `spawn_blocking` in the server (async runtime constraint)
  but inline calls in Tauri/headless.
