> **Note**: This is the original monolithic design document, preserved for reference and git history.
> The canonical, maintained versions of each section now live in `docs/design/`.
> See [docs/index.md](docs/index.md) for the full documentation index.

# Rundale — An Irish Living World Text Adventure (on the Parish engine)

## Overview

A text-based interactive fiction game set in rural Ireland. The player spawns in a small parish near Roscommon, County Roscommon. The entire game world is the island of Ireland, built on real geography with fictional people and businesses.

The core innovation is a cognitive level-of-detail (LOD) system: NPCs near the player are driven by full LLM inference for rich, emergent behavior. Distant NPCs are simulated at progressively lower fidelity. The result is a living world where hundreds of NPCs have ongoing lives, relationships, and conversations — whether or not the player is watching.

**This is a prototype. No story or quest system yet. The goal is to get the simulation loop, TUI, movement, NPC interaction, and persistence working end-to-end.**

-----

## Tech Stack

|Component    |Technology                              |Purpose                                           |
|-------------|----------------------------------------|--------------------------------------------------|
|Language     |**Rust**                                |Core game engine, simulation, TUI                 |
|Async Runtime|**Tokio**                               |Concurrent simulation tiers, async inference calls|
|TUI          |**Ratatui + Crossterm**                 |Terminal UI with 24-bit true color                |
|LLM Inference|**Ollama** (local, via REST API)        |NPC cognition, natural language parsing           |
|HTTP Client  |**Reqwest**                             |Communication with Ollama at `localhost:11434`    |
|Serialization|**Serde** (JSON)                        |World state, LLM structured output                |
|Persistence  |**SQLite** (via rusqlite)               |Save system, NPC memory, world events             |
|Entity System|**Bevy ECS** (standalone) or hand-rolled|World simulation data model                       |

### Hardware Assumptions

- **GPU**: RX 9070 16GB — dedicated to LLM inference via Ollama/ROCm
- **CPU**: Intel i9-13900KS — handles game logic, TUI rendering, background simulation on E-cores
- **Models**: Qwen3 14B for close-proximity NPCs, smaller model (8B/3B) for nearby tier

-----

## Architecture

### Core Loop

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

### Cognitive LOD Tiers

The simulation runs at four fidelity levels based on distance from the player:

#### Tier 1 — Immediate (GPU, 14B model)

- Full LLM inference per NPC interaction
- Rich dialogue, nuanced decisions, emotional responses, awareness of player actions
- Real-time, per-interaction inference
- Capacity: ~3-5 NPCs simultaneously
- Structured JSON output: `{"action": "...", "target": "...", "dialogue": "...", "mood": "...", "internal_thought": "..."}`

#### Tier 2 — Nearby (GPU, 8B or 3B model)

- Lighter inference, shorter prompts, summary-level reasoning
- NPCs interact with each other at reduced depth
- The player may overhear or learn about these interactions
- Capacity: ~10-20 NPCs
- Tick rate: every few game-minutes

#### Tier 3 — Distant (GPU, batch inference)

- Bulk tick: one LLM call covers many NPCs
- Prompt: "Here are 50 NPCs and their current states. Simulate N hours of activity. Return updated states."
- Broad strokes: relationships shift, resources change, major events occur
- Tick rate: every in-game day or two (~every few real-world minutes)

#### Tier 4 — Far Away (CPU only, no LLM)

- Pure rules engine, deterministic or lightly randomized state transitions
- Births, deaths, trade, seasonal changes, national-level events
- Runs on 13900KS E-cores — low priority, high parallelism
- Tick rate: once per in-game season (~every 30-45 real-world minutes)
- Events from this tier filter down as news/gossip through NPC conversations

### Tier Transitions

When a player moves toward a distant NPC, that NPC's sparse state must be "inflated" into a rich context for real-time interaction. This is a prompt engineering problem:

> "You are [name]. Here's your personality. Here's what you've been up to lately: [summary from distant tick]. The player just arrived. Continue naturally."

An event bus must propagate state changes across tier boundaries to maintain coherence (e.g., if a nearby NPC decides to betray a distant one).

-----

## World & Geography

### Map Source Data

The world is built on real Irish geography. All places are real. All people and businesses are fictional.

**Primary data source**: OpenStreetMap via Geofabrik Ireland extract

- Roads, buildings, waterways, railways, places, land use
- Filter to County Roscommon / target parish for starting area
- ODbL licensed (attribution required)
- Download: https://download.geofabrik.de/europe/ireland-and-northern-ireland.html

**Parish/townland boundaries**: Townlands.ie

- GeoJSON/Shapefile/CSV downloads
- Roscommon: 7 baronies, 62 civil parishes, 110 electoral divisions, 2,082 townlands
- Townland = fundamental unit of Irish rural land division (pre-Norman origin)
- Download: https://www.townlands.ie/page/download/

**Official boundaries**: Tailte Éireann (formerly Ordnance Survey Ireland)

- Civil parishes, townlands, counties, baronies
- CC-BY licensed
- Available in CSV, KML, Shapefile, GeoJSON (ITM projection)
- Portal: https://data-osi.opendata.arcgis.com/

**Historical reference** (for world-building, not direct data import):

- GeoHive historical OS maps (6-inch and 25-inch series): https://webapps.geohive.ie/mapviewer/index.html
- Down Survey maps (17th century): http://downsurvey.tcd.ie/down-survey-maps.php

### World Structure

The world is a **graph of location nodes**, not a continuous coordinate grid.

- **Nodes**: Named locations — the pub, the church, farms, crossroads, landmarks, the fairy fort
- **Edges**: Paths between nodes with traversal times in game-minutes (derived from real distances in OSM data)
- **Movement**: Natural language ("go to the pub", "walk to the church", "head down the boreen toward Lough Ree")
- **Traversal**: The world ticks forward while the player moves. A 10-minute walk means 10 game-minutes of simulation. Encounters may happen en route.
- **Resolution by distance**:
  - Starting parish: ~30-50 location nodes (dense, intimate)
  - Roscommon town: ~10 nodes (visitor-level detail)
  - Galway/Athlone: sparse
  - Dublin/Cork: ~5 nodes (you're a stranger here)

Each location has:

- Name (real place name)
- Description template (dynamically enriched by LLM based on time, weather, season, current events)
- Connections to other locations with traversal times
- Properties: indoor/outdoor, public/private
- Associated NPCs (home, workplace)

The map is a **static authored data file** (JSON or SQLite). Geography never changes. Only the people and events within it are dynamic.

### Disclaimer

> Any resemblance to real persons, living or dead, or actual businesses is purely coincidental. All characters and commercial establishments in this game are fictional.

-----

## Time System

### Day/Night Cycle

- **20 real-world minutes = 1 in-game day** (matches Minecraft pacing)
- Night portion: ~7-8 real minutes

### Seasons & Years

- **Target: 2-3 real-world hours = 1 in-game year**
- ~6-9 in-game days per season
- Each season lasts ~30-45 real-world minutes
- A full year is experienced in a single play session
- Multiple years of play show parish evolution: relationships deepen, people age, things change

### Irish Calendar Festivals

The four traditional Irish seasonal festivals map to the game's seasons:

- **Imbolc** (start of spring) — ~February 1
- **Bealtaine** (start of summer) — ~May 1
- **Lughnasa** (start of autumn) — ~August 1
- **Samhain** (start of winter) — ~November 1

These are potential moments where the mythological layer surfaces. Not scripted yet — but the temporal hooks should exist in the time system.

-----

## Weather System

Weather is a simulation driver, not just visual dressing:

- Rain keeps people indoors, changes encounter patterns
- Harsh winters strain resources, shift NPC conversations
- Beautiful evenings bring people outdoors
- Fog, overcast, storms affect atmosphere and NPC behavior
- Weather state is part of world state and affects NPC context prompts

-----

## TUI Design

### Layout

- **Top bar**: Location name, time of day (as a word, not a number — "late afternoon"), weather description, season, optional unicode weather/moon symbol
- **Main panel**: Text output — descriptions, dialogue, narration. This is where the game lives.
- **Bottom**: Input prompt. Subtle status line if core stats are needed later.

### Color System (24-bit True Color)

The TUI uses background and accent color gradients to represent time of day and weather. The player should feel time passing without being told explicitly.

- **Dawn**: Pale wash, soft yellows
- **Morning**: Warming golds
- **Midday**: Warm, bright tones
- **Afternoon**: Deepening golds
- **Dusk**: Deep blues, amber
- **Night**: Near-black, cold grey
- **Midnight**: Darkest palette

Weather modifies the palette:

- Overcast: Muted/desaturated
- Rain: Cooler tones, grey cast
- Fog: Heavily desaturated
- Clear: Full saturation

Color transitions should be **gradual**, not stepped.

### Terminal Compatibility

Target: kitty, alacritty, wezterm, Windows Terminal. All support 24-bit RGB.

-----

## Player Input & Command System

### Natural Language Input

The primary interaction is undecorated natural language text. The player just types and the game figures out intent via LLM parsing.

- "Go to the pub"
- "Tell Mary I saw her husband at the crossroads"
- "Look around"
- "Pick up the stone"

### System Commands

System commands use `/` prefix for now (placeholder — may change to a prefix-free autocomplete system later).

**Target UX (future)**: No prefix at all. The system detects exact/fuzzy matches against a small fixed command set and shows an inline confirmation prompt: "Quit the game? y/n". If the player says no, the input passes through to the game world. False positives are harmless because of the confirmation step.

#### Command List

|Command       |Description                                                            |
|--------------|-----------------------------------------------------------------------|
|`/pause`      |Freeze all simulation ticks, TUI stays up                              |
|`/resume`     |Unfreeze simulation                                                    |
|`/quit`       |Persist current state, clean shutdown                                  |
|`/save`       |Manual snapshot to current branch                                      |
|`/fork <name>`|Snapshot current state, create new named branch, continue on new branch|
|`/load <name>`|Load a branch head, resume from that point                             |
|`/branches`   |List all branches with timestamps and brief context                    |
|`/log`        |Show history of current branch (git log style)                         |
|`/status`     |Current branch name, in-game date, play time, NPC count by tier        |
|`/help`       |Show help reference                                                    |
|`/map`        |(Future) Simple ASCII parish layout                                    |

-----

## Persistence & Save System

### Philosophy

The player never thinks about saving. They quit whenever they want, load whenever they want, and the world is exactly where they left it. Saving is continuous and invisible, like Minecraft.

### Architecture: Write-Ahead Log

Three layers:

#### 1. Journal (Real-time)

- Every state mutation (NPC moved, relationship updated, dialogue happened, weather shifted) appended as it occurs
- Append-only, cheap writes
- This is the crash recovery net

#### 2. Snapshot (Periodic)

- Full compaction of current world state every ~30-60 seconds
- Runs on a background thread — no gameplay stutter
- This is the "clean" save point

#### 3. Branch (Named reference)

- A branch = a snapshot + its journal tail
- Fork copies the current snapshot and starts a new journal
- Load switches to a different snapshot and journal
- Each branch maintains its own independent clock — no time passes in unplayed branches

### Git-Like Branching Model

- **Journal** = working directory
- **Snapshot** = commit
- **Branch** = branch
- **Fork** = `git checkout -b`
- **Load** = `git checkout`
- Autosave on quit
- Background persistence thread on a dedicated CPU core

### Storage

SQLite in WAL mode. One database file per branch, or a single database with branch-tagged rows. The journal is the WAL.

-----

## NPC System

### Entity Data Model

Each NPC has:

- **Identity**: Name, age, physical description, occupation
- **Personality**: Traits, values, temperament (used as LLM system prompt)
- **Location**: Current node, home node, workplace node
- **Schedule**: Daily routine patterns (varies by day of week, season, weather)
- **Relationships**: Weighted edges to other NPCs (family, friend, rival, enemy, romantic, etc.)
- **Memory**:
  - Short-term: Last few interactions, current goals, immediate observations
  - Long-term: Key events, major relationship changes, grudges, secrets
  - Consider embedding-based retrieval for relevant long-term memories
- **Physical State**: Health, energy, hunger (if applicable)
- **Knowledge**: What they know about the world — public events, gossip, secrets

### NPC Context Construction

For each LLM inference call, build a context from:

1. System prompt: personality, backstory, current emotional state
1. Public knowledge: weather, time, season, major recent events
1. Personal knowledge: their relationships, recent experiences, secrets
1. Immediate situation: where they are, who's present, what just happened
1. Conversation history (if in dialogue)

### Gossip & Information Propagation

NPCs share information through conversation. A public event gets injected into the shared knowledge base. Private information (gossip, secrets) spreads through NPC-to-NPC interactions, potentially getting distorted. The player can learn about offscreen events through NPC dialogue organically.

### Structured Output Schema

All LLM responses for NPC behavior should be structured JSON:

```json
{
  "action": "speak|move|trade|work|rest|observe",
  "target": "player|npc_id|location|item",
  "dialogue": "What they say (if speaking)",
  "mood": "current emotional state",
  "internal_thought": "what they're actually thinking (hidden from player)",
  "knowledge_gained": ["any new information learned"],
  "relationship_changes": [{"npc_id": "...", "delta": 0.0}]
}
```

-----

## Inference Pipeline

```
Simulation Threads → Inference Queue (Tokio mpsc channel) → Inference Worker → Ollama REST API → Response Router → World State Update
```

- Inference queue accepts requests from any simulation tier
- A dedicated async task pulls requests, sends to Ollama, routes responses back
- Batch requests where possible (multiple Tier 2/3 NPCs in one call)
- Tiered model selection:
  - Tier 1 (direct interaction): Qwen3 14B
  - Tier 2 (nearby activity): Qwen3 8B or 3B
  - Tier 3 (distant batch): Qwen3 8B or 3B with bulk prompts
- Expected throughput with Qwen3 14B on RX 9070: ~30-50 tokens/sec
- At ~100-150 tokens per NPC response: ~3-5 NPC "thoughts" per second

### Player Input Parsing

Player natural language input is also sent to Ollama for intent parsing. The LLM maps free text to game actions:

```json
{
  "intent": "move|talk|look|interact|examine",
  "target": "location_id|npc_id|item_id",
  "dialogue": "what the player is saying (if talking)",
  "clarification_needed": false
}
```

If the LLM can't resolve intent, the game asks for clarification in-character.

-----

## Mythology Layer (Future — Hooks Only)

Irish mythology should have structural hooks in the prototype even if no content exists yet:

- The time system tracks festival dates (Samhain, Imbolc, Bealtaine, Lughnasa)
- Location nodes can have a `mythological_significance` property (fairy forts, holy wells, crossroads, bogs)
- The day/night cycle creates space: daytime = social simulation, nighttime = potential for "something else"
- NPC knowledge system can accommodate beliefs, superstitions, half-remembered stories

No mythological content or events for v1. Just ensure the data model doesn't preclude it.

-----

## Development Phases

### Phase 1 — Core Loop (Start Here)

1. Rust project scaffolding with Tokio, Ratatui, Reqwest, Serde, Rusqlite
1. Basic TUI with true color day/night cycle (just time passing, color shifting)
1. Single location, single NPC, Ollama integration
1. Player types natural language → LLM parses intent → NPC responds with structured JSON → response rendered as text
1. Basic world state in memory

### Phase 2 — World Graph

1. Parse OSM data for a small area near Roscommon into a location graph
1. Implement movement between nodes with traversal time
1. Time advances during movement
1. Multiple locations with descriptions

### Phase 3 — Multiple NPCs & Simulation

1. Multiple NPCs with schedules and locations
1. Tier 1 and Tier 2 cognitive LOD
1. NPCs interact with each other (Tier 2)
1. Basic relationship graph
1. NPC memory (short-term)

### Phase 4 — Persistence

1. Write-ahead log / journal system
1. Continuous autosave
1. Save/load/quit
1. Fork/branch system

### Phase 5 — Full LOD & Scale

1. Tier 3 and Tier 4 simulation
1. Expand world beyond starting parish
1. Weather system
1. Seasonal cycle with effects on NPCs
1. Gossip/information propagation
1. NPC long-term memory

### Phase 6 — Polish & Mythology Hooks

1. Help system
1. Map command
1. Status/log/branches UI
1. Mythological data model hooks
1. Night-time atmosphere differentiation

-----

## Open Questions (Deferred)

- Exact parish location near Roscommon (pick a real one)
- Player character — do they have stats? An inventory? A job? Or are they a blank observer?
- Goal/quest structure (deferred intentionally — get the sandbox working first)
- Story and lore
- Command prefix (currently `/`, may become prefix-free autocomplete)
- Mythology content and supernatural events
- What the player "does" — the verb set beyond movement and conversation
