# parish-cli — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Config/Cargo | P2 | `Cargo.toml:21` | Unused dependency `thiserror` — no file in the crate imports, derives, or references it. Present in `[dependencies]` but never used. |
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
| TD-013 | Dead Code | P3 | `src/app.rs:32-73` | `ScrollState` struct and its methods (`scroll_up`, `scroll_down`, `scroll_to_top`, `scroll_to_bottom`) are defined in the CLI crate but only meaningful with a GUI. In headless mode these fields are never exercised — they exist solely for Tauri mode-parity. |
| TD-014 | Dead Code | P3 | `src/main.rs:351-367` `src/main.rs:370-385` | `find_data_dir` and `find_ui_dist_dir` use `std::env::current_dir()` + parent-walk (up to 4 levels) searching for marker files. This violates AGENTS.md rule #9 ("Never call current_dir(), parent-walks, or marker-file searches") and will break in daemonised or `/tmp`-working-directory deployments. Existing safeguard comment: `parish_persistence::picker::resolve_project_saves_dir` is the prescribed alternative per the rule text. |
| TD-015 | Config/Cargo | P3 | `src/config.rs:319-330` | `load_toml` falls back to `Path::new("parish.toml")` — a cwd-relative path. Same AGENTS.md rule #9 concern as TD-014. Packages, daemonised servers, and deployments with non-standard working directories will silently miss the config file. |
| TD-016 | Stale Docs | P3 | `src/emitter.rs:15` | Doc comment example uses `use parish_cli::emitter::StdoutEmitter;` — but the crate is published as `parish`, not `parish_cli`. The correct import is `use parish::emitter::StdoutEmitter;`. |
| TD-017 | Stale Docs | P3 | `src/debug.rs:475` | Doc comment on `strength_bar` says it renders "████░░░░░░" but the implementation uses `#` and `.` characters. Doc matches visual output only coincidentally on some terminals. |
| TD-018 | Weak Tests | P3 | `src/emitter.rs:66-74` | `text_log_printed` and `non_text_log_silent` tests only verify "no panic" — they never capture or assert on stdout output. If `println!` silently breaks or the event-name guard is removed, these tests won't catch it. |
| TD-019 | Stale Docs | P2 | `src/headless.rs:41-48` | `#[allow(clippy::too_many_arguments)]` comment explicitly acknowledges the parameter count is a known problem: "The count will decrease when the save-picker and provider initialization are extracted into a shared setup struct (#future)." This is recorded technical debt. |

## In Progress

*(none)*

## Done

*(none)*
