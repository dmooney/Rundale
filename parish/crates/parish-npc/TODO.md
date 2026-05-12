# parish-npc — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-002 | Duplication | P2 | `src/ticks.rs:1075-1097`, `src/reactions.rs:1127-1154` | Two modules still define their own `make_test_npc`/`test_npc` helpers instead of reusing `test_helpers::make_test_npc`. ticks.rs has ~35 call sites and a different interface (takes name, age=40, personality="Friendly"). reactions.rs has a more complex helper with custom intelligence/mood defaults. Both require careful auditing to avoid test-isolation changes. |
| TD-010 | Complexity | P2 | `src/reactions.rs:1-2017` | `reactions.rs` is the largest file in the crate (~2,017 lines) containing emoji reactions, arrival reactions, and LLM greeting/resolution. Should split into `emoji_reactions.rs`, `arrival_reactions.rs`, and keep the shared palette in `reactions.rs`. Requires careful module-public-API coordination. |
| TD-011 | Complexity | P3 | `src/lib.rs:405-460` | `build_tier1_system_prompt()` contains a ~55-line `format!()` call with inline JSON examples. Template extraction is brittle because placeholders span multiple indentation levels. Low risk — function is well-documented and tested. |

## In Progress

*(none)*

## Done

| ID | Category | Description |
|----|----------|-------------|
| TD-001 | Weak Tests | Added 6 tests for death system: multiple simultaneous dooms (herald + death), DOOM_HERALD_WINDOW_HOURS boundary (exactly 12h, 12h+1m), clock rewind (doesn't double-herald, past-doom rewind safe). |
| TD-003 | Complexity | Extracted `resolve_cuaird_location()` and `needs_weather_shelter()` from `tick_schedules()` (~130 lines → two focused helpers + slim loop body). |
| TD-004 | Complexity | Extracted `select_reaction_kind()` (kind-selection chain) and `cap_reactions_by_priority()` (truncation pass) from `generate_arrival_reactions()` (~85 lines). |
| TD-005 | Complexity | Extracted 8 context-block helpers (`interlocutor_block`, `other_npcs_block`, `conversation_block`, `continuity_block`, `reactions_block`, `stm_block`, `ltm_block`, `gossip_block`) from `build_enhanced_context_with_config()` (~100 lines). |
| TD-006 | Dead Code | Removed `month_name()` (duplicated `chrono::format("%B")`), replaced call site with `now.format("%B")`. |
| TD-007 | Weak Tests | Added 5 direct unit tests for `find_eligible_couples()`: normal pair, one ill, both outside age range, one in range, duplicate romantic relationships. |
| TD-008 | Weak Tests | Added 4 boundary tests for `pick_next_speaker()`: strength exactly 0.1 (no bonus), just above 0.1 (bonus), negative -0.1001 (abs check), exactly 0.5 threshold (eligible). |
| TD-009 | Weak Tests | Added `dead_npc_excluded_from_birth_check`: brute-force seed search to find a tick where age-100 NPC dies, then verifies no birth involves the dead NPC. |
| TD-012 | Dead Code | Removed unused `MemoryKind::ReceivedGossip` variant. |
| TD-013 | Dead Code | Removed unused `tempfile` from `[dev-dependencies]`. |
| TD-014 | Stale Docs | Updated `CogTier` doc: removed "future" labels from Tier 3 and Tier 4 descriptions. |
| TD-015 | Dead Code | Removed `DailySchedule` struct/impl (superseded by `SeasonalSchedule`), converted 3 tests to use `SeasonalSchedule`. |
| TD-002 (partial) | Duplication | Replaced local `make_npc` helpers in `transitions.rs` and `autonomous.rs` with `test_helpers::make_test_npc`. |
| TD-016 | Dead Code | Removed `build_action_line` (only used in tests), folded tests into `build_named_action_line`. |
| TD-017 | Dead Code | Removed `extract_intended_references`, `format_reference_hint`, and `ReferencePrePassResponse` (unused). |
| TD-018 | Duplication | Extracted `push_entry` and `format_lines` helpers in `reactions.rs` to deduplicate `add`/`add_player_message_reaction` and `context_string`/`npc_context_string`. |
| TD-019 | Duplication | Replaced `tier1_npcs`/`tier2_npcs`/`tier3_npcs`/`tier4_npcs` with single `npcs_in_tier(tier: CogTier)` in `manager.rs`. |
| TD-020 | Duplication | Collapsed tier-state management copy-paste into `TierTickState` struct in `manager.rs`. |
| TD-021 | Complexity | Table-drove `Intelligence::prompt_guidance` using per-dimension lookup tables and a shared `push_guidance` helper. |
| TD-022 | Complexity | Extracted `apply_illness`, `apply_recovery`, `apply_death`, `apply_birth`, `apply_seasonal_shift`, `apply_trade`, `apply_festival_detected`, `apply_festival_bond` helpers from `tier4::apply_events`. |
| TD-023 | Logic Smell | Fixed Death-without-banshee path in `tier4.rs` to use `npcs.remove(npc_id)` atomically and only emit event when NPC exists. |
| TD-024 | Logic Smell | Fixed Birth arm empty parent names in `tier4.rs` by early-returning if either parent is missing from the NPC map. |
| TD-025 | Dead Code | Downgraded `build_enhanced_system_prompt`, `build_enhanced_context`, `apply_tier1_response`, and `apply_tier2_event` no-config shims to `#[cfg(test)] pub(crate)`; updated external call sites to use `_with_config` variants. |

## Progress Log

- **2026-05-11**: Completed TD-016 through TD-025. All changes behavior-safe; 400 parish-npc tests pass; clippy clean on parish-npc, parish-core, parish-tauri, and parish-cli.

## Follow-up

- **TD-002 (ticks.rs, reactions.rs)**: Remaining modules with local test helpers. ticks.rs has ~35 call sites with different interface (takes name, age=40, "Friendly" personality). reactions.rs has custom intelligence/mood defaults. Both need careful per-call-site audit.

- **TD-010**: reactions.rs split into modules. Emoji reactions (lines 1-353), arrival reactions (lines 354-650), and LLM greeting/resolution (651-~1000?) are distinct subsystems. Safe to split but requires updating all `pub use` re-exports.

- **TD-011**: build_tier1_system_prompt contains a 55-line format!() with inline JSON. Low risk, well-tested — defer unless the template changes.
