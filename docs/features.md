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
- Fuzzy (Jaro-Winkler) name matching for movement commands (e.g. "go to the chapel" finds "St. John's Chapel")
- Traversal time varies by distance; in-game clock advances during travel
- Dynamic location descriptions using template interpolation (time, weather, season, NPCs present)
- **Hybrid geography:** locations can be real-world (geocoded from OSM), author-pinned, or fully fictional, with relative anchors that let fictional clusters subordinate to a real place. Each location carries a `geo_kind` field (`real` / `manual` / `fictional`).
- **Worn-paths map visualization:** per-edge traversal counts accumulate at runtime and drive a "worn paths" overlay on the map — heavily traveled routes appear more prominently than rarely used ones

### Time System

- Continuous game clock: day/night cycle with 7 named periods (Midnight, Dawn, Morning, Midday, Afternoon, Dusk, Night)
- Four seasons (Spring, Summer, Autumn, Winter)
- **Five game speed presets:** Slow (80 min/day), Normal (40 min/day, default), Fast (20 min/day), Fastest (10 min/day), Ludicrous (100 sec/day for testing)
- Pause and resume simulation (`/pause`, `/resume`)
- Manual time advancement (`/wait <minutes>`, `/tick`)

### Weather System

- **Seven weather states:** Clear, PartlyCloudy, Overcast, LightRain, HeavyRain, Fog, Storm (`crates/parish-types/src/ids.rs`)
- Weather transition engine runs in the simulation tick path
- **Adjacent-state-only transitions** — weather cannot jump from Clear directly to Storm; it must step through intermediate states
- **2-hour minimum dwell** — once a weather state is entered it persists for at least 2 in-game hours before any transition is considered
- **Season-biased probabilities** — transition likelihoods are weighted by the current season (e.g. Fog is more common in autumn, Storm more common in winter)
- Weather state available to NPC dialogue context
- **Weather-gated travel** — connections carry optional `hazard` tags (`flood`, `lakeshore`, `exposed`) that make paths impassable in a storm, slower in heavy rain, or treacherous in fog. The flooded ford refuses the player back; an alternate route is used where one exists (`crates/parish-world/src/movement.rs`). `/weather` shows the current weather; `/weather <name>` forces a state for testing.
- **NPCs seek shelter** — during heavy rain and storms, NPCs move indoors or to the nearest sheltered location

### Travel

- Per-edge travel time computed from lat/lon distance
- **Transport modes:** walk vs. horse/cart — configurable travel speeds per mode, surfaced into travel-time calculations
- **Travel encounters:** time-of-day-weighted en-route encounters with ~20% base probability modulated by time of day, mod-driven flavour text in `encounters.json` keyed by time period
- **Wayfarers** — traveling NPCs randomly encountered on roads during movement. Each encounter resolves through the wayfarer system (`parish-world/src/wayfarers.rs`): an encounter is selected, an enrichment prompt is built for the LLM, and the wayfarer's dialogue is integrated into the travel narrative. Seed-based for reproducibility.

### Festivals

- Four traditional Irish calendar festivals, data-driven from mod files:
  - **Imbolc** (Feb 1) — Start of spring, feast of St. Brigid
  - **Bealtaine** (May 1) — Start of summer, bonfires lit on hilltops
  - **Lughnasa** (Aug 1) — Start of autumn, harvest festival
  - **Samhain** (Nov 1) — Start of winter, when the veil between worlds is thin
- Festivals display in the Time & Weather and Debug notebook sheets when active
- **Relationship boosts** — festivals trigger positive mood shifts and relationship-strength increases across the NPC population
- **Narrative hooks** — active festivals are injected into NPC prompts, causing them to mention preparations, memories, or local lore tied to the celebration

### Mythology

- Locations carry a `mythological_significance` field that is surfaced into NPC prompts
- Reserved for future folklore systems (Phase 6); data fields exist, no active effects

---

## NPC System

### Cognitive Level-of-Detail (LOD)

Parish's core innovation: NPCs are simulated at different fidelity levels based on proximity to the player.

| Tier       | Proximity        | Method                | Description                                                                                            |
| ---------- | ---------------- | --------------------- | ------------------------------------------------------------------------------------------------------ |
| **Tier 1** | Same location    | Full LLM inference    | Rich, contextual conversation with memory and personality                                              |
| **Tier 2** | Nearby locations | Lighter LLM inference | Background activity, "overhear" mechanic, mood/relationship deltas every ~5 game-minutes within ~100 m |
| **Tier 3** | Distant          | Batch inference       | 10 NPCs per LLM call, daily updates, lowest-priority inference lane                                    |
| **Tier 4** | Far away         | CPU-only rules engine | Probabilistic life events (birth, death, illness, marriage, trade per season), no LLM required         |

### NPC Entity Model

- **Identity:** Name, age, occupation, personality traits
- **Schedule:** Time-of-day-driven movement between locations (e.g. farmer goes to fields in morning, pub in evening), with optional home and workplace assignments
- **Season-aware schedules:** hourly activity/location entries with per-season overrides — an NPC's routine can shift completely between summer and winter
- **Short-term memory:** 20-entry ring buffer of recent interactions and observations
- **Tier assignment:** Dynamic promotion/demotion based on player proximity; memory persists across tier deflation

### NPC Intelligence Profile

Every NPC has a 6-dimension intelligence profile (each rated 1–5) that shapes LLM prompt guidance and speech patterns:

- **Verbal** — Eloquence and vocabulary (high = precise word choice; low = simple phrasing)
- **Analytical** — Abstract reasoning (low = concrete thinking only)
- **Emotional** — Emotional perception (high = reads people like a book)
- **Practical** — Common sense and real-world skills
- **Wisdom** — Life experience and judgment
- **Creative** — Imagination and novel thinking

Profile dimensions are translated into behavioral directives and injected into the NPC's prompt, shaping how the NPC speaks, reasons about events, and reacts emotionally.

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

### Autonomous NPC Chains

- After a player turn, NPCs may chain up to three follow-on exchanges with one another
- Chains are driven by relationship strength and mood — close friends banter, rivals snipe, family members exchange practical news
- This creates the sensation that conversations continue independent of the player's participation

### Off-Screen Social Simulation

- NPCs interact with one another independent of the player's presence — the world moves forward whether the player is there to witness it or not
- Tier 2 and Tier 3 inference ticks resolve relationship events, mood shifts, and story beats between non-player characters
- Outcomes are persisted to world state, progress each NPC's personal story, and surface later as gossip the player can overhear

### Gossip Network

- **60% transmission probability** — when an NPC learns new information, each adjacent NPC has a 60% chance of receiving it
- **20% distortion per hop** — each relay introduces a cumulative distortion factor, so rumors mutate as they spread
- **Bystander propagation** — Tier 2 NPCs at the same location overhear conversations and propagate what they hear to their own contacts
- Gossip state is tracked in the debug panel's Gossip tab

### Death & The Banshee

NPCs can die through Tier 4 life events (illness, old age, accident). When death is scheduled, a **banshee herald** is triggered:

- Doom is scheduled with a configurable lead time (`DOOM_LEAD_TIME_HOURS`) during which the NPC remains alive
- Keening wails are emitted to the world text log during the dusk-to-dawn window on a random night within the lead-time period
- Players who are outdoors or near a window may hear the cry; the NPC themselves never hears it
- On the final tick, the NPC dies and an epitaph line is generated
- Integrated across all three runtimes (Tauri, server, headless)

Controlled by the `banshee` feature flag (default-on). NPCs carry a `banshee_heralded: bool` field to prevent double-triggering.

### Anachronism Detection

- Scans player input for words and concepts that post-date 1820
- Categories: Technology, Slang, Concepts, Materials, Measurements
- ~60-term registry with origin years and categories
- Word-boundary matching to minimize false positives
- Detected anachronisms are injected into the NPC's prompt so they respond in-period with authentic confusion
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

### Message Reactions

- Emoji palette available on messages, persisted with the save
- Reactions survive save/load cycles and appear in the conversation log

### Tab Completion

- Tab completion for known nouns (NPC names, location names, common verbs)
- Works in both GUI and headless CLI modes

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

Known engine flags (all **default-on**; disable to opt out):

- `period-map-tiles` — `/map <id>` tile-source switching.
- `local-inference-onboarding` — first-run wizard that downloads bundled
  vllm-mlx + Qwen weights on macOS, or routes to BYOK on other hosts.
  Disable to skip the wizard entirely and force startup to use
  whatever `PARISH_*` env vars / `parish.toml` already configure.
- `night-visions` (planned) — see `docs/design/ideas/night-visions.md`.
- `banshee` — banshee death-herald system: keening cries announce impending NPC death during dusk-to-dawn windows (see Death & The Banshee).

Flags documented in plans but not yet implemented:

- `inference-rejection-sampler` — planned (see `docs/plans/gemma4-rundale-training-plan.md`).
- `rundale-dialect-model` — planned (see `docs/plans/gemma4-rundale-training-plan.md`).

Opt-in engine flags (**default-off**; `/flag enable <name>` to turn on):

- `npc-idle-banter` — spontaneous NPC chatter triggered after
  `idle_banter_after_secs` of player silence. Opt in to let nearby NPCs
  start talking when the player is idle; player-initiated dialogue
  (`npc-llm-reactions`) is unaffected.
- `npc-arrival-greetings` — spontaneous NPC greetings on arrival. When the
  player moves into a populated location, present NPCs may greet, welcome,
  nod, or introduce themselves. **Default-off**: arrivals are silent unless
  opted in. Muting greetings does not strand NPCs as anonymous — they are
  still introduced by name on first conversation — and the background social
  simulation (gossip, mood, schedules) is unaffected; only the visible
  greeting is gated. Other unprompted-speech paths keep their own switches
  (`travel-encounters`, `banshee`, `npc-idle-banter`, `autonomous-npc-chain`).

**Provider Configuration (base):**

- `/provider [name]` — Show or set the base LLM provider
- `/model [name]` — Show or set the base model
- `/key [value]` — Show or set the base API key
- `/preset <name>` — Apply a pre-configured provider stack (e.g. `/preset nvidia-nim` loads a Nemotron 3 triple: Super 120B for Dialogue, Nano 30B for Intent, Nano 9B for Simulation). Presets are defined in provider TOMLs; new ones can be added by mods.

**Provider Configuration (cloud, legacy subcommand form):**

- `/cloud` — Show cloud provider config
- `/cloud provider [name]` — Show or set the cloud provider
- `/cloud model [name]` — Show or set the cloud model
- `/cloud key [value]` — Show or set the cloud API key

**Per-Category Overrides (dot notation):**
Categories are `dialogue`, `simulation`, `intent`, or `reaction`.

- `/provider.<category> [name]` — e.g. `/provider.dialogue openai`
- `/model.<category> [name]` — e.g. `/model.intent qwen3:3b`
- `/key.<category> [value]` — e.g. `/key.reaction sk-...`

**Debug:**

- `/debug [subcommand]` — Debug operations and metrics
- `/spinner [seconds]` — Show loading spinner (testing; default 30s)

---

## Persistence

### SQLite Storage

- SQLite with WAL journaling for concurrent reads — readers never block writers, so autosave can fire mid-conversation without hitching
- Three-table schema (`branches`, `snapshots`, `journal_events`)
- Periodic snapshot compaction (autosave every 45 seconds, configurable, plus manual `/save` and graceful-shutdown autosave on `/quit`)

### Append-Only Journal

- Append-only event journal records every game event alongside snapshots
- Enables deterministic replay from any snapshot and its subsequent events — the engine can reconstruct any past game state by loading a snapshot and reapplying the journal

### Cross-Process Save Lock

- File-based lock prevents two running instances from opening or writing to the same save concurrently, preventing corruption

### Git-Like Branching Saves

- Named save branches that can be forked and loaded
- Full branch history with `/log`
- Branch DAG visualization in the GUI save picker
- Papers Please-style save picker UI (activated with F5)
- `/fork <name>` creates a non-destructive branch from the current state; `/load` switches; `/branches` lists

---

## LLM / Inference

### Provider Support

15 LLM backends supported out of the box (out of 24 total providers available across built-in defaults and mod-loaded configurations):

| Provider          | Type              | Notes                                                                                                                                 |
| ----------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| **Simulator**     | Offline (default) | Generates nonsense locally, no network or model download                                                                              |
| **Ollama**        | Local             | Auto-start, auto-install, GPU detection, automatic model selection by VRAM                                                            |
| **LM Studio**     | Local             |                                                                                                                                       |
| **vLLM**          | Local             | vllm-mlx on Apple Silicon; current bundled dialogue profiles are experimental, not production-qualified                               |
| **OpenRouter**    | Cloud             |                                                                                                                                       |
| **OpenAI**        | Cloud             |                                                                                                                                       |
| **Anthropic**     | Cloud             | Native `/v1/messages` API — not the OpenAI-compatibility shim                                                                         |
| **Google Gemini** | Cloud             |                                                                                                                                       |
| **Groq**          | Cloud             |                                                                                                                                       |
| **xAI (Grok)**    | Cloud             |                                                                                                                                       |
| **Mistral**       | Cloud             |                                                                                                                                       |
| **DeepSeek**      | Cloud             |                                                                                                                                       |
| **Together AI**   | Cloud             |                                                                                                                                       |
| **NVIDIA NIM**    | Cloud             | OpenAI-compatible; ships with a Nemotron 3 Super 120B / Nemotron 3 Nano 30B / Nemotron Nano 9B preset triple via `/preset nvidia-nim` |
| **Custom**        | User-provided     | Any OpenAI-compatible endpoint                                                                                                        |

20 additional providers are available via mod-loaded configurations under `mods/` — including Cohere, GitHub Models, Qwen, Zhipu (智谱), Moonshot, Scaleway, SiliconFlow, Vercel AI, and more. Each ships as a `kind = "providers"` mod manifest with a single provider TOML file. See `mods/AGENTS.md` for the full list.

### Inference Categories

Four independent inference categories, each with its own provider/model/key override:

- **Dialogue** — NPC conversations with the player
- **Simulation** — World state updates and NPC behavior ticks
- **Intent** — Player input parsing and classification
- **Reaction** — NPC emote/mood reactions

Use dot-notation commands (e.g. `/provider.reaction openai`) or `PARISH_REACTION_*` env vars to route a specific category.

### Priority Queue

- **Three-lane priority queue** — Interactive (player dialogue) preempts Background (Tier 2) preempts Batch (Tier 3)
- A slow batch call cannot block the player's conversation; interactive requests jump to the head and run immediately

### Structured JSON Output

- NPC turns return structured JSON: `{mood, action, internal_thought, irish_words}`
- Partial JSON is recovered on truncation — if a streaming response is cut off, the parser extracts whatever valid fields it can
- Enables deterministic downstream processing of NPC state changes without fragile string parsing

### Streaming

- Token-by-token streaming of NPC responses via a bounded 1024-token channel with back-pressure — a slow consumer never OOMs the engine
- Streaming cursor in the chat panel
- Input auto-disabled during active streaming

### Configuration Resolution

Provider config is resolved by `resolve_config` in `crates/parish-config/src/provider.rs`. Later layers override earlier ones:

1. Hardcoded defaults (default provider is **Simulator**; no network or API key required)
2. TOML config file (`parish.toml`) with per-category overrides
3. Environment variables (`PARISH_PROVIDER`, `PARISH_BASE_URL`, `PARISH_API_KEY`, `PARISH_MODEL`)
4. CLI flags (`--provider`, `--model`, `--api-key`, `--base-url`)

### Reachability & Timeouts

- Per-environment configurable knobs for request timeout, streaming timeout, model-load timeout, and download timeout
- Prevent hung inference calls from blocking the game loop indefinitely

### Prompt-Injection Defense (ADR-010)

Five-layer defense against prompt injection:

1. **Role separation** — system prompt is immutable and never concatenated with user input
2. **Delimited input** — player input is wrapped in explicit boundary markers so the LLM cannot confuse it with instructions
3. **Input sanitisation** — special-character sequences are neutralized at the system boundary
4. **Strict output parsing** — only valid structured JSON fields are accepted; anything outside the schema is discarded
5. **Output filtering** — parsed output is filtered before display to remove any surviving instruction-like content

### Ollama Bootstrap

- Auto-starts `ollama serve` if not running; shuts down cleanly on exit
- Binary detection via PATH; auto-installs if missing
- **GPU detection** via `nvidia-smi`, `rocm-smi`, or `sysctl hw.memsize` (Apple Silicon unified memory)
- **Automatic model selection by VRAM** (`crates/parish-setup/src/model_select.rs`):
  - ≥25 GB → `gemma4:31b` (dense)
  - ≥17 GB → `gemma4:26b` (MoE, 4B active)
  - ≥11 GB → `gemma4:e4b` (edge, 4.5B effective)
  - <11 GB → `gemma4:e2b` (edge, 2.3B effective)
- Auto-pulls models not already cached; warmup before gameplay begins

### BYOK Setup / First-Run Onboarding

On first launch (or when no inference provider is configured), the engine presents a **SetupOverlay** fork screen:

- **Local inference path (experimental)** — downloads bundled Qwen2.5 weights for vllm-mlx on macOS, or auto-installs Ollama on Linux/Windows; setup shows that no current profile has passed the production dialogue gate
- **BYOK cloud path (recommended for dialogue)** — configure any supported cloud provider with your own API key

The BYOK flow is driven by two tools:

- `parish_setup_status` — reads current setup state: `{complete, provider, model, base_url, has_api_key, has_env_key}`
- `parish_setup_byok` — persists a provider config (provider id, API key, optional base URL / model override) and rebuilds the live inference worker

API keys are stored in the **OS keychain** via the `SecretStore` trait (using the `keyring` crate), never in plaintext config files. `parish.toml` explicitly excludes the `api_key` field — secrets and config are deliberately separated. The `local-inference-onboarding` feature flag (default-on) controls whether the wizard appears; disable to skip it entirely and force startup to use whatever `PARISH_*` env vars or `parish.toml` already configure.

In the GUI, the onboarding flow renders as a **SetupOverlay** component with a fork UX (`ByokOnboarding.svelte`, `LocalInferenceFork.svelte`), live weight-download progress with a triquetra spinner SVG, and streaming setup-message updates. In server mode, the equivalent HTTP endpoints are `/api/setup-status` and `/api/submit-byok`.

### Packaged macOS Bundle

For a shippable `.app` that runs with zero Python or vllm-mlx setup on the end user's machine:

- `just build-vllm-mlx-bundle` (~5 min, ~360 MB compressed) materializes a relocatable Python runtime at `parish/dist/vllm-mlx/python-runtime/` with vllm-mlx pip-installed via `python-build-standalone` — no venv (absolute paths would break when the bundle moves)
- `cargo tauri build` includes the bundle under `Rundale.app/Contents/Resources/vllm-mlx/python-runtime/`
- On first launch, the app detects the bundle, offers local inference as experimental, recommends BYOK for dialogue, and downloads Qwen2.5 weights with a live progress bar only when the user chooses local
- CI: `.github/workflows/build-vllm-mlx-bundle.yml` (manual trigger)

For dev iteration on `cargo tauri dev`, skip the bundle build — the runtime falls through to a `PATH`-installed vllm-mlx (`uv tool install vllm-mlx`).

### Inference Logging

- Ring buffer of recent LLM calls (configurable capacity, default 50)
- Logs prompt, response, model, timing, streaming flag, and error status
- Viewable in the Debug Panel's Inference tab

### Rate Limiting

- Outbound request throttling per provider client, gating every LLM call before it leaves the process (`crates/parish-providers/src/rate_limit.rs`)
- Token-bucket / GCRA quota via the `governor` crate — sustained `per_minute` rate plus a `burst` capacity
- Per-category overrides under `[engine.inference.rate_limits.*]` in `parish.toml` (`dialogue`, `simulation`, `intent`, `reaction`), resolved by `RateLimitConfig::for_category`; plus a `default` limit for the base client (`crates/parish-config/src/engine.rs`)
- Off by default — omitting the config (or setting `per_minute = 0`) leaves clients unthrottled, preserving existing behavior
- Both blocking (`acquire`) and non-blocking (`try_acquire`) entry points so callers can either queue or shed load

---

## GUI (Tauri 2 + Svelte 5)

### Chat-First Illustrated Viewport

- The default play view is semantic Svelte DOM: `StatusBar`, a responsive scene
  header, the readable `ChatPanel`, enriched `InputField`, `Sidebar`, and map
  context.
- Approved watercolor plates, NPC portraits, and selected icons are responsive
  DOM images; the retired Pixi/notebook renderer is not shipped.
- Desktop keeps map, nearby people, and language hints beside chat. Mobile keeps
  the transcript and input primary with explicit Map and People & Words controls.
- Transcript attribution, streaming, reactions, sticky scrolling, history,
  mentions, slash/model/location completion, and quick travel remain in the
  primary interaction model.

### Coordinated Surfaces

- **Map:** the full parish map, opened from the mobile/desktop control or M.
- **Ledger:** save/load branch picker, opened from the status bar or F5.
- **Debug:** eight diagnostic tabs, opened from Developer tools or F12.
- **Mod, Bug Report, and Shortcuts:** contained utility surfaces opened from
  Developer tools or their shortcuts.
- One presentation-neutral coordinator owns these surfaces. It prevents overlap,
  blocks dismissal of required mod selection, cancels in-flight bug preparation
  safely, and restores focus to the invoking control or player input.

### Map

- **Full coordinated map surface** — complete parish map with zoom and pan, custom SVG icons per location type, traversal-weighted edges, and click-to-travel (toggled with the M hotkey)
- **Animated travel** — when the player moves between locations, the map smoothly pans and zooms to the destination, interpolating both center and zoom level across the journey's duration so the post-travel view is already framed when the player arrives
- **Tile sources:** `/map` lists configured tile sources; `/map <id>` switches to one (requires the `period-map-tiles` flag)
- Fixed-scale Mercator projection from real lat/lon coordinates
- Label collision avoidance using force-directed repulsion

### Theme System

- **Three themes** selectable with `/theme`: cream/parchment (default), Solarized Light, Solarized Dark
- Driven by CSS custom properties and persisted in `localStorage` so reloads don't flash the wrong palette
- Time-of-day color theming applies smooth RGB gradient interpolation between the current time phase's palette and the next, driven by Rust theme-tick events that push CSS custom properties to the frontend. Transitions are continuous across the themed notebook-sheet internals as game-time advances through morning, afternoon, evening, and night.
- Mod-configurable accent color

### Save Picker

- Coordinated Ledger surface (F5 hotkey)
- Branch DAG tree visualization with hierarchical layout
- Create, load, fork, and manage save branches visually
- Auto-zoom bounding box for branch tree viewport

### Debug Panel

- **8 tabs:** Overview, NPCs, World, Weather, Gossip, Conversations, Events, Inference
- Contained in a coordinated utility surface, opened with F12 or Developer tools
- **Overview:** Game clock, time of day, season, weather, speed, pause state, festival, location, tier summary (T1-T4 NPC counts and names)
- **NPCs:** Selectable NPC list with detailed view (age, occupation, personality, relationships, memory)
- **World:** World state inspection
- **Weather:** Weather state machine details, transition history, season biases
- **Gossip:** Active gossip nodes, transmission chains, distortion levels
- **Conversations:** Full conversation log with structured JSON metadata per turn
- **Events:** Event log viewer
- **Inference:** LLM call monitoring

### Input Field

- A plain single-line **Player intent** control preserves the concept art's command-strip shape
- Enter submits the current intent; adjacent illustrated action stamps provide common actions
- The field remains editable during NPC streaming so a first keystroke can flush the stream and
  become the start of the next intent
- Tab moves through the notebook controls; visible focus and Enter activation mirror pointer use

### Demo / Auto-Play Mode

A hands-free auto-player that drives NPC conversations at a configurable pace:

- Invoke via `just demo <pause> <turns>` (e.g. `just demo 3 50` for 3-second pauses, 50 turns)
- The auto-player selects NPCs, issues `/talk` commands, and advances through conversation turns
- UI includes a `DemoBanner` and `DemoPanel` overlay showing current turn and remaining count
- Toggle the demo panel with F10; F11 retains its desktop fullscreen role
- Use cases: smoke testing, quality assessment, capturing proof transcripts

### Screenshot Capture

- Press **F2** to capture a screenshot of the current game view
- Screenshots are saved to the platform-appropriate pictures directory
- The MCP tool `parish_latest_screenshot` returns metadata (path, timestamp, size) for the most recent player-triggered capture
- Server endpoints: `/api/take-screenshot` and `/api/latest-screenshot`; agent-triggered capture reuses the latest verified screenshot with a warning when the desktop window cannot produce a fresh capture
- Automated screenshot capture via `--screenshot <dir>` flag: captures at four times of day for use in `just screenshots`

### Keyboard Shortcuts

- **F2** — capture screenshot
- **F5** — open the Ledger (save / load)
- **F10** — toggle the demo panel
- **F11** — toggle fullscreen in the desktop app
- **F12** — toggle the Debug notebook sheet
- **M** — toggle the full parish map
- **?** — show the keyboard-shortcuts sheet
- **Tab** — move through notebook controls
- **Enter** — activate a focused control or send the current intent
- **Esc** — close the active dismissible sheet or stop the demo

### Parish Designer (GUI Editor)

- Integrated GUI editor at the `/editor` route, accessible from both the Tauri desktop app and the web server (`PARISH_ENABLE_EDITOR=1`)
- Follows the mode-parity rule — every editor command is implemented once in `parish-core` and wired to both backends
- **Mod browser** — lists all mods under `mods/`, switch between them without restarting
- **NPC editor** — edit identity, six-axis intelligence (tunable via sliders), home/workplace (location picker, no id-memorizing), knowledge items, gossip seeds, and relationships with automatic bidirectional bookkeeping
- **Schedule timeline** — read-only 24-hour SVG band per season/day-type showing when each NPC is where
- **Location editor** — description templates with live placeholder preview (`{time}`, `{weather}`, `{npcs_present}`), lat/lon, indoor/public flags, and connection editing with enforced bidirectional edges
- **Cross-reference validator** — runs `WorldGraph::validate()` plus orphan NPC homes/workplaces, broken relationship targets, and schedule location refs; click any issue to jump to the field
- **Save inspector** — browse `.db` save files, branches, and snapshots; view deserialized world state; export a snapshot as a fixture JSON
- **Deterministic JSON writer** — stable key ordering and 2-space indentation on every save so `git diff` stays clean even after a no-op round-trip
- **Running-game isolation** — the editor operates on a fresh in-memory copy of mod files and never touches the live game session; a warning banner appears when the loaded mod matches the one being edited

### Accessibility

- ARIA-labelled controls and visible focus rings on all interactive elements
- Semantic HTML throughout the UI (landmark regions, heading hierarchy, form labels)
- WCAG-AA contrast compliance across all three theme variants
- Full keyboard navigation with Esc as a reliable dismiss action for optional sheets

### Ambient Sound

Location-based ambient audio with distance attenuation and weather dampening:

- Each location can specify ambient sounds (birdsong, rain, wind, village bustle, church bells) with per-audio volume and loop configuration
- Volume attenuates with distance from the player's current location
- Weather states (rain, storm) add dampening and overlay effects
- Playback via `rodio`; feature-gated for GUI-only (Tauri desktop app)
- See ADR-015 for the full design rationale

---

## Web Server

### Axum Backend

- `parish-server` crate (`crates/parish-server/`) serves the same Svelte UI over HTTP + WebSocket
- One isolated engine session per `parish_sid` cookie — each browser tab gets its own game instance
- Library crate plus a runnable binary with `--port PORT` flag (default 3001)
- **Tile proxy** at `/tiles/{*path}` with a 3-tier cache: user-local disk cache (configurable via `PARISH_TILE_CACHE_DIR`), bundled tiles shipped with the app (`PARISH_BUNDLED_TILES_DIR`), and upstream fetch from tile servers as the final fallback. Tiles are served as XYZ slippy-map fragments for MapLibre GL.

### Authentication

- **Cloudflare Access JWT** — validates CF Access tokens in production deployments
- **Google OAuth** — optional sign-in flow for non-Cloudflare environments
- **Loopback bypass** — local dev on `127.0.0.1` skips auth entirely
- **Fail-closed** — misconfigured auth always denies access rather than allowing it

### WebSocket Events

- World state updates pushed to the client in real time (location changes, time advances, weather shifts)
- Streaming inference tokens delivered word-by-word over the socket
- Theme changes and map source switches propagated instantly
- Connection resilience — per-session backpressure, reconnect loop avoidance with jittered retry, and admission control (rate limiting per connecting IP)

### Session & Persistence

- Per-session save isolation — game state lives under `<user-data>/saves/<session_id>/` and survives server restarts
- User-data root is platform-native: `~/Library/Application Support/Rundale` (macOS), `$XDG_DATA_HOME/rundale` (Linux), `%APPDATA%\Rundale` (Windows)
- Override with `PARISH_SAVES_DIR` (saves), `PARISH_TILE_CACHE_DIR` (tile cache), or `PARISH_USER_DATA_DIR` (root)

### Monitoring

- Prometheus-style `/metrics` endpoint exposing auth failures, active session counts, and inference call statistics

### Deploy Artifacts

- Multi-stage `Dockerfile` in `deploy/` — builds the Rust backend and Svelte frontend, then bakes them into a slim runtime image

---

## Thin HTTP Client (`parish-client`)

### Architecture

- Separate `parish` binary that talks to a running `parish-server` over HTTP — no engine in-process, no game state owned locally
- All game logic lives on the server; the client is a thin shell that serializes commands and renders responses

### Four Modes

| Mode            | Invocation               | Description                                                                      |
| --------------- | ------------------------ | -------------------------------------------------------------------------------- |
| **Single-shot** | `parish "<cmd>"`         | One command, formatted output, exits immediately                                 |
| **Script**      | `parish --script <file>` | Batch fixture execution from a `.txt` command file                               |
| **REPL**        | `parish` (no args)       | Interactive read-eval-print loop with history and tab completion                 |
| **JSON**        | `parish --json "<cmd>"`  | Raw `CommandResponse` JSON — suitable for piping into `jq` or automation scripts |

### Cookie Persistence

- The server's `parish_sid` cookie is saved to disk between runs
- Subsequent invocations automatically resume the same save branch without re-authentication

### Use Cases

- CI scripts and agent harnesses that drive a remote or local server
- Lightweight terminal play against a running server without booting the full engine
- Automation and pipeline integration via `--json` mode

---

## Mod System (Factorio-Style)

### Separation of Engine and Content

All game content is loaded from mod packages, keeping the engine generic. The same mod loads identically in Tauri, the web server, and the test harness — backend-agnostic loading ensures parity across all runtime modes.

### Mod Structure

```text
mods/<mod-name>/
├── mod.toml              # Manifest (name, version, start date, start location, period year)
├── world.json            # World graph (locations, connections, coordinates, geo_kind per location)
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
├── transport.toml        # Transport configuration
└── fonts/                # Typeface registry for UI customization (monospace for tabular output, body text for dialogue)
```

### world.json Location Fields

- `geo_kind` — `real` (geocoded from OSM), `manual` (author-pinned coordinates), or `fictional` (purely invented)
- Relative anchors let fictional clusters subordinate to a real place — a fictional cottage can be placed "2 km north-east of Kilteevan"
- `mythological_significance` field surfaced into NPC prompts
- Connection edges with prose descriptions and optional hazard tags

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

Parish ships as five binaries with one shared engine core:

| Binary          | Mode                                   | Has engine in-process?              | Description                                                                                            |
| --------------- | -------------------------------------- | ----------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `parish-tauri`  | `just run`, `--screenshot <dir>`       | yes                                 | Default desktop experience — full GUI in a native window; `--screenshot` captures at four times of day |
| `parish-engine` | `--headless`, `--script FILE`          | yes                                 | Single-process terminal REPL; `--script` drives the deterministic test harness                         |
| `parish-server` | `--port PORT` (`just web`)             | yes (one engine per cookie session) | Multi-user web server; serves the same Svelte UI over HTTP + WebSocket                                 |
| `parish-client` | single-shot / script / REPL / `--json` | no — thin shell                     | Drive a running `parish-server` over HTTP; lightweight terminal alternative                            |
| `parish-mcp`    | MCP server bridge                      | no — bridge                         | Expose `mcp__parish__*` tools to AI agents over HTTP to a running backend                              |

### Mode Parity Rule

Every gameplay feature behaves identically across Tauri, headless, and web. Shared orchestration lives in `parish-core`; entry-point crates contain only thin wiring. A given feature's behavior cannot diverge between runtimes.

### MCP Bridge

The desktop app (`parish-tauri`) can expose an in-process MCP bridge via `--mcp-port <N>`. This opens an Axum router on `127.0.0.1:<N>` that serves a subset of game endpoints (world snapshot, submit input, save/load, setup, screenshot) to the external `parish-mcp` binary. The bridge enforces mode parity — every Tauri IPC command that has an MCP counterpart is verified at compile time.

- `parish-mcp` connects to the bridge over HTTP and registers itself as an MCP server for AI agents (Claude Code, etc.)
- The same bridge is used by `parish/scripts/parish-mcp-backend.sh` for the headless server path
- Bridge endpoints: world snapshot, map, NPCs, save state, submit input, new game, save/load branch, setup status, setup BYOK, latest screenshot

---

## Developer Tools

### Geo Tool (`parish-geo-tool`)

- Standalone Overpass-API CLI that pulls real Irish features into `world.json` by named area or bounding box
- Cached responses, dry-run preview, hand-curated merge mode
- `realign-coords` utility for snapping coordinates to historical map positions
- Lives as its own crate at `crates/parish-geo-tool/`

### NPC Tool (`parish-npc-tool`)

- SQLite-backed NPC builder for bulk generation, querying, and editing
- Bulk-generate parish or county populations with seedable randomness and 1820s demographic weights
- Query/filter by parish, occupation, or tier; promote/demote tiers
- Edit moods, relationships, and intelligence profiles
- Batch-elaborate backstories with an LLM (pass an NPC set through inference for richer detail)
- Validate referential integrity across the entire NPC set
- Export and import JSON

### rundale-bench (LLM Benchmark)

A reproducible, self-contained LLM benchmark for evaluating model quality against Rundale's dialogue demands:

- **Four evaluation slices:** dialogue quality, intent classification, reaction generation, NPC simulation
- **Pinned-judge contract:** rubric prompts are SHA-256 hashed and verified before scoring to prevent drift
- **ELO pairwise scoring** (K=32 → K=16 after 50 matches per candidate, bootstrap 5/95 CI via 500 i.i.d. match resamples) alongside absolute multi-axis 0-10 scoring
- **Live leaderboard** at [dmooney.github.io/Rundale](https://dmooney.github.io/Rundale/) (v2 promptfoo site): ranked per-category scores with 95% bootstrap CIs, a quality-vs-cost efficiency frontier, cost tiers, and per-model drill-downs
- **Reproducible harness:** `rundale-bench/rundale_bench.py` single-entry orchestrator with a v1-dev dataset (155 prompts, growing to 1100)
- **Local MLX sweep:** `local_runner.py` spawns `mlx_lm.server` per candidate and runs the full bench, appending rows to the local leaderboard
- Status: v1-dev — structurally complete but dataset frozen tag (`v1.0`) waits for corpus growth and three independently-evaluated targets on the holdout split

### Script Harness

- `.txt` fixtures in `testing/fixtures/` drive the engine through scripted scenes
- Structured `ScriptResult` JSON output enables deterministic regression checking
- Run a single fixture: `just game-test-one <name>`
- Run all fixtures: `just game-test-all`
- List available scripts: `just game-test-list`

### Eval Rubrics & Baselines

- Snapshot `Vec<ScriptResult>` JSONs in `testing/evals/baselines/`
- Structural rubrics gate against empty look descriptions, frozen clocks, and anachronistic vocabulary
- Compare current engine output against known-good baselines

### Architecture Fitness Tests

- `crates/parish-core/tests/architecture_fitness.rs` mechanically enforces:
  - **Leaf-crate purity** — no `tauri`/`axum`/`tower`/`wry`/`tao` dependencies in shared crates
  - **CLI-vs-leaf duplication bans** — shared logic must live in a leaf crate, not duplicated in `parish-engine/`
  - **Orphaned-module detection** — source files on disk but not declared as `mod` are rejected
- Each failure prints a self-correcting hint

### Build System

- `justfile` with ~50 recipes grouping build, test, harness, lint, screenshots, deps, geo/NPC tooling, Ollama control, and local CI via `act`
- Witness-marker scan (`just witness-scan`) — rejects AI completion stubs (the usual `todo!` and ellipsis-comment patterns) in changed files
- Doc-path validator (`just check-doc-paths`) — ensures every backtick-cited file path in `docs/` actually exists
- `just setup` — one-time recipe installing system dependencies, Rust toolchain, Node.js v20+, and frontend packages
- `just act-*` recipes for running CI workflows locally via `nektos/act`
- `just reset-onboarding` — clears keychain entries and config markers for end-to-end testing of the BYOK flow

### Frontend Test Stack

- **Vitest** unit tests with `@testing-library/svelte` (22 tests)
- **Playwright** E2E tests with headless Chromium and mocked Tauri IPC
- **Screenshot baselines** — `just screenshots` regenerates reference screenshots at 4 times of day

---

## Testing

### Automated Testing

- Rust unit tests across all crates (`cargo test`)
- Frontend component tests with Vitest + @testing-library/svelte (22 tests)
- E2E browser tests with Playwright (headless Chromium)
- Script-based game harness testing (`GameTestHarness`) with eval baselines
- Architecture fitness tests enforcing module ownership and crate purity
- 90%+ code coverage target (`cargo tarpaulin`)

### Screenshot Generation

- Automated GUI screenshots at 4 times of day (morning, midday, dusk, night)
- Playwright + headless Chromium with mocked Tauri IPC
- No X11 or display server required

---

## Technical Foundation

| Component      | Technology                            |
| -------------- | ------------------------------------- |
| Language       | Rust                                  |
| Async runtime  | Tokio                                 |
| Desktop GUI    | Tauri 2                               |
| Frontend       | Svelte 5 + SvelteKit (static adapter) |
| HTTP client    | reqwest                               |
| Database       | SQLite (rusqlite, bundled)            |
| Serialization  | serde + serde_json                    |
| Error handling | thiserror (library) / anyhow (binary) |
| Logging        | tracing                               |
| Time           | chrono                                |
| Web server     | axum                                  |
| CLI parsing    | clap                                  |
| Map rendering  | MapLibre GL JS                        |
| Icons          | Phosphor Icons (MIT)                  |

---

## Implementation Status

### Fully Implemented

- **Phases 1–4 complete:** Core loop, world graph, NPC system with all four cognitive tiers dispatched, SQLite persistence with branching saves
- **Phases 5A–5E complete:** Event bus and tier transitions, weather state machine, long-term memory and gossip network, Tier 3 batch inference (10 NPCs per call, daily), Tier 4 rules engine (birth, death, illness, marriage, trade per season)
- **Phase 8 complete:** Tauri GUI rewrite with Svelte 5 frontend
- **Web server shipped:** Axum backend in `parish-server`, same Svelte UI over HTTP + WebSocket, Cloudflare Access JWT / Google OAuth / loopback auth, per-session save isolation, Prometheus `/metrics`
- **Parish Designer shipped:** Integrated GUI editor at `/editor` for NPCs, locations, schedules, and mod data
- **MCP bridge shipped:** `parish-mcp` exposes `mcp__parish__*` tools to AI agents
- **rundale-bench shipped:** Reproducible LLM benchmark for dialogue quality, Gaeilge fluency, and per-provider latency
- **Ambient sound system shipped:** Location-based audio with distance attenuation, weather dampening, and GUI-only playback (feature-gated)
- All 40+ slash commands
- Multi-provider LLM support (15 backends) with per-category routing
- Three-lane inference priority queue with structured JSON output and token streaming with bounded back-pressure
- Five-layer prompt-injection defense (ADR-010)
- Short-term NPC memory, relationships, mood, intelligence profiles
- Autonomous NPC chains (up to 3 follow-on exchanges) and off-screen social simulation
- Anachronism detection
- Gossip network with 60% transmission probability and 20% distortion per hop
- MapLibre GL interactive map with historic 1840s OS Ireland tiles, custom SVG icons, traversal-weighted edges, click-to-travel, and animated travel
- Three UI themes (cream/parchment, Solarized Light, Solarized Dark) with CSS custom property system
- Irish mod system with data-driven content loading and backend-agnostic loading

### Partially Implemented

- Full web & mobile client (web server shipped; mobile client planned)
- Mythology hooks (data fields exist in world.json, no active effects; Phase 6 planned)

### In Progress

- **rundale-bench model-quality benchmark** — active development on the LLM judging harness and leaderboard
- **World expansion (Phase 5F)** — Roscommon town, Athlone, Dublin with inter-region travel

### Planned

- **Save/Load UI (Phase 9)** — full GUI save management overhaul
- **Phase 6 (Polish & Mythology)** — folklore encounters, fairy fort events, `/help` and ASCII `/map` commands
- **Phase 7 (Web & Mobile)** — full mobile client
- **NPC function-calling / tool use (ADR-020)** — NPCs execute game-world actions via structured tool calls
- **Embedding-based NPC memory retrieval (ADR-021)** — semantic search over long-term memory
- **Hiberno-English dialogue fine-tune** — Gemma-based model trained on 1820s Irish dialect corpus

---

## Documentation

- **`docs/index.md`** is the master hub — phase status, design overview, ADR index, plans, research, and agent guides
- **Architectural Decision Records (ADRs)** — 24 records capturing the rationale behind graph-based worlds, cognitive LOD, SQLite WAL persistence, git-like branching, structured JSON output, real geography, per-category inference, prompt-injection defenses, the OSM geo-tool pipeline, and more
- **Historical research archive** — comprehensive 1820s Ireland research covering religion, family, education, crafts, food, transportation, law, politics, folklore, and Hiberno-English dialect notes informing NPC dialogue
- **`docs/agent/`** — slim, indexed reference for AI coding agents (build, architecture, code style, gotchas, harness, skills, git workflow, scaling rules), linked from `CLAUDE.md` and `AGENTS.md`
