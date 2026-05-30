# Test Coverage Analysis

**Date:** 2026-03-22
**Overall Coverage:** 46.36% (1,968 / 4,245 lines covered)
**Target:** 90% per CLAUDE.md engineering standards

## Per-File Coverage

| File | Lines Covered | Total Lines | Coverage |
|------|--------------|-------------|----------|
| `src/npc/memory.rs` | 25/25 | 100% | |
| `src/npc/mod.rs` | 55/55 | 100% | |
| `src/world/mod.rs` | 35/35 | 100% | |
| `src/world/encounter.rs` | 19/19 | 100% | |
| `src/world/description.rs` | 28/29 | 97% | |
| `src/world/graph.rs` | 101/105 | 96% | |
| `src/world/movement.rs` | 21/22 | 95% | |
| `src/npc/data.rs` | 45/48 | 94% | |
| `src/npc/manager.rs` | 123/132 | 93% | |
| `src/npc/types.rs` | 19/21 | 90% | |
| `src/world/time.rs` | 65/73 | 89% | |
| `src/npc/overhear.rs` | 16/16 | 100% | |
| `src/testing.rs` | 175/217 | 81% | |
| `src/input/mod.rs` | 87/110 | 79% | |
| `src/npc/ticks.rs` | 77/101 | 76% | |
| `src/gui/theme.rs` | 51/71 | 72% | |
| `src/gui/mod.rs` | 70/349 | 20% | |
| `src/gui/screenshot.rs` | 49/124 | 40% | |
| `src/tui/debug_panel.rs` | 36/69 | 52% | |
| `src/tui/mod.rs` | 52/199 | 26% | |
| `src/inference/openai_client.rs` | 34/102 | 33% | |
| `src/headless.rs` | 56/264 | 21% | |
| `src/inference/setup.rs` | 42/219 | 19% | |
| `src/inference/mod.rs` | 5/28 | 18% | |
| `src/inference/client.rs` | 6/97 | 6% | |
| `src/gui/sidebar.rs` | 0/62 | 0% | |
| `src/gui/status_bar.rs` | 0/43 | 0% | |
| `src/main.rs` | 0/369 | 0% | |

---

## Priority Areas for Improvement

### Priority 1 — CRITICAL: `src/main.rs` (0% → target 70%+)

**832 lines, 0 lines covered.** The main entry point contains significant
extracted logic that is testable without running a full game loop:

- **`setup_provider()`** — Constructs inference clients for Ollama, OpenRouter,
  LMStudio, and custom providers. Test each provider path, missing model name
  errors, and OllamaProcess initialization.
- **`handle_system_command()`** — Dispatches 15+ slash-commands (`/provider`,
  `/model`, `/key`, `/debug`, `/improv`, etc.). Each branch is independently
  testable.
- **`show_location_arrival()` / `show_location_description()`** — Format
  location output with NPC lists and exits. Test output formatting with varying
  NPC counts and graph connectivity.
- **`handle_movement()`** — Resolves movement, advances the clock, triggers
  schedule events. Test with the `GameTestHarness`.
- **`process_schedule_events()`** — Filters and formats NPC departure/arrival
  messages.

**Recommendation:** Extract these functions into a `game_loop.rs` module (or
similar) that takes dependencies as parameters rather than constructing them
inline. This makes them unit-testable without needing a full runtime.

---

### Priority 2 — CRITICAL: Inference module (6–33%)

| File | Coverage |
|------|----------|
| `inference/client.rs` | 6% |
| `inference/mod.rs` | 18% |
| `inference/setup.rs` | 19% |
| `inference/openai_client.rs` | 33% |

**What's missing:**

- **`OllamaClient::generate()` / `generate_stream()` / `generate_json()`** —
  No tests for HTTP timeouts, malformed JSON responses, partial NDJSON streams,
  or receiver-dropped mid-stream scenarios.
- **`OllamaProcess::ensure_running()` / `stop()`** — Process lifecycle
  management is completely untested. Test with mock process spawning or at
  minimum test the "already running" path.
- **`check_ollama_installed()` / `install_ollama()`** — System interaction
  functions. Use mock filesystem/commands or `#[cfg(test)]` feature gates.
- **GPU detection (`detect_nvidia`, `detect_amd`, `detect_windows_gpu`)** —
  Parse functions are testable with sample command output strings.
- **`select_model()` / `is_model_available()` / `pull_model()`** — API
  interaction functions. Use `mockito` or similar to test HTTP paths.

**Recommendation:** Introduce a trait-based HTTP client abstraction (or use
`mockall`) so inference tests can run without a live Ollama instance. GPU
detection parsers should be tested with captured sample outputs from each
platform.

---

### Priority 3 — HIGH: `src/headless.rs` (21%)

**675 lines, 56 covered.** The headless REPL mode has almost no unit tests.

**What's missing:**

- **`run_headless()`** — The entire REPL loop (input → parse → dispatch →
  output) is untested. At minimum, test the command dispatch table.
- **`handle_headless_command()`** — Mirrors `handle_system_command()` from
  main.rs; each branch should be tested.
- **`handle_headless_game_input()`** — NPC interaction flow including streaming
  output formatting.
- **`print_location_arrival()` / `print_location_description()`** — Output
  formatting with various NPC/exit combinations.
- **`handle_headless_movement()` / `process_headless_schedule_events()`** —
  Movement resolution and event handling.

**Recommendation:** The headless module shares significant logic with `main.rs`.
Consider extracting shared game-loop logic into a common module, then test that
module once. For headless-specific I/O formatting, capture stdout in tests.

---

### Priority 4 — HIGH: GUI & TUI (0–52%)

| File | Coverage |
|------|----------|
| `gui/sidebar.rs` | 0% |
| `gui/status_bar.rs` | 0% |
| `gui/mod.rs` | 20% |
| `gui/screenshot.rs` | 40% |
| `tui/mod.rs` | 26% |
| `tui/debug_panel.rs` | 52% |

**What's missing:**

- **`gui/sidebar.rs`** — Sidebar rendering with NPC info, no tests at all.
- **`gui/status_bar.rs`** — Status bar display, no tests at all.
- **`GuiApp::handle_system_command()` / `handle_movement()`** — Game logic
  duplicated from main.rs, untested in GUI context.
- **`GuiApp::maybe_idle_tick()` / `process_schedule_events()`** — Timer-driven
  simulation ticks, untested.
- **`tui/mod.rs`** — TUI rendering and input handling at 26%.

**Recommendation:** While rendering code is hard to unit test, the *logic*
behind what gets rendered is not. Extract data preparation from rendering (e.g.,
"what text goes in the sidebar" vs "how to draw it") and test the data layer.
For `egui`/`ratatui` rendering, consider snapshot testing.

---

### Priority 5 — MEDIUM: `src/npc/ticks.rs` (76%)

Good coverage but missing critical edge cases:

- **`run_tier2_for_group()`** — The async LLM call path is untested. Test with
  mock HTTP responses for both success and failure.
- **`apply_tier2_event()`** — Missing tests for: NPC not found in map,
  relationship not found, memory overflow, relationship strength clamping at
  bounds (>1.0, <-1.0), events with duplicate participant IDs.
- **`truncate_for_memory()`** — Missing tests for multi-byte UTF-8 characters
  at truncation boundary, empty input, and `max_len = 0`.

---

### Priority 6 — MEDIUM: `src/testing.rs` (81%)

The test harness itself needs better coverage:

- **`GameTestHarness::new()`** — No tests for missing data files or corrupted
  JSON.
- **`advance_time()`** — No tests for negative time, overflow, or NPCs in
  transit during advancement.
- **`add_canned_response()`** — No tests for multiple responses per NPC, case
  sensitivity, or empty responses.
- **`npcs_here()` / `debug_log()`** — No direct unit tests.

---

### Priority 7 — LOW: Well-covered modules needing edge cases

These modules are above 89% but have specific gaps:

- **`world/time.rs` (89%)** — Close to threshold; check which 8 lines are
  uncovered and add targeted tests.
- **`input/mod.rs` (79%)** — The LLM-based `parse_intent()` async path is
  untested. Test with mock HTTP. Also add tests for Unicode/emoji input and
  very long strings.
- **`npc/types.rs` (90%)** — At exactly the threshold; one missed branch could
  drop it below.

---

## Architectural Recommendations

1. **Extract shared game-loop logic.** `main.rs`, `headless.rs`, and
   `gui/mod.rs` all duplicate command handling, movement, and schedule event
   processing. Extracting this into a shared `game_logic.rs` module would:
   - Eliminate ~300 lines of duplication
   - Allow testing the logic once instead of three times
   - Make each frontend a thin I/O adapter

2. **Introduce trait-based mocking for inference.** The inference module is
   nearly untestable because it requires a live Ollama instance. Defining an
   `InferenceClient` trait and using `mockall` would unlock testing for:
   - NPC conversation flows
   - Tier 2 group events
   - Intent parsing
   - Error handling and retries

3. **Add integration tests for the headless REPL.** The `--script` mode in
   `GameTestHarness` provides a good foundation, but there are no integration
   tests that exercise the headless stdin/stdout path directly.

4. **Consider property-based testing** (`proptest` or `quickcheck`) for:
   - Input parsing (fuzz with random strings)
   - World graph operations (random graphs)
   - Time arithmetic (random durations)

---

## Path to 90% Coverage

| Area | Current Lines | Gap to 90% | Estimated New Tests |
|------|--------------|------------|-------------------|
| main.rs | 0/369 | +332 lines | ~15 tests |
| headless.rs | 56/264 | +182 lines | ~10 tests |
| inference/* | 87/446 | +314 lines | ~20 tests |
| gui/* | 170/649 | +414 lines | ~15 tests |
| tui/* | 88/268 | +153 lines | ~8 tests |
| Other gaps | — | ~100 lines | ~10 tests |
| **Total** | **1,968/4,245** | **~1,495 lines** | **~78 tests** |

Getting to 90% requires covering approximately 1,495 additional lines, which
translates to roughly 78 new test functions. The highest-impact work is
extracting shared game logic (Priority 1 + 3) and adding inference mocking
(Priority 2), which together would unlock coverage for ~800+ lines.
