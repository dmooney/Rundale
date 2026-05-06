# parish-types — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Dead Code | P2 | `src/time.rs:393-402,531-536` | `GameClock::check_festival_data` and `HasFestivalDate` trait have zero external callers or implementations anywhere in the workspace. Only the hardcoded `check_festival` (using `Festival` enum) is called externally (`parish-core/src/ipc/handlers.rs:30`, `parish-cli/src/debug.rs:181`, etc.). The data-driven path was scaffolded but never wired up. |
| TD-002 | Duplication | P2 | `src/lib.rs:29-41` & `parish-core/src/game_mod.rs:156-168` | `AnachronismEntry` struct is defined in both crates with identical fields (`term`, `category`, `origin_year`, `note`). `parish-types` version derives only `Deserialize`; `parish-core` version derives `Serialize + Deserialize`. `parish-npc` uses the `parish-types` version (fully-qualified path at `parish-npc/src/anachronism.rs:578,860`), while `parish-core` uses its own copy. |
| TD-003 | Weak Tests | P2 | `src/error.rs:1-44` | Zero tests for `ParishError`. No coverage for Display message strings, `#[from] Serialization` and `#[from] Io` conversions, or variant construction. Every other source file in this crate has tests. |
| TD-004 | Weak Tests | P2 | `src/time.rs:305-524` | `GameClock` pause/resume, inference_pause/inference_resume, `set_speed`, `current_speed`, `speed_factor()`, `start_game()`, `paused_game_time()`, `real_elapsed_secs()`, and `GameSpeed::from_name`/`activation_message` are untested. Only basic time-of-day, season, advance, SpeedConfig defaults, and DayType are covered. |
| TD-005 | Complexity | P2 | `src/ids.rs:229-345` | `extract_dialogue_from_partial_json` is 117 lines (exceeds 100-line threshold). Single function bundles JSON depth-aware scanning, escape-sequence handling, surrogate-pair decoding, and UTF-8 char-boundary logic. |
| TD-006 | Config/Cargo | P3 | `Cargo.toml:19` | `tokio-test` is declared as a dev-dependency but no test in this crate uses it (zero `tokio::test` or `tokio_test` references). |
| TD-007 | Stale Docs/Comments | P3 | `src/gossip.rs:201-206` | `distort` doc comment documents 4 distortion rules with weights 30-30-30-10 but the implementation only has 3 rules with a 33-33-34 split. The "Swap a name — not implemented without NPC name list (10% weight, skipped)" comment implies a 10% dead zone that doesn't exist in the code. |

## In Progress

*(none)*

## Done

*(none)*
