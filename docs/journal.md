# Parish Development Journal

> [Docs Index](index.md)

Notes, observations, and recommendations carried between sessions.

---

## 2026-03-19 — Phase 3: Multiple NPCs & Simulation

### Changes this session

- **NPC module restructured**: Split `src/npc/mod.rs` into 7 submodules: `mod.rs` (core types), `manager.rs` (NpcManager + CogTier), `memory.rs` (ShortTermMemory), `relationship.rs` (Relationship + RelationshipKind), `schedule.rs` (DailySchedule + NpcState), `tier.rs` (Tier 1/2 tick functions), `overhear.rs` (overhear mechanic).
- **Extended Npc struct**: Added `home`, `workplace`, `state` (NpcState), `schedule` (DailySchedule), `relationships` (HashMap<NpcId, Relationship>), `memory` (ShortTermMemory), and `knowledge` (Vec<String>).
- **Relationship system**: `Relationship` struct with `RelationshipKind` enum (Family, Friend, Neighbor, Rival, Enemy, Romantic, Professional), strength (-1.0 to 1.0), history, and `adjust()` method. Context strings generated for LLM prompts.
- **Short-term memory**: Ring buffer (default 20 entries) with `add()`, `recent(n)`, `context_string()`. Auto-evicts oldest on overflow.
- **Daily schedule system**: `DailySchedule` with weekday/weekend schedules, seasonal overrides. `ScheduleEntry` specifies start_hour, end_hour, location, activity. Supports midnight-wrapping entries.
- **NpcState**: `Present(LocationId)` or `InTransit { from, to, arrives_at }` for tracking NPC movement.
- **NpcManager**: Central hub for all NPCs. Owns `HashMap<NpcId, Npc>` and `HashMap<NpcId, CogTier>`. `assign_tiers()` uses BFS distance: same location → Tier1, 1-2 edges → Tier2, 3+ → Tier3, in transit → Tier4. Query methods: `npcs_at()`, `get_mut()`, `npcs_in_tier()`.
- **Tier 1 tick**: Full LLM inference with extended context (personality + relationships + memory + knowledge + player action). Updates mood and memory after response.
- **Tier 2 tick**: Lighter group inference for NPCs at the same location. Produces `Tier2Event` with summary text and relationship changes.
- **Overhear mechanic**: `check_overhear()` surfaces Tier 2 events from locations 1 edge away as atmospheric text.
- **NPC data file**: `data/npcs.json` with 8 distinct NPCs — Padraig Darcy (publican), Siobhan Murphy (farmer), Fr. Declan Tierney (priest), Roisin Connolly (shopkeeper), Tommy O'Brien (farmer), Aoife Brennan (teacher), Mick Flanagan (retired guard), Niamh Darcy (barmaid). Each with full schedules, 3+ relationships, and knowledge items.
- **Main loop integration**: `NpcManager` loaded from data file on startup. Tier assignment runs before each player action. Tier 1 NPCs get full context prompts with relationships and memory.
- **Test count**: 232 tests passing (up from 160). 18 new NPC integration tests in `tests/npc_integration.rs`.

### Technical notes

- NPCs without explicit `state` in JSON default to `Present(LocationId(1))` (crossroads). Game loop should update NPC positions based on schedules at startup.
- Tier 2 ticks need timer-based invocation (every 5 game-minutes) — currently the framework is in place but not called from the game loop on a timer.
- The `Npc` struct implements `Serialize`/`Deserialize` for full data round-tripping.
- Relationship `context_string()` produces warmth labels: "very close" (≥0.7), "friendly" (≥0.3), "neutral" (≥-0.3), "strained" (≥-0.7), "hostile" (<-0.7).

### Recommendations for next session

1. **Wire NPC schedule movement**: At each tick, compare `desired_location()` to current `NpcState` and move NPCs via the world graph. Calculate traversal times and use `NpcState::InTransit`.
2. **Timer-based Tier 2 ticks**: Add a timer in the game loop that fires Tier 2 ticks every 5 game-minutes for groups of NPCs at the same location.
3. **Surface overhear in TUI**: Call `check_overhear()` after Tier 2 events and display the results in the text log.
4. **Apply Tier 2 relationship changes**: After Tier 2 events, update NPC relationships based on the `relationship_changes` in `Tier2Event`.
5. **Load world from parish file on startup**: `from_parish_file()` is called in main.rs but the legacy `WorldState::new()` fallback could be removed.
6. **Wire movement into game loop**: `resolve_movement()` still needs to be connected to `IntentKind::Move` handling.

---

## 2026-03-19 — Phase 2: World Graph Implementation

### Changes this session

- **World graph system**: New `src/world/graph.rs` module with `WorldGraph`, `Connection`, and `LocationData` types. Graph supports BFS pathfinding, fuzzy name search (with article stripping), neighbor queries, and path travel time calculation.
- **Parish data file**: `data/parish.json` with 14 hand-authored Kiltoom locations: The Crossroads (hub), Darcy's Pub, St. Brigid's Church, The Post Office, The GAA Pitch, The National School, Lough Ree Shore, Hodson Bay, Murphy's Farm, O'Brien's Farm, The Fairy Fort, The Bog Road, Connolly's Shop, and The Creamery.
- **Graph validation**: On load, validates all connection targets exist, connections are bidirectional, and no orphan nodes.
- **Movement system**: New `src/world/movement.rs` with `resolve_movement()` — resolves "go to X" intents to destinations via fuzzy matching, computes shortest path via BFS, and generates travel narration text.
- **Encounter system**: New `src/world/encounter.rs` — probability-based random encounters during travel. Base ~20% chance, modified by time of day (higher in morning, lower at night/midnight).
- **Dynamic descriptions**: New `src/world/description.rs` — renders location description templates by interpolating `{time}`, `{weather}`, and `{npcs_present}` placeholders with current game state.
- **WorldState integration**: `WorldState::from_parish_file()` loads the world graph and populates both the new graph and legacy locations map. New `current_location_data()` accessor.
- **Serde support**: Added `Serialize`/`Deserialize` to `LocationId` and `NpcId` with `#[serde(transparent)]`.
- **New error variant**: `WorldGraph(String)` in `ParishError`.
- **Test count**: 160 tests passing (up from 90), 1 ignored. Added 21 integration tests in `tests/world_graph_integration.rs`.
- **Mythological significance**: Crossroads, St. Brigid's Church, The Fairy Fort, The Bog Road, and Lough Ree Shore all have mythological flavor text.

### Technical notes

- `find_by_name()` fuzzy matching strips common articles ("the", "a", "an") and checks both directions (query in name, name in query).
- All 14 locations are reachable from the crossroads. Traversal times range from 2-10 minutes.
- The encounter system uses an explicit `roll` parameter for deterministic testing.
- Description templates use simple string replacement; LLM enrichment deferred to Phase 6.

### Recommendations for next session

1. **Wire movement into game loop**: The movement resolution, time advancement, encounter checks, and description rendering are all implemented but not yet connected to the main game loop (`main.rs`, `headless.rs`). Next step is to handle `IntentKind::Move` in the game loop by calling `resolve_movement()`, advancing the clock, checking encounters, and rendering the new location.
2. **Wire `from_parish_file` into startup**: Replace `WorldState::new()` with `WorldState::from_parish_file()` in `main.rs` so the full parish loads on game start.
3. **Add `/look` command**: Now that dynamic descriptions exist, wire up `IntentKind::Look` to render the current location description.
4. **OSM extraction tool**: Deferred — hand-authored data is sufficient for now.

---

## 2026-03-19 — Robust Ollama Integration & Headless Mode

### Changes this session

- **Ollama auto-install**: If `ollama` binary is not found, the game downloads and runs the official install script. Works for AMD (ROCm), NVIDIA (CUDA), and CPU-only.
- **GPU/VRAM detection**: Queries `nvidia-smi` or `rocm-smi` to detect GPU vendor and available VRAM. Falls back to CPU-only mode gracefully.
- **Automatic model selection**: Picks the best model for available VRAM (14b → 8b → 3b → 1.5b). Conservative thresholds leave headroom for OS/desktop.
- **Automatic model pulling**: If the selected model isn't available locally, pulls it via Ollama's `/api/pull` endpoint with progress reporting.
- **Headless CLI mode**: `--headless` flag starts a plain stdin/stdout REPL for testing without the TUI. Identical game logic.
- **CLI argument parsing**: Added `clap` for `--headless`, `--model`, and `--ollama-url` flags. Env vars (`PARISH_MODEL`, `PARISH_OLLAMA_URL`) still work as fallbacks.
- **New module**: `src/inference/setup.rs` — full Ollama lifecycle management (install, GPU detection, model selection, pulling).
- **New module**: `src/headless.rs` — headless REPL game loop.
- **New error variants**: `Setup(String)` and `ModelNotAvailable(String)` in `ParishError`.
- **Test count**: 90 tests passing (up from 52), 1 ignored.

### Technical notes

- Model selection uses free VRAM when available, 80% of total VRAM when free is unknown, or assumes 8GB when a GPU is detected but VRAM can't be queried.
- The `SetupProgress` trait allows TUI and headless modes to display setup progress differently.
- AMD ROCm detection falls back to checking `/opt/rocm` existence if `rocm-smi` output can't be parsed.

---

## 2026-03-18 — Phase 1 Complete

Phase 1 (Core Loop) is fully done. All roadmap items checked off. The game boots, renders a TUI, accepts natural language input, sends it through the Ollama inference pipeline, and renders NPC responses.

### Changes this session

- **Esc clears input** instead of quitting. `/quit` is now the only exit path.
- **Ollama auto-start/stop**: `OllamaProcess` checks if Ollama is reachable on startup, spawns `ollama serve` if not, and kills it on exit (with `Drop` safety net).

### Recommendations for Phase 2 (World Graph)

1. **Start with movement.** Hand-author 10-15 locations as JSON and wire up "go to X" commands. Movement + time-passing during travel is the single biggest gameplay unlock.
2. **Defer OSM extraction.** Hand-authored locations are fine for prototyping. The OSM tool is a nice-to-have and can come later without blocking anything.
3. **Dynamic location descriptions** layer well on the existing inference pipeline — enrich templates with time-of-day, weather, and NPC presence.
4. **En-route encounters** can be simple at first (random NPC sighting, weather change) and expanded later.

### Recommendations for Phase 3+

5. **Decide player identity before Phase 3.** NPC behavior (how they greet, what they share, suspicion level) depends heavily on whether the player is a known local, a newcomer, or an observer. See `docs/plans/open-questions.md`.
6. **Pull forward a minimal autosave.** Even a simple save-on-quit before the full Phase 4 branching system would reduce friction during testing.
7. **Nail down the verb set.** Minimal (look/go/talk) vs. moderate (also take/use/give) vs. expansive affects parser complexity and NPC prompt design.

### Technical notes

- **Ctrl+C handling**: Currently Ctrl+C will leave the terminal in raw mode and won't stop a Parish-started Ollama. A graceful signal handler (tokio::signal) should be added.
- **Test coverage**: 52 tests passing, 1 ignored (live Ollama test). Coverage should be verified with `cargo tarpaulin` before Phase 2 starts.
- **Inference latency**: No metrics yet. Consider adding request timing before scaling to multiple NPCs in Phase 3.
