# Parish — Feature List

Parish is a text-based adventure game set in 1820s rural Ireland, powered by LLM-driven NPCs with a cognitive level-of-detail simulation. Every NPC lives an ongoing life — working, gossiping, attending festivals — whether or not the player is watching.

---

## Game World

### Setting
- **Location:** Rural Ireland (1820) — default mod is Rundale, set in the Kiltoom/Roscommon area
- **Historical context:** Post-Acts of Union (1800), pre-Catholic Emancipation (1829) and Great Famine (1845)
- **22 hand-authored locations** based on real Irish geography with lat/lon coordinates

### World Graph
- Graph-based location system with named connections between places
- BFS pathfinding for multi-hop travel
- Fuzzy name matching for movement commands (e.g. "go to the chapel" finds "St. John's Chapel")
- Traversal time varies by distance; in-game clock advances during travel
- Dynamic location descriptions using template interpolation (time, weather, season, NPCs present)

### Time System
- Continuous game clock: day/night cycle with 7 named periods (Midnight, Dawn, Morning, Midday, Afternoon, Dusk, Night)
- Four seasons (Spring, Summer, Autumn, Winter)
- **Five game speed presets:** Slow (80 min/day), Normal (40 min/day, default), Fast (20 min/day), Fastest (10 min/day), Ludicrous (100 sec/day for testing)
- Pause and resume simulation (`/pause`, `/resume`)
- Manual time advancement (`/wait <minutes>`, `/tick`)

### Weather System
- **Seven weather states:** Clear, PartlyCloudy, Overcast, LightRain, HeavyRain, Fog, Storm (`crates/parish-types/src/ids.rs`)
- Weather transition engine runs in the simulation tick path
- Weather affects UI palette tinting (desaturation, brightness, color temperature)
- Weather influences en-route encounter probability

### Festivals
- Four traditional Irish calendar festivals, data-driven from mod files:
  - **Imbolc** (Feb 1) — Start of spring, feast of St. Brigid
  - **Bealtaine** (May 1) — Start of summer, bonfires lit on hilltops
  - **Lughnasa** (Aug 1) — Start of autumn, harvest festival
  - **Samhain** (Nov 1) — Start of winter, when the veil between worlds is thin
- Festivals display in the status bar and debug panel when active

### Travel Encounters
- Random en-route encounters during travel (~20% base probability per trip)
- Probability influenced by time of day (higher at dawn/morning, lower at night)
- Encounter flavour text varies by time period (dawn, morning, midday, afternoon, dusk, night, midnight)
- Encounters are data-driven from mod JSON files

---

## NPC System

### Cognitive Level-of-Detail (LOD)
Parish's core innovation: NPCs are simulated at different fidelity levels based on proximity to the player.

| Tier | Proximity | Method | Description |
|------|-----------|--------|-------------|
| **Tier 1** | Same location | Full LLM inference | Rich, contextual conversation with memory and personality |
| **Tier 2** | Nearby locations | Lighter LLM inference | Background activity, "overhear" mechanic |
| **Tier 3** | Distant | Batch inference | 8-10 NPCs per LLM call, daily updates |
| **Tier 4** | Far away | CPU-only rules engine | Probabilistic life events, no LLM required |

### NPC Entity Model
- **Identity:** Name, age, occupation, personality traits
- **Schedule:** Time-of-day-driven movement between locations (e.g. farmer goes to fields in morning, pub in evening), with optional home and workplace assignments
- **Short-term memory:** 20-entry ring buffer of recent interactions and observations
- **Tier assignment:** Dynamic promotion/demotion based on player proximity

### NPC Intelligence Profile
Every NPC has a 6-dimension intelligence profile (each rated 1–5) that shapes LLM prompt guidance and speech patterns:
- **Verbal** — Eloquence and vocabulary (high = precise word choice; low = simple phrasing)
- **Analytical** — Abstract reasoning (low = concrete thinking only)
- **Emotional** — Emotional perception (high = reads people like a book)
- **Practical** — Common sense and real-world skills
- **Wisdom** — Life experience and judgment
- **Creative** — Imagination and novel thinking

Profile dimensions are translated into behavioral directives and injected into the NPC's prompt.

### NPC Mood
- Real-time mood tracking with 20+ emoji states (anger, fear, joy, contemplation, etc.)
- Mood displayed alongside NPCs in the `/npcs` listing and debug panel
- Mood and relationships update from Tier 2 interactions

### Relationships
- **Seven relationship types:** Family, Friend, Neighbor, Rival, Enemy, Romantic, Professional
- **Strength scale:** -1.0 (hostile) to 1.0 (close), with configurable label thresholds
- Relationship history stored as an append-only event log with timestamps
- Strength visualized as bars in the debug panel

### Conversation
- Natural language conversation with any NPC at the player's location
- LLM-powered responses shaped by NPC personality, occupation, and context
- NPC token streaming — responses appear word-by-word in real time
- "Overhear" mechanic: nearby Tier 2 NPCs generate ambient background chatter

### Anachronism Detection
- Scans player input for words and concepts that post-date 1820
- Categories: Technology, Slang, Concepts, Materials, Measurements
- Word-boundary matching to minimize false positives
- Detected anachronisms are injected into the NPC's prompt so they respond in-period
- Both hardcoded dictionary and mod-driven `anachronisms.json`

### Improv Mode
- Toggleable "improv craft" mode for NPC dialogue (`/improv`)
- Enhances NPC responses with theatrical improvisation techniques

---

## Player Input

### Natural Language
- Free-form text input parsed by LLM into structured intents
- **Intent types:** Move, Talk, Look, Interact, Examine, Unknown
- Local keyword matching for common actions (no LLM round-trip needed for simple movement/look commands)
- LLM fallback for complex or ambiguous intents

### Slash Commands

Most configuration commands follow a **unified show/set pattern**: running the command with no argument shows the current value; running it with an argument sets it.

**Game Control:**
- `/pause` / `/resume` — Pause or resume the simulation
- `/quit` — Exit game
- `/new` — Start a fresh game
- `/status` — Show current game state
- `/time` — Display current in-game time
- `/where` — Show current location
- `/npcs` — List NPCs at current location (with mood emoji)
- `/wait [minutes]` — Advance time without moving
- `/tick` — Advance one simulation tick
- `/help` — Show available commands
- `/about` — Credits and version info

**Save/Load (Git-like branching):**
- `/save` — Create a manual snapshot
- `/fork [name]` — Create a named save branch
- `/load [name]` — Load a named branch
- `/branches` — List all save branches
- `/log` — Show save history

**Display:**
- `/map` — List available tile sources; `/map <id>` switches to the named tile source (gated on the `period-map-tiles` flag)
- `/designer` — Open the parish designer
- `/theme [arg]` — Show or set the UI theme
- `/irish` — Toggle the Focail (Irish pronunciation) sidebar
- `/improv` — Toggle improv craft mode for NPC dialogue
- `/speed [preset]` — Show or set game speed (`slow`, `normal`, `fast`, `fastest`, `ludicrous`)

**Feature Flags:**
- `/flags` — List all feature flags and their states
- `/flag list` — List flags (same as above)
- `/flag enable <name>` / `/flag disable <name>` — Toggle a specific flag

**Provider Configuration (base):**
- `/provider [name]` — Show or set the base LLM provider
- `/model [name]` — Show or set the base model
- `/key [value]` — Show or set the base API key

**Provider Configuration (cloud, legacy subcommand form):**
- `/cloud` — Show cloud provider config
- `/cloud provider [name]` — Show or set the cloud provider
- `/cloud model [name]` — Show or set the cloud model
- `/cloud key [value]` — Show or set the cloud API key

**Per-Category Overrides (dot notation):**
Categories are `dialogue`, `simulation`, or `intent`.
- `/provider.<category> [name]` — e.g. `/provider.dialogue openai`
- `/model.<category> [name]` — e.g. `/model.intent qwen3:3b`
- `/key.<category> [value]` — e.g. `/key.dialogue sk-...`

**Debug:**
- `/debug [subcommand]` — Debug operations and metrics
- `/spinner [seconds]` — Show loading spinner (testing; default 30s)

---

## Persistence

### SQLite Storage
- SQLite with WAL journaling for concurrent reads
- Append-only event journal (every game event logged)
- Periodic snapshot compaction (autosave every 45 seconds)

### Git-Like Branching Saves
- Named save branches that can be forked and loaded
- Full branch history with `/log`
- Branch DAG visualization in the GUI save picker
- Papers Please-style save picker UI (activated with F5)

---

## LLM / Inference

### Provider Support
13 LLM backends supported out of the box:

| Provider | Type |
|----------|------|
| **Simulator** | Offline (default) — generates nonsense locally, no network or model download |
| **Ollama** | Local |
| **LM Studio** | Local |
| **vLLM** | Local |
| **OpenRouter** | Cloud |
| **OpenAI** | Cloud |
| **Google Gemini** | Cloud |
| **Groq** | Cloud |
| **xAI (Grok)** | Cloud |
| **Mistral** | Cloud |
| **DeepSeek** | Cloud |
| **Together AI** | Cloud |
| **Custom** | User-provided OpenAI-compatible endpoint |

### Inference Categories
Four independent inference categories, each with its own provider/model/key override:
- **Dialogue** — NPC conversations with the player
- **Simulation** — World state updates and NPC behavior ticks
- **Intent** — Player input parsing and classification
- **Reaction** — NPC emote/mood reactions

Use dot-notation commands (e.g. `/provider.reaction openai`) or `PARISH_REACTION_*` env vars to route a specific category.

### Configuration Resolution
Provider config is resolved by `resolve_config` in `crates/parish-config/src/provider.rs`. Later layers override earlier ones:
1. Hardcoded defaults (default provider is **Simulator**; no network or API key required)
2. TOML config file (`parish.toml`) with per-category overrides
3. Environment variables (`PARISH_PROVIDER`, `PARISH_BASE_URL`, `PARISH_API_KEY`, `PARISH_MODEL`)
4. CLI flags (`--provider`, `--model`, `--api-key`, `--base-url`)

### Ollama Bootstrap
- Auto-starts `ollama serve` if not running; shuts down cleanly on exit
- Binary detection via PATH; auto-installs if missing
- **GPU detection** via `nvidia-smi` or `rocm-smi`
- **Automatic model selection by VRAM:**
  - ≥12 GB → `qwen3:14b`
  - ≥6 GB → `qwen3:8b`
  - ≥3 GB → `qwen3:3b`
  - <3 GB → `qwen3:1.5b`
- Auto-pulls models not already cached; warmup before gameplay begins

### Streaming
- Token-by-token streaming of NPC responses via an unbounded channel
- Streaming cursor in the chat panel
- Input auto-disabled during active streaming

### Inference Logging
- Ring buffer of recent LLM calls (configurable capacity, default 100)
- Logs prompt, response, model, timing, streaming flag, and error status
- Viewable in the Debug Panel's Inference tab

---

## GUI (Tauri 2 + Svelte 5)

### Chat Panel
- Scrolling chat log with full conversation history
- Speaker labels distinguishing player, NPC, and system messages
- **Emote parsing:** asterisk-wrapped text (`*nods slowly*`) renders as italic action text
- Real-time NPC response streaming with animated cursor (▋)
- Auto-scroll to bottom on new messages
- Celtic knot loading spinner with culturally themed phrases (25 mod-driven phrases like "Pondering the craic...", "Consulting the sheep...", "Muttering in Irish...")
- Spinner color cycles through mod-defined RGB palette during load

### Status Bar
- Current location, in-game time, weather, season
- Active festival display
- Debug panel toggle

### Map
- **Minimap:** Player-centered SVG map showing neighboring locations (1-hop radius)
  - Smooth tweened panning (400ms, cubic-out easing)
  - Auto-zoom based on nearby location bounding box
  - Click-to-navigate on visible locations
- **Full map overlay:** Complete parish map with zoom and pan (toggled with the M hotkey)
- **Tile sources:** `/map` lists configured tile sources; `/map <id>` switches to one (requires the `period-map-tiles` flag)
- Fixed-scale Mercator projection from real lat/lon coordinates
- Label collision avoidance using force-directed repulsion

### Sidebar
- **NPCs Here:** Lists all NPCs at the player's current location
- **Focail (Irish Words):** Irish language pronunciation guide panel
- Toggleable via `/irish`

### Theme System
- Time-of-day color theming with smooth RGB gradient interpolation
- Season-aware palette tinting
- Weather-variant tinting
- CSS custom properties driven by Rust theme-tick events
- Mod-configurable accent color

### Save Picker
- Papers Please-style interface (F5 hotkey)
- Branch DAG tree visualization with hierarchical layout
- Create, load, fork, and manage save branches visually
- Auto-zoom bounding box for branch tree viewport

### Debug Panel
- **5 tabs:** Overview, NPCs, World, Events, Inference
- **Overview:** Game clock, time of day, season, weather, speed, pause state, festival, location, tier summary (T1-T4 NPC counts and names)
- **NPCs:** Selectable NPC list with detailed view (age, occupation, personality, relationships, memory)
- **World:** World state inspection
- **Events:** Event log viewer
- **Inference:** LLM call monitoring

### Input Field
- Contenteditable multi-line input with enter-to-submit
- **@mention autocomplete:** type `@` to list NPCs at current location with tab/arrow navigation; mentions render as styled chips
- **Slash command autocomplete:** type `/` to see filtered command list
- **Input history:** localStorage-persisted, 50 entries, up/down arrow navigation
- **Quick travel buttons:** one-click navigation to adjacent locations
- Auto-disabled during NPC streaming responses
- Auto-refocus when streaming stops

---

## Mod System (Factorio-Style)

### Separation of Engine and Content
All game content is loaded from mod packages, keeping the engine generic.

### Mod Structure
```
mods/<mod-name>/
├── mod.toml              # Manifest (name, version, start date, start location, period year)
├── world.json            # World graph (locations, connections, coordinates)
├── npcs.json             # NPC definitions (identity, personality, schedule, relationships)
├── prompts/              # LLM prompt templates with {placeholder} interpolation
│   ├── tier1_system.txt  # Tier 1 system prompt
│   ├── tier1_context.txt # Tier 1 context template
│   └── tier2_system.txt  # Tier 2 system prompt
├── anachronisms.json     # Period-specific anachronism dictionary
├── festivals.json        # Calendar festivals with dates and descriptions
├── encounters.json       # Travel encounter text by time of day
├── loading.toml          # Spinner animation frames, colors, and loading phrases
├── ui.toml               # Sidebar labels, accent color
├── pronunciations.json   # Name pronunciation hints (Irish names to phonetic guides)
└── transport.toml        # Transport configuration
```

### Default Mod: Rundale
Shipped at `mods/rundale/` (`mod.toml` id: `rundale`, title: "Rundale", description: "Rural Ireland, 1820 — a living world of land, labour, and community").

- **22 locations** with real geographic coordinates
- **23 NPCs** with distinct personalities, occupations, and schedules
- 4 Irish festivals
- 7 time-of-day encounter variants
- 25 culturally themed loading phrases
- Irish name pronunciation guide

---

## Multiple Runtime Modes

| Mode | Command | Description |
|------|---------|-------------|
| **Tauri Desktop** | `cargo tauri dev` | Full GUI with Svelte frontend in native window |
| **Web Server** | `cargo run -- --web [port]` | Browser-based play via HTTP + WebSocket (default port 3001) |
| **Headless CLI** | `cargo run` | Terminal stdin/stdout REPL |
| **Script Testing** | `cargo run -- --script <file>` | JSON-output test harness for automated behavior verification |

All modes share the same core game logic from `crates/parish-core/`.

---

## Developer Tools

### Geo Tool
- Standalone OSM (OpenStreetMap) geographic data extraction tool
- Lives as its own crate at `crates/geo-tool/`
- Used to pin world locations to real-world coordinates and build the world graph

---

## Testing

### Automated Testing
- Rust unit tests across all crates (`cargo test`)
- Frontend component tests with Vitest + @testing-library/svelte (22 tests)
- E2E browser tests with Playwright (headless Chromium)
- Script-based game harness testing (`GameTestHarness`)
- 90%+ code coverage target (`cargo tarpaulin`)

### Screenshot Generation
- Automated GUI screenshots at 4 times of day (morning, midday, dusk, night)
- Playwright + headless Chromium with mocked Tauri IPC
- No X11 or display server required

---

## Technical Foundation

| Component | Technology |
|-----------|------------|
| Language | Rust |
| Async runtime | Tokio |
| Desktop GUI | Tauri 2 |
| Frontend | Svelte 5 + SvelteKit (static adapter) |
| HTTP client | reqwest |
| Database | SQLite (rusqlite, bundled) |
| Serialization | serde + serde_json |
| Error handling | thiserror (library) / anyhow (binary) |
| Logging | tracing |
| Time | chrono |
| Web server | axum |
| CLI parsing | clap |

---

## Implementation Status

### Fully Implemented
- **Phases 1–4 complete:** Core loop, world graph, NPC system with all four cognitive tiers dispatched, SQLite persistence with branching saves
- **Phase 8 in progress:** Tauri GUI rewrite with Svelte 5 frontend
- All 40+ slash commands
- Multi-provider LLM support with per-category routing
- Short-term NPC memory, relationships, mood, intelligence profiles
- Anachronism detection
- Interactive map with Mercator projection
- Irish mod system with data-driven content loading
- Token-level streaming inference

### Partially Implemented (Infrastructure Ready)
- Gossip propagation between NPCs (memory structures exist)
- Mythology hooks (data fields exist, no active effects)

### Planned (Future Phases)
- **Phase 5A:** Event bus, cognitive tier transitions with context inflation/deflation
- **Phase 5C:** Long-term memory with keyword retrieval; gossip network with distortion
- **Phase 5D:** Tier 3 batch inference (8–10 NPCs per call, daily)
- **Phase 5E:** Tier 4 rules engine — illness, death, birth, trade, seasonal overrides
- **Phase 5F:** World expansion — Roscommon town, Athlone, Dublin with inter-region travel
- **Phase 6:** Mythology layer (legends, fairy fort encounters), `/help` and ASCII `/map` commands
- **Phase 7:** Web and mobile clients with axum server and authentication
- **Audio system (designed, feature-gated):** ambient location sounds, distance attenuation, weather dampening
