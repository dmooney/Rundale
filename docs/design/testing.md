# Testing Harness Design

## Overview

The `GameTestHarness` (`crates/parish-cli/src/testing.rs`) provides a programmatic, synchronous
API for driving the game without a TUI or LLM. It enables:

- **Automated regression testing** via `cargo test`
- **Script-mode execution** via `cargo run -- --script <file>`
- **Agent interaction** — an AI coding assistant can run commands and
  verify game behavior through structured JSON output

## Architecture

```
┌──────────────────────────────────────────┐
│            GameTestHarness               │
│                                          │
│  ┌─────┐  ┌────────────────┐  ┌───────┐ │
│  │ App │  │ canned_responses│  │ query │ │
│  │     │  │ (NPC mocks)    │  │ APIs  │ │
│  └──┬──┘  └───────┬────────┘  └───┬───┘ │
│     │             │               │      │
│  execute(input) ──┴───────────────┘      │
│     │                                    │
│  classify_input() → SystemCommand        │
│                   → GameInput            │
│     │                                    │
│  parse_intent_local() → Move/Look        │
│                       → None → NPC mock  │
└──────────────────────────────────────────┘
```

### Key Design Decisions

1. **No Ollama dependency** — Uses `parse_intent_local()` for movement/look.
   NPC interactions use canned responses instead of LLM inference.

2. **Synchronous** — No async runtime needed. All game logic (movement,
   time, descriptions) is synchronous anyway.

3. **Same code paths** — Reuses `resolve_movement()`, `render_description()`,
   `format_exits()`, `classify_input()`, and `GameClock::advance()` from
   the production code.

4. **Structured output** — `ActionResult` enum captures every outcome as a
   typed variant, not prose text. Tests assert on structure, not strings.

## ActionResult Variants

| Variant | When |
|---------|------|
| `Moved { to, minutes, narration }` | Player moved to a new location |
| `Looked { description }` | Player looked around |
| `AlreadyHere` | Tried to move to current location |
| `NotFound { target }` | Destination not in world graph |
| `SystemCommand { response }` | `/pause`, `/status`, `/speed`, `/help`, etc. |
| `NpcResponse { npc, dialogue }` | Canned NPC response consumed |
| `NpcNotAvailable` | NPC present but no canned response |
| `UnknownInput` | Input not recognized locally |
| `Quit` | `/quit` executed |

## Script Mode

`cargo run -- --script <file>` reads commands from a text file (one per line)
and outputs one JSON object per command:

```bash
$ echo -e "go to pub\nlook\n/status\n/quit" > test.txt
$ cargo run -- --script test.txt
{"command":"go to pub","result":"moved","to":"Darcy's Pub","minutes":5,...}
{"command":"look","result":"looked","description":"..."}
{"command":"/status","result":"system_command","response":"Location: ..."}
{"command":"/quit","result":"quit","location":"Darcy's Pub",...}
```

Lines starting with `#` are comments. Empty lines are skipped.

## CLI-GUI Parity Commands

The headless CLI (`crates/parish-cli/src/headless.rs`) and test harness (`crates/parish-cli/src/testing.rs`) support
commands that mirror GUI-only features, enabling full play-testing without Tauri:

| Command | Description | Handler Source |
|---------|-------------|----------------|
| `/map` | Text-based map: lists all locations with connections, marks player with `*` | `WorldGraph::location_ids()` + `neighbors()` |
| `/npcs` | NPCs at current location: name, occupation, mood, introduced status | `NpcManager::npcs_at()` + `display_name()` |
| `/time` | Detailed time info: hour:minute, time_of_day, season, weather, speed, festival | `GameClock::now()` + `.season()` + `.check_festival()` |
| `/wait [N]` | Advance time by N game minutes (default 15), tick NPC schedules | `GameClock::advance()` + `tick_schedules()` |
| `/tick` | Manually tick NPC schedules without advancing time | `assign_tiers()` + `tick_schedules()` |
| `/new` | Start a fresh game: reload world/NPCs from mod files, reset persistence | Same init path as `GameTestHarness::new()` |
| `/where` | Alias for `/status` | Parsed as `Command::Status` |

### Time Advancement Design

The GUI advances time via background tick loops (`tokio::spawn` in Tauri and
the Axum web server). The CLI cannot do this because `reader.lines()` blocks
synchronously on stdin — adding background ticks would require switching to
async stdin with `tokio::select!`, which is a significant refactor.

Instead, the CLI uses **explicit time control**:
- `/wait N` advances the game clock by N minutes and ticks NPC schedules
- `/tick` runs NPC schedule assignment without advancing time
- The game clock still runs in real-time between commands (via `speed_factor`)

This is actually better UX for a text adventure — the player controls time
explicitly rather than having NPCs move unpredictably during input.

### Command Enum Variants

Added to `Command` in `src/input/mod.rs`:

```rust
Map,           // /map
NpcsHere,      // /npcs
Time,          // /time
Wait(u32),     // /wait [N] — default 15
NewGame,       // /new
Tick,          // /tick
```

`/where` is parsed as `Command::Status` (no new variant).

## Agent Play-Testing Skill

The `/play` skill (`.agents/skills/play/SKILL.md`) enables an AI coding assistant to
autonomously play-test the game via `--script` mode:

1. Build the project with `cargo build`
2. Generate or use a script file with game commands
3. Run `cargo run -- --script <file>` to get structured JSON output
4. Analyze each JSON line for correctness (movement, NPCs, time, errors)
5. Report a play-test summary with findings

This leverages the CLI-parity commands so the AI can exercise the same
features available in the GUI: checking the map, observing NPCs, advancing
time, and verifying schedule-driven NPC behavior.

## Test Fixtures

Test scripts live in `testing/fixtures/`:

| File | Purpose |
|------|---------|
| `test_walkthrough.txt` | Full navigation across multiple locations |
| `test_movement_errors.txt` | Already-here, not-found, various verbs |
| `test_commands.txt` | All system commands |
| `test_speed.txt` | Game speed preset commands |
| `test_debug.txt` | Debug subsystem commands |
| `test_all_locations.txt` | Navigate to and look at all 15 parish locations |
| `test_fuzzy_names.txt` | Fuzzy location name matching (partial, apostrophes, articles) |
| `test_multi_hop.txt` | Multi-hop pathfinding to non-adjacent locations |
| `test_movement_verbs.txt` | All 8 movement verbs (go/walk/head/stroll/saunter/mosey/run/dash) |
| `test_time_progression.txt` | Time-of-day advancement through many round trips |
| `test_pause_resume_cycle.txt` | Pause/resume state machine and idempotency |
| `test_debug_all_npcs.txt` | `/debug schedule/memory/rels` for all 8 NPCs |
| `test_debug_at_locations.txt` | `/debug here/tiers/clock` at multiple locations |
| `test_npc_locations.txt` | NPC presence verification at expected locations |
| `test_edge_cases.txt` | Already-here, not-found, repeated commands, unknown inputs |
| `test_look_variants.txt` | `look`, `l`, `look around` at multiple locations |
| `test_grand_tour.txt` | Visit all 15 locations with look + status at each |
| `test_speed_assertions.txt` | Speed preset changes with status verification |
| `test_new_commands.txt` | CLI-parity commands: `/map`, `/npcs`, `/time`, `/wait`, `/tick`, `/where` |

## Captured Script Mode (`run_script_captured`)

For tests that need to assert on script output (not just "no crash"),
use `run_script_captured()` which returns a `Vec<ScriptResult>`:

```rust
use parish::testing::{run_script_captured, ActionResult, ScriptResult};
use std::path::Path;

#[test]
fn test_example_with_assertions() {
    let results = run_script_captured(Path::new("testing/fixtures/test_grand_tour.txt")).unwrap();

    // Assert every movement succeeded
    for r in &results {
        if let ActionResult::Moved { to, minutes, .. } = &r.result {
            assert!(!to.is_empty());
            assert!(*minutes > 0);
        }
    }

    // Verify location tracking
    for r in &results {
        if let ActionResult::Moved { to, .. } = &r.result {
            assert_eq!(r.location, *to);
        }
    }
}
```

The `ScriptResult` struct captures command, result, location, time, and season
for each executed line:

```rust
pub struct ScriptResult {
    pub command: String,
    pub result: ActionResult,
    pub location: String,
    pub time: String,
    pub season: String,
}
```

## Usage in Tests

```rust
use parish::testing::{GameTestHarness, ActionResult};

#[test]
fn test_example() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig O'Brien", "Ah, good morning!");

    h.execute("go to pub");
    assert_eq!(h.player_location(), "Darcy's Pub");

    h.execute("go to crossroads");
    let r = h.execute("hello Padraig");
    assert!(matches!(r, ActionResult::NpcResponse { .. }));
}
```

## Integration Test Files

| File | Tests | Purpose |
|------|-------|---------|
| `tests/game_harness_integration.rs` | 23 | Multi-step harness scenarios, NPC responses, script fixture smoke tests |
| `tests/world_graph_integration.rs` | 21 | World graph validation, pathfinding, descriptions |
| `tests/headless_script_tests.rs` | 68 | Comprehensive fixture-driven tests with assertions on every ActionResult |

The `headless_script_tests.rs` file uses `run_script_captured()` to exercise
all 18 fixture scripts with real assertions on game state — verifying locations,
time progression, NPC data, debug output, error handling, and more.

## Eval baselines

`crates/parish-cli/tests/eval_baselines.rs` is an inferential sensor for
gameplay behavior — it runs each baselined fixture through `run_script_captured`,
serializes the captured `Vec<ScriptResult>` to JSON, and diffs against a stored
baseline at `testing/evals/baselines/<fixture>.json`. Any drift fails the
test with a "live | baseline" diff window and the canonical fix.

The same file applies three structural rubrics to every baselined fixture:

| Rubric | Catches |
|---|---|
| `rubric_anachronisms_are_empty` | NpcResponse drift that introduces out-of-period words |
| `rubric_movement_minutes_are_positive` | Frozen game clock — Moved with `minutes == 0` |
| `rubric_look_descriptions_are_non_empty` | Silent renderer failure — Looked with empty description |

Baselined fixtures (`BASELINED_FIXTURES` in the test): `test_movement_errors`,
`test_walkthrough`, `test_all_locations`. New fixtures go in this list once
their structured output has been verified deterministic across runs.

To regenerate after an intentional gameplay change:

```sh
just baselines    # = UPDATE_BASELINES=1 cargo test -p parish --test eval_baselines
git diff testing/evals/baselines/   # review the diff before committing
```

The `/rubric` skill (`.skills/rubric/SKILL.md`) documents the agent-facing
workflow.

## E2E GUI Testing (Playwright)

The Svelte frontend has Playwright E2E tests in `apps/ui/e2e/` that run against
the Vite dev server with Tauri IPC fully mocked. This enables:

- **Real browser rendering** — headless Chromium, no X11/GDK/xvfb required
- **Screenshot generation** — captures 4 times of day to `docs/screenshots/`
- **Visual regression** — baseline comparison via `toHaveScreenshot()`
- **Interaction testing** — input submission, streaming, theme transitions

### How the Mock Works

`apps/ui/e2e/fixtures.ts` uses `page.addInitScript()` to install a fake
`window.__TAURI_INTERNALS__` before any app code runs. This provides:

- `invoke()` — returns mock data for `get_world_snapshot`, `get_map`, etc.
- `transformCallback()` — registers callbacks with numeric IDs
- `plugin:event|listen` — tracks event listeners by name + callback ID
- `__TEST_EMIT_EVENT__()` — helper for tests to dispatch events to listeners

### Running

```bash
# Full E2E suite
cd ui && npx playwright test           # or: just ui-e2e

# Screenshots only
cd ui && npx playwright test e2e/screenshots.spec.ts  # or: just screenshots

# Update visual regression baselines
cd ui && npx playwright test --update-snapshots       # or: just ui-e2e-update
```

### Test Files

| File | Tests | Purpose |
|------|-------|---------|
| `e2e/app.spec.ts` | 10 | Layout, status bar, chat, map, sidebar, theme, events |
| `e2e/interactions.spec.ts` | 5 | Input, streaming, paused state, festival badge |
| `e2e/screenshots.spec.ts` | 8 | Screenshot capture + visual regression baselines |

### Visual Regression Baselines

Baseline images live in `apps/ui/e2e/screenshots/baseline/`. When UI changes are
intentional, update them with `npx playwright test --update-snapshots`.

## Query APIs

| Method | Returns |
|--------|---------|
| `player_location()` | Location name (`&str`) |
| `location_id()` | `LocationId` |
| `time_of_day()` | `TimeOfDay` |
| `season()` | `Season` |
| `text_log()` | Full `&[String]` log |
| `last_output()` | Last non-empty log line |
| `npcs_here()` | NPC names at current location |
| `exits()` | Formatted exit string |
| `weather()` | Weather string |
| `is_paused()` | Clock pause state |
