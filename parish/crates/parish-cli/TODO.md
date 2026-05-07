# parish-cli — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-002 | Complexity | P2 | `src/headless.rs:49-571` | `run_headless` is 522 lines — well over the 100-line threshold. Orchestrates inference setup, world loading, persistence, NPC management, weather, all tier ticks, and autosave in a single function. |
| TD-003 | Complexity | P2 | `src/headless.rs:779-1015` | `handle_headless_game_input` is 237 lines with 5+ levels of nesting (match → match → if let → match → if). Handles intent parsing, movement, NPC conversation with streaming, metadata parsing, memory pipeline, and witness recording. |
| TD-004 | Complexity | P2 | `src/config.rs:115-311` | `resolve_category_configs` is 197 lines with 5 config-resolution layers (legacy cloud → TOML → env → CLI → provider-key-env). The layered merging logic is hard to follow and has a subtle parent-vs-child interaction at line 256. |
| TD-005 | Complexity | P2 | `src/app.rs:85-201` | `App` struct has 74 fields covering world state, inference clients (base + 4 categories + cloud), persistence, debug, UI state, and feature flags. Category-specific fields repeat the same 5-field pattern (client, model, provider_name, api_key, base_url) 5 times. |
| TD-006 | Complexity | P2 | `src/main.rs:18-105` | `Cli` struct has 27 CLI arguments. The per-category provider/model/base_url triple is repeated 3 times (dialogue, simulation, intent). |
| TD-007 | Duplication | P2 | `src/headless.rs:1378-1403` `src/testing.rs:511-540` | `process_headless_schedule_events` and `process_schedule_events` are near-identical — both match on `ScheduleEventKind`, format debug strings, and log arrival/departure messages. The only difference is `println!` vs `self.app.world.log()`. |
| TD-008 | Duplication | P2 | `src/app.rs:382-454` | `snapshot_config` and `apply_config` are mirror methods that copy the same 40+ fields back and forth between `App` ↔ `GameConfig`. Every new App field requires updating both methods. |
| TD-009 | Duplication | P2 | `src/app.rs:268-368` | Per-category getter/setter methods (`category_provider_name`, `category_model`, `category_api_key`, `category_base_url`, `category_client` plus their `set_` counterparts) are 10 near-identical methods all doing the same `match cat { Dialogue => ..., Simulation => ..., ... }`. |
| TD-010 | Duplication | P2 | `src/headless.rs:578-600` `src/headless.rs:688-699` `src/headless.rs:762-770` | Snapshot loading + replay + tier-assignment sequence is duplicated in `restore_from_db`, `handle_headless_load` (bare-load path), and `handle_headless_new_game`. |
| TD-011 | Duplication | P2 | `src/headless.rs:363-539` | Tier 4 (lines 366-391), Tier 3 (lines 396-452), and Tier 2 (lines 457-539) tick dispatch blocks are structurally identical (check `needs_tierN_tick` → collect NPCs → tick → apply events → record) but repeated 3 times inline in the REPL loop. |
| TD-012 | Complexity | P2 | `src/main.rs:108-269` | `main()` is 162 lines. Handles .env loading, tracing setup, OTel provider, CLI parsing, script/web/headless routing, provider resolution (base + cloud + per-category), mod loading, and engine config — all inline. |
| TD-015 | Config/Cargo | P3 | `src/config.rs:319-330` | `load_toml` falls back to `Path::new("parish.toml")` — a cwd-relative path. Same AGENTS.md rule #9 concern as TD-014. Packages, daemonised servers, and deployments with non-standard working directories will silently miss the config file. |

## In Progress

*(none)*

## Done

| ID | Category | Description |
|----|----------|-------------|
| TD-001 | Config/Cargo | Removed unused `thiserror` from `Cargo.toml:20`. |
| TD-013 | Dead Code | Removed `ScrollState` struct, its methods, and all associated tests from `src/app.rs`. |
| TD-014 | Dead Code | Added `#[deprecated]` to `find_data_dir` and `find_ui_dist_dir` in `src/main.rs` with `#[allow(deprecated)]` at call sites. |
| TD-016 | Stale Docs | Fixed doc comment in `src/emitter.rs:15`: `parish_cli` → `parish`. |
| TD-017 | Stale Docs | Fixed `strength_bar` doc comment in `src/debug.rs:497` to match implementation. |
| TD-018 | Weak Tests | Refactored `StdoutEmitter` to expose `format_event()` for direct testing of content-extraction logic; added 5 new assertions. |
| TD-019 | Stale Docs | Updated `too_many_arguments` comment in `src/headless.rs` to reference TODO.md; removed stale `#future` reference. |
