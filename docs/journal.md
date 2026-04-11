# Rundale Development Journal

> [Docs Index](index.md)

Notes, observations, and recommendations carried between sessions.

---

## 2026-04-04 — Prompt Immersion: Conversation Awareness System

Major NPC immersion feature: NPCs now hear and remember conversations happening around them.

**Conversation Witness System**: When the player talks to NPC A at a location where NPCs B and C are present, B and C each get a short-term memory entry recording what they overheard. Next time the player talks to B, their context includes "Overheard: a traveller said '...' and Padraig replied '...'".

**Conversation Log**: New `ConversationLog` struct (ring buffer of 30 exchanges) tracks recent player-NPC exchanges per location. Injected into context prompts as "What's been said here" — gives NPCs awareness of the ongoing scene, not just their own isolated exchange.

**Scene Continuity**: If the player recently spoke to the same NPC, a cue prevents re-greeting: "You are already in conversation with this traveller."

**Prompt Quality Overhaul**: Relationships now reference NPCs by name ("Niamh Darcy: Family, very close") instead of IDs. "Also present" includes relationship context. Knowledge section reframed as "WHAT'S ON YOUR MIND".

**Pre-existing Gap Fixed**: Server, headless CLI, and Tauri modes were not calling `apply_tier1_response` after NPC conversations — NPCs never updated mood or recorded speaker memories in production modes. Only the test harness did. Fixed across all modes.

**Mode Parity Audit**: All 4 modes verified to have identical post-response wiring: `apply_tier1_response` + `conversation_log.add()` + `record_witness_memories()`.

Play-tested at Darcy's Pub: after 3 exchanges between player/Padraig/Niamh, Padraig had 3 memories (2 spoken, 1 overheard) and Niamh had 3 memories (1 spoken, 2 overheard). Feature proven live.

---

## 2026-03-27 — Celtic Triquetra Loading Spinner

Replaced the generic rotating circle CSS spinner in `ChatPanel.svelte` with an inline SVG Celtic triquetra (Trinity knot) animation. The three interlocking lobes draw and erase themselves sequentially using `stroke-dasharray`/`stroke-dashoffset`, with staggered delays and a slow rotation overlay. Uses `var(--color-accent)` so it adapts to time-of-day palette changes. Pure CSS animation, no JS libraries. Thematically consistent with the game's 1820s Irish setting.

Research: no existing Celtic knot spinner libraries were found — constructed the triquetra geometry from three pairs of SVG arc commands forming vesica piscis lobes at 120° intervals, with `pathLength="120"` for normalized dash control.

---

## 2026-03-26 — Location Alias System

Added `aliases` field to `LocationData` to support colloquial and semantic synonyms
for location names. Players can now say "go to the coast" (→ Lough Ree Shore),
"go to the store" (→ Connolly's Shop), "go to the rath" (→ The Fairy Fort), etc.

- Added `aliases: Vec<String>` with `#[serde(default)]` to `LocationData`
- Extended `find_by_name` to check aliases at every matching level (exact, substring,
  reverse substring, article-stripped), with name matches always taking priority
- Added Level 5 Jaro-Winkler fuzzy fallback (`strsim` crate) to catch typos
  and near-misses (threshold 0.82 to avoid false positives)
- Added aliases to all 15 locations in `data/parish.json`
- Added unit tests for alias matching, fuzzy matching, and integration tests
  against real parish data

---

## 2026-03-25 — TUI Removal

Removed the ratatui terminal UI (`src/tui/`) from the project. Rundale now has two modes:

- **Headless REPL** (default): plain stdin/stdout, launched via `cargo run`
- **Tauri GUI** (desktop app): launched via `cargo tauri dev`

The `App` struct (core application state, `ScrollState`) moved from `src/tui/mod.rs` to `src/app.rs`. All ratatui and crossterm dependencies have been dropped. Updated all documentation to reflect the change.

---

## 2026-03-21 — Phase 3: Multiple NPCs & Simulation

Implemented Phase 3 in 6 batches:

1. **Data structures**: `types.rs` (Relationship, DailySchedule, NpcState, CogTier, Tier2Event), `memory.rs` (ShortTermMemory ring buffer). Extended `Npc` struct with home, workplace, schedule, relationships, memory, knowledge, state.

2. **NPC data**: `data/npcs.json` with 8 NPCs (Padraig Darcy, Siobhan Murphy, Fr. Declan Tierney, Roisin Connolly, Tommy O'Brien, Aoife Brennan, Mick Flanagan, Niamh Darcy). Loader hydrates bidirectional relationships.

3. **NpcManager** (`manager.rs`): Central coordinator with BFS-based tier assignment, schedule-driven NPC movement (InTransit state), `npcs_at()` queries, Tier 2 grouping.

4. **Tier ticks** (`ticks.rs`): Enhanced system prompts with relationship/knowledge context, enhanced context with memory and co-present NPCs. Tier 2 inference via `generate_json`. Snapshot-and-apply pattern for background Tier 2.

5. **Overhear** (`overhear.rs`): Atmospheric messages for Tier 2 events 1 edge from player. Integrated NpcManager into App (replaced `Vec<Npc>`), updated main.rs, headless.rs, testing.rs.

6. **Polish**: Updated all tests for new NPC names/locations, docs, roadmap.

Key decisions: NpcManager owned by App (no Arc), Tier 2 uses non-streaming `generate_json`, overhear surfaces via text_log.

---

## 2026-03-21 — Historical Setting, Pronunciation Sidebar, and Whimsy

### Changes this session

- **Historical period fixed**: Game now set in 1820. Clock initialization changed from 2026 to 1820. All location data in `data/parish.json` updated to remove anachronisms (GAA → hurling green, An Post → letter office, National School → hedge school, tractors → donkeys, corrugated sheds → thatched stone outbuildings, creamery → lime kiln, modern shop goods → period-appropriate items).
- **NPC system prompt overhauled**: `build_tier1_system_prompt()` now includes detailed 1820 historical context (Acts of Union, Catholic Emancipation not yet achieved, agricultural economy, no modern technology), cultural guidelines (avoid stereotypes, portray with dignity), and instructions for Irish word pronunciation hints in metadata.
- **Irish pronunciation sidebar**: New collapsible sidebar in the TUI (toggle via Tab or `/irish`). Displays Irish words used in NPC dialogue with phonetic pronunciation and English translations. New `IrishWordHint` struct and `irish_words` field added to `NpcMetadata`. Sidebar renders in a 70/30 horizontal split with title "Focail — Words".
- **Whimsical text throughout**: All player-facing messages updated with warm, atmospheric Irish flavor. The LLM/Ollama is framed as "the parish storyteller." Idle messages rotate through atmospheric descriptions. Help text, quit message, error messages, and setup progress all rewritten. Headless mode updated to match.
- **Cultural sensitivity guardrails**: System prompt explicitly forbids stereotypical portrayals (drunkenness, violence, stage-Irish dialect, "begorrah"). Emphasizes dignity, intelligence, warmth.
- **Test count**: 226 tests passing (205 unit + 21 integration). Added 14 new tests covering Irish word hints, pronunciation metadata, sidebar state, `/irish` command parsing, historical context in prompts, and anti-stereotype guardrails.

### Technical notes

- Sidebar uses `Layout::horizontal()` with `Constraint::Percentage(70/30)` split when visible.
- `IrishWordHint` uses `#[serde(default)]` for the optional `meaning` field for robustness.
- Idle messages use a simple counter (modular index into a const array) — no randomness needed.
- Headless mode uses `AtomicUsize` for idle counter since it doesn't have the `App` struct's `idle_counter` field available.

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
