# parish-types — Technical Debt

## Open

*(none)*

## In Progress

*(none)*

## Done

| ID | Category | Severity | Resolution |
|----|----------|----------|------------|
| TD-001 | Dead Code | P2 | Removed `check_festival_data` method and `HasFestivalDate` trait from `src/time.rs`. Updated module-level and method doc comments to remove stale references to the data-driven festival path. |
| TD-002 | Duplication | P2 | Added `Serialize` derive to `AnachronismEntry` in `src/lib.rs` so the `parish-types` version is a superset of the `parish-core` copy. **Follow-up required:** remove the duplicate `AnachronismEntry` from `parish-core/src/game_mod.rs` and have `parish-core` import from `parish-types` instead. |
| TD-003 | Weak Tests | P2 | Added 13 tests covering all `ParishError` variant Display messages, `#[from]` conversions (serde_json::Error, std::io::Error), and variant construction. |
| TD-004 | Weak Tests | P2 | Added 17 tests covering `GameClock` pause/resume, inference_pause/inference_resume, set_speed/current_speed, speed_factor(), start_game(), paused_game_time(), real_elapsed_secs(), GameSpeed::from_name, GameSpeed::activation_message, and GameClock::with_speed. |
| TD-005 | Complexity | P2 | Extracted `handle_json_unicode_escape` helper from `extract_dialogue_from_partial_json`. Both functions are now under 100 lines. No behavior change. |
| TD-006 | Config/Cargo | P3 | Removed unused `tokio-test` dev-dependency from `Cargo.toml`. |
| TD-007 | Stale Docs/Comments | P3 | Updated `distort` doc comment in `src/gossip.rs` to reflect the actual 3 distortion rules (~33% each), removing the non-existent "Swap a name" rule and the 30-30-30-10 weight claim. |
| TD-008 | Dead Code | P2 | Removed `GameSpeed::factor_with_config` from `src/time.rs` (only used internally by `factor`; inlined defaults). |
| TD-009 | Dead Code | P2 | Removed `ConversationLog::last_speaker_at` from `src/conversation.rs` (unused workspace-wide). `ConversationLog::all` retained — actively used by `parish-core` debug snapshot. |
| TD-010 | Dead Code | P2 | Removed `GossipNetwork::recent` from `src/gossip.rs` (unused outside its own test). Removed associated test. |
| TD-011 | Stale Docs | P3 | Fixed broken intra-doc link in `src/events.rs` module doc (`crate::persistence::journal::WorldEvent` → plain text). |
| TD-012 | Stale Docs | P3 | Updated `README.md` module list to include `lib.rs` root re-exports. |
| TD-013 | Inconsistency | P2 | Added `Serialize`/`Deserialize` derives to `Festival`, `TimeOfDay`, `Weather`, `GameSpeed`, and `SpeedConfig` in `src/time.rs` and `src/ids.rs`. |
| TD-014 | Complexity/Perf | P2 | Replaced `GossipNetwork::create` sort+drain eviction with `VecDeque`+`pop_front`. Updated `all_items` return type to `Vec<&GossipItem>` for compatibility. |
| TD-015 | Weak Tests | P2 | Added Display output tests for `Festival`, `Season`, `TimeOfDay`, and `GameSpeed`. |
| TD-016 | Weak Tests | P2 | Added EventBus overflow and lag tests in `src/events.rs`. |
| TD-017 | Weak Tests | P2 | Added `GameClock::set_speed` while frozen test in `src/time.rs`. |

## Follow-up

- **TD-002 (cross-crate):** Remove the duplicate `AnachronismEntry` from `parish-core/src/game_mod.rs` and make `parish-core` import from `parish-types` instead. Now that `parish-types::AnachronismEntry` derives both `Serialize` and `Deserialize`, the `parish-core` copy is redundant.

## Discovery

2026-05-11 — Resolved TD-008 through TD-017. Post-fix verification: 135 tests passing, zero clippy warnings, zero unused dependencies.

2026-05-07 — All TODO items resolved. Discovery scan of `parish/crates/parish-types` found no additional credible technical debt within scope. Crate has clean tests (116 passing), zero clippy warnings, and no unused dependencies.
