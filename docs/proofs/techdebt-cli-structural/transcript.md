# Techdebt CLI Structural Refactor — Transcript

## What was changed and why

Phase 3.5 through 3.8 of the parish-cli technical debt sweep. Pure structural changes — no behavior modifications. All internal to the `parish-cli` crate; public API intact.

### Phase 3.5: Headless refactor (TD-002, TD-003, TD-004)

**TD-002 — Break up `run_headless` (525→97 lines)**
- Extracted `print_startup_header` — banner and provider info
- Extracted `setup_inference_queue` — inference worker spawn + channel creation
- Extracted `run_headless_repl_loop` — the main stdin/stdout loop
- `run_headless` now calls these helpers then delegates to the REPL loop
- The loop body itself delegates to 6 `dispatch_headless_*` helpers

**TD-003 — Break up `handle_headless_game_input` (237→81 lines)**
- Extracted `stream_headless_npc_dialogue` — full NPC conversation flow with loading animation, streaming, memory pipeline, conversation log, and witness recording
- The main function retains intent parsing, movement/look routing, and the idle-message fallback

**TD-004 — Simplify `resolve_category_configs` (200→21 lines)**
- Extracted `category_toml_override` — maps category to its TOML override section
- Extracted `category_has_overrides` — the messy 5-source boolean check
- Extracted `resolve_single_category` — the per-category config resolution with all 5 override layers
- Top-level function is now a simple loop calling `resolve_single_category`

### Phase 3.6: Struct refactors (TD-005, TD-006)

**TD-005 — Reduce `App` struct (74→62 fields, net -12)**
- Introduced `CategoryOverride` struct wrapping the repeated 5-field pattern (client, model, provider_name, api_key, base_url)
- Replaced 15 per-category fields (intent_*, simulation_*, reaction_*) with 3 `CategoryOverride` instances
- Added `category_override` and `category_override_mut` helpers
- All 10 getter/setter methods simplified to delegate through these helpers

**TD-006 — Shrink `Cli` processing code (33→17 lines)**
- Refactored `build_cli_category_overrides` to iterate a tuple array of (name, provider, base_url, model) instead of repeating the same block 3 times

### Phase 3.7: Duplication elimination (TD-007 through TD-011)

**TD-007 — Merge schedule event processors**
- Created `process_schedule_events_generic` returning `Vec<String>` of player-visible messages
- Headless wrapper iterates messages with `println!`
- Testing wrapper iterates messages with `world.log()`
- Eliminated duplicate match arms and debug-string logic

**TD-008 — Mirror methods `snapshot_config`/`apply_config`**
- Indirectly simplified by TD-005: getter/setter methods now use `category_override` helpers, making the per-category iteration block in both methods concise and consistent

**TD-009 — Replace per-category getter/setter duplication**
- Added `category_override(&self, cat)` and `category_override_mut(&mut self, cat)` helpers
- All 10 methods (5 getters + 5 setters) now delegate to these helpers with `Dialogue` special-cased for cloud fields
- Eliminates the repeated `match cat { Dialogue => ..., Simulation => ..., Intent => ..., Reaction => ... }` inside each method

**TD-010 — Extract snapshot loading sequence**
- Created `load_and_restore_snapshot` (28 lines) — loads snapshot, replays journal, assigns tiers
- Shared by `restore_from_db` and the named-branch path in `handle_headless_load`
- Removes inline duplicate of snapshot-load + replay + tier-assign that appeared in 2 places

**TD-011 — Extract tier tick dispatch**
- Created `dispatch_headless_weather`, `dispatch_headless_banshee`
- Created `dispatch_headless_tier4_tick`, `dispatch_headless_tier3_tick`, `dispatch_headless_tier2_tick`
- Created `dispatch_headless_autosave`
- REPL loop now calls each dispatch function in sequence

### Phase 3.8: main() (TD-012)

**TD-012 — Break up `main()` (172→40 lines)**
- Extracted `setup_tracing_and_otel` — tracing subscriber and OTel layer setup
- Extracted `resolve_configs` (async) — resolves provider/cloud/category configs + builds clients + loads engine config, returns `ResolvedConfigs` struct
- Extracted `load_game_mod` — mod loading from CLI path or auto-detect
- `main()` now dispatches script/web/headless modes and calls the extracted helpers

## Before/After line counts

| Function/Area | Before | After | Change |
|---|---|---|---|
| `run_headless` | 525 | 97 | -428 |
| `run_headless_repl_loop` (new) | — | 96 | +96 |
| `handle_headless_game_input` | 237 | 81 | -156 |
| `stream_headless_npc_dialogue` (new) | — | 157 | +157 |
| `process_headless_schedule_events` | 26 | 5 | -21 |
| `process_schedule_events_generic` (new) | — | 30 | +30 |
| `restore_from_db` | 23 | 27 | +4 |
| `load_and_restore_snapshot` (new) | — | 28 | +28 |
| `resolve_category_configs` | 200 | 21 | -179 |
| `resolve_single_category` (new) | — | 130 | +130 |
| `main()` | 172 | 40 | -132 |
| `setup_tracing_and_otel` (new) | — | 36 | +36 |
| `resolve_configs` (new) | — | 58 | +58 |
| `load_game_mod` (new) | — | 19 | +19 |
| `build_cli_category_overrides` | 33 | 17 | -16 |
| `process_schedule_events` (testing) | 30 | 6 | -24 |
| App struct (pub fields) | 74 | 62 | -12 |
| Total (files listed above) | ~1266 | ~850 | Net -416 |

## Files changed

1. `parish/crates/parish-cli/src/app.rs` — TD-005, TD-008, TD-009
2. `parish/crates/parish-cli/src/headless.rs` — TD-002, TD-003, TD-007, TD-010, TD-011
3. `parish/crates/parish-cli/src/config.rs` — TD-004
4. `parish/crates/parish-cli/src/main.rs` — TD-006, TD-012
5. `parish/crates/parish-cli/src/testing.rs` — TD-007
6. `parish/crates/parish-cli/TODO.md` — moved all items to Done

## Commands run

```
cargo test -p parish           # all tests pass
cargo test --workspace         # only pre-existing tauri failure
cargo clippy --all-targets -- -D warnings  # clean
```
