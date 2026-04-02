# Parish — Feature List

Parish is a text-based adventure game set in 1820s rural Ireland, powered by LLM-driven NPCs with a cognitive level-of-detail simulation. Every NPC lives an ongoing life — working, gossiping, attending festivals — whether or not the player is watching.

---

## Game World

### Setting
- **Location:** Kiltoom parish, County Roscommon, Ireland (1820)
- **Historical context:** Post-Acts of Union (1800), pre-Catholic Emancipation (1829) and Great Famine (1845)
- **15 hand-authored locations** based on real Irish geography with lat/lon coordinates

### World Graph
- Graph-based location system with named connections between places
- BFS pathfinding for multi-hop travel
- Fuzzy name matching for movement commands (e.g. "go to the chapel" finds "St. John's Chapel")
- Traversal time varies by distance; in-game clock advances during travel
- Dynamic location descriptions using template interpolation (time, weather, season, NPCs present)

### Time System
- Continuous game clock: day/night cycle with 7 named periods (Dawn, Morning, Midday, Afternoon, Dusk, Night, Midnight)
- Four seasons (Spring, Summer, Autumn, Winter)
- Configurable game speed presets (adjustable in-game via `/set-speed`)
- Default time scale: 20 real minutes = 1 in-game day
- Pause and resume simulation (`/pause`, `/resume`)
- Manual time advancement (`/wait <minutes>`, `/tick`)

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
| **Tier 3** | Distant | Batch inference (planned) | 8-10 NPCs per LLM call, daily updates |
| **Tier 4** | Far away | CPU-only rules (planned) | Probabilistic life events, no LLM required |

### NPC Entity Model
- **Identity:** Name, age, occupation, personality traits
- **Schedule:** Time-of-day-driven movement between locations (e.g. farmer goes to fields in morning, pub in evening)
- **Relationships:** Named relationships between NPCs
- **Short-term memory:** 20-entry ring buffer of recent interactions and observations
- **Tier assignment:** Dynamic promotion/demotion based on player proximity

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
- Toggleable "improv craft" mode for NPC dialogue (`/toggle-improv`)
- Enhances NPC responses with theatrical improvisation techniques

---

## Player Input

### Natural Language
- Free-form text input parsed by LLM into structured intents
- **Intent types:** Move, Talk, Look, Interact, Examine, Unknown
- Local keyword matching for common actions (no LLM round-trip needed for simple movement/look commands)
- LLM fallback for complex or ambiguous intents

### Slash Commands

**Game Control:**
- `/pause` / `/resume` — Pause or resume the simulation
- `/quit` — Exit game
- `/new-game` — Start a fresh game
- `/status` — Show current game state
- `/time` — Display current in-game time
- `/npcs-here` — List NPCs at current location
- `/wait <minutes>` — Advance time
- `/tick` — Advance one simulation tick
- `/help` — Show available commands
- `/about` — Credits and version info

**Save/Load (Git-like branching):**
- `/save` — Create a manual snapshot
- `/fork <name>` — Create a named save branch
- `/load <name>` — Load a named branch
- `/branches` — List all save branches
- `/log` — Show save history

**Display:**
- `/map` — Toggle full map overlay
- `/toggle-sidebar` — Toggle Irish pronunciation sidebar
- `/toggle-improv` — Toggle improv craft mode
- `/show-speed` / `/set-speed <preset>` — View or change game speed

**Provider Configuration (10 commands):**
- `/show-provider` / `/set-provider <name>` — Base LLM provider
- `/show-model` / `/set-model <name>` — Model selection
- `/show-key` / `/set-key <key>` — API key
- `/show-cloud-provider` / `/set-cloud-provider` — Cloud provider
- `/show-cloud-model` / `/set-cloud-model` — Cloud model
- `/show-cloud-key` / `/set-cloud-key` — Cloud API key
- Per-category overrides: `/show-category-provider`, `/set-category-provider`, etc.

**Debug:**
- `/debug [subcommand]` — Debug operations and metrics
- `/spinner <seconds>` — Show loading spinner (testing)

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
10 LLM providers supported out of the box:

| Provider | Type |
|----------|------|
| **Ollama** | Local (default) |
| **LM Studio** | Local |
| **OpenRouter** | Cloud |
| **OpenAI** | Cloud |
| **Google Gemini** | Cloud |
| **Groq** | Cloud |
| **xAI (Grok)** | Cloud |
| **Mistral** | Cloud |
| **DeepSeek** | Cloud |
| **Together AI** | Cloud |
| **Custom** | User-provided endpoint |

### Inference Categories
Three independent inference categories, each independently configurable:
- **Dialogue** — NPC conversations with the player
- **Simulation** — World state updates and NPC behavior ticks
- **Intent** — Player input parsing and classification

### Configuration Resolution
Provider config resolves in priority order:
1. CLI flags (`--provider`, `--model`, `--api-key`, `--base-url`)
2. Environment variables (`PARISH_*` prefix)
3. TOML config file (`parish.toml`) with per-category overrides
4. Defaults (Ollama on localhost:11434)

### Streaming
- Token-by-token streaming of NPC responses
- Streaming cursor in the chat panel
- Input disabled during active streaming

---

## GUI (Tauri 2 + Svelte 5)

### Chat Panel
- Scrolling chat log with full conversation history
- Real-time NPC response streaming with animated cursor
- Celtic knot loading spinner with culturally themed phrases (25 mod-driven phrases like "Pondering the craic...", "Consulting the sheep...", "Muttering in Irish...")

### Status Bar
- Current location, in-game time, weather, season
- Active festival display
- Debug panel toggle

### Map
- **Minimap:** Player-centered SVG map showing neighboring locations (1-hop radius)
  - Smooth tweened panning (400ms, cubic-out easing)
  - Auto-zoom based on nearby location bounding box
  - Click-to-navigate on visible locations
- **Full map overlay:** Complete parish map with zoom and pan (toggled with M hotkey or `/map`)
- Fixed-scale Mercator projection from real lat/lon coordinates
- Label collision avoidance using force-directed repulsion

### Sidebar
- **NPCs Here:** Lists all NPCs at the player's current location
- **Focail (Irish Words):** Irish language pronunciation guide panel
- Toggleable via `/toggle-sidebar`

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
- Text input with enter-to-submit
- Auto-disabled during NPC streaming responses

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

### Default Mod: Kilteevan 1820
- 15 locations in Kiltoom parish with real geographic coordinates
- 8 NPCs with distinct personalities, occupations, and schedules
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
| **Script Testing** | `cargo run -- --script <file>` | Automated test harness for game behavior verification |

All modes share the same core game logic from `crates/parish-core/`.

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
