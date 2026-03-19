# Parish Development Journal

> [Docs Index](index.md)

Notes, observations, and recommendations carried between sessions.

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
