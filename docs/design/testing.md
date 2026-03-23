# Testing Harness Design

## Overview

The `GameTestHarness` (`src/testing.rs`) provides a programmatic, synchronous
API for driving the game without a TUI or LLM. It enables:

- **Automated regression testing** via `cargo test`
- **Script-mode execution** via `cargo run -- --script <file>`
- **Claude Code interaction** — the AI coding assistant can run commands and
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

## Test Fixtures

Test scripts live in `tests/fixtures/`:

| File | Purpose |
|------|---------|
| `test_walkthrough.txt` | Full navigation across multiple locations |
| `test_movement_errors.txt` | Already-here, not-found, various verbs |
| `test_commands.txt` | All system commands |
| `test_speed.txt` | Game speed preset commands |

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
