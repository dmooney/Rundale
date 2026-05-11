# parish-types — Technical Debt

## Open

| ID | Category | Severity | Resolution |
|----|----------|----------|------------|
| TD-008 | Dead Code | P3 | `GameSpeed::factor_with_config` (`src/time.rs:252`) is `pub` but only ever called by `GameSpeed::factor` (`src/time.rs:248`); no other workspace crate references it (`rg -n "factor_with_config" parish/` returns just those two hits). Either make it private or wire it into a code path that takes a runtime `SpeedConfig`. |
| TD-009 | Dead Code | P3 | `ConversationLog::all` (`src/conversation.rs:141-143`) and `ConversationLog::last_speaker_at` (`src/conversation.rs:72-78`) are documented as "Used by the debug panel" / public helpers but no caller exists outside this file (workspace search for `\.all\(\)`/`\.last_speaker_at\(` against `conversation_log` yields zero hits). Drop them or wire the debug panel to use them. |
| TD-010 | Dead Code | P3 | `GossipNetwork::recent` (`src/gossip.rs:126-131`) is only called by its own unit test (`src/gossip.rs:414`); no production caller (the only other `.recent(` hits in the workspace are on `npc.memory`, a different type). Either delete or surface it from the debug snapshot. |
| TD-011 | Stale Docs | P3 | Module doc-comment at `src/events.rs:6-8` references `crate::persistence::journal::WorldEvent`, but `parish-types` has no `persistence` module — that type lives in `parish-persistence`. The intra-doc link is broken; rewrite the comment to name the crate path or drop the link. |
| TD-012 | Stale Docs | P3 | `Cargo.toml:7` description claims "zero internal deps", which `lib.rs` echoes. The crate genuinely has no `parish-*` deps, but `error.rs:18-27` notes that `Database` and `Network` exist *because* `parish-types` cannot depend on `rusqlite`/`reqwest`. The description is fine; what's stale is the README — `README.md` lists "ids — strongly typed IDs and core world entity structs" without mentioning that `ids.rs` also hosts `extract_dialogue_from_partial_json`, `floor_char_boundary`, and `Weather`, all of which are public surface that callers must discover. Update the module list. |
| TD-013 | Inconsistency | P3 | Serde derive coverage is uneven: `Season`, `DayType`, `LocationId`, `NpcId`, `LanguageHint` all derive `Serialize + Deserialize`, but `Festival` (`src/time.rs:182-192`), `TimeOfDay` (`src/time.rs:67-83`), `Weather` (`src/ids.rs:17-26`), and `GameSpeed` (`src/time.rs:222-234`) do not. `SpeedConfig` (`src/time.rs:17`) only has `Deserialize`, never `Serialize`. Persistence and snapshot code currently has to round-trip these via `Display`/`from_str`; adding the derives would let snapshots serialize the speed/time-of-day directly. |
| TD-014 | Complexity / Perf | P3 | `GossipNetwork::create` (`src/gossip.rs:63-86`) re-sorts the entire `items` vector by timestamp on every overflow insert (`sort_unstable_by_key` + `drain`, O(n log n) per call once at capacity). Since insertions are already monotonically time-ordered in practice (game time only moves forward), the sort is wasted work — replace with a `VecDeque` + `pop_front`, mirroring `ConversationLog`'s ring-buffer pattern (which the comment at `src/gossip.rs:16-18` already claims to follow). |
| TD-015 | Weak Tests | P3 | `Festival::Display` (`src/time.rs:207-216`), `Season::Display` (`src/time.rs:131-140`), `TimeOfDay::Display` (`src/time.rs:85-97`), and `GameSpeed::Display` (`src/time.rs:286-296`) have no direct tests — only `DayType::Display` is covered (`src/time.rs:640-644`). Add round-trip / canonical-string tests so renames don't silently change UI strings. |
| TD-016 | Weak Tests | P3 | `EventBus`'s lag/overflow semantics are documented at `src/events.rs:19-20` and `src/events.rs:160-164` ("subscribers that fall behind by more than `BUS_CAPACITY` events will receive `RecvError::Lagged`") but no test exercises the lag path. Add a test that publishes >256 events without draining and asserts the receiver observes `RecvError::Lagged`. |
| TD-017 | Weak Tests | P2 | `GameClock::set_speed` while frozen (`src/time.rs:465-474`) has the early return branch but no test verifies that mutating speed during a player-pause does not leak real time on resume. The current `test_set_speed_changes_factor` (`src/time.rs:736-742`) only exercises the running path. Add a test: pause -> set_speed(Fast) -> resume -> assert no game-time jump. |

## In Progress

*(none)*

## Done

| ID | Category | Severity | Resolution |
|----|----------|----------|------------|
| TD-001 | Dead Code | P2 | Removed `check_festival_data` method and `HasFestivalDate` trait from `src/time.rs`. Updated module-level and method doc comments to remove stale references to the data-driven festival path. |
| TD-002 | Duplication | P2 | Added `Serialize` derive to `AnachronismEntry` in `src/lib.rs`, then removed the duplicate `AnachronismEntry` from `parish-core/src/game_mod.rs` and made `parish-core` import from `parish-types` instead (cross-crate). |
| TD-003 | Weak Tests | P2 | Added 13 tests covering all `ParishError` variant Display messages, `#[from]` conversions (serde_json::Error, std::io::Error), and variant construction. |
| TD-004 | Weak Tests | P2 | Added 17 tests covering `GameClock` pause/resume, inference_pause/inference_resume, set_speed/current_speed, speed_factor(), start_game(), paused_game_time(), real_elapsed_secs(), GameSpeed::from_name, GameSpeed::activation_message, and GameClock::with_speed. |
| TD-005 | Complexity | P2 | Extracted `handle_json_unicode_escape` helper from `extract_dialogue_from_partial_json`. Both functions are now under 100 lines. No behavior change. |
| TD-006 | Config/Cargo | P3 | Removed unused `tokio-test` dev-dependency from `Cargo.toml`. |
| TD-007 | Stale Docs/Comments | P3 | Updated `distort` doc comment in `src/gossip.rs` to reflect the actual 3 distortion rules (~33% each), removing the non-existent "Swap a name" rule and the 30-30-30-10 weight claim. |

## Discovery

2026-05-07 — All TODO items resolved. Discovery scan of `parish/crates/parish-types` found no additional credible technical debt within scope. Crate has clean tests (116 passing), zero clippy warnings, and no unused dependencies.
