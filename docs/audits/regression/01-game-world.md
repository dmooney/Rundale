# Regression Audit: Game World

Scope: World Graph (BFS, fuzzy match, dynamic descriptions), Time System
(clock, day-periods, seasons, speed presets, `/pause` `/resume` `/wait`
`/tick`), Weather (states + transitions + tinting), Festivals (4 Irish),
Travel Encounters (`parish_world::encounter`).

## 1. Sub-features audited

- World Graph & BFS pathfinding
- Fuzzy / alias name matching
- Dynamic location descriptions (weather + NPC + time placeholders)
- Time System (clock, day-periods, seasons, speed presets, pause/resume/wait/tick)
- Weather state machine + dialogue/palette context
- Festivals (4 Irish, data-driven from mod)
- Travel Encounters wiring into `move_player`

## 2. Coverage matrix

| Sub-feature | Unit | Fixture | Rubric | UI | Gap |
|---|---|---|---|---|---|
| World Graph / BFS | `crates/parish-world/src/graph.rs:582` (load/get/neighbors); `crates/parish-cli/tests/world_graph_integration.rs:95` BFS path tests | `testing/fixtures/test_multi_hop.txt`; `testing/fixtures/test_all_locations.txt` | `eval_baselines.rs:144` (`baseline_test_all_locations`); `eval_baselines.rs:173` (movement minutes positive) | `apps/ui/src/components/MapPanel.test.ts` | No negative-path test for unreachable / disconnected subgraph |
| Fuzzy / alias matching | `crates/parish-world/src/movement.rs:268` partial-name; `world_graph_integration.rs:331-379` alias tests | `testing/fixtures/test_fuzzy_names.txt`; `headless_script_tests.rs:218` `test_fuzzy_pub_variants` | None — fuzzy fixture not in `BASELINED_FIXTURES` (`eval_baselines.rs:35`) | — | Fuzzy output drift undetected; no rubric for "ambiguous match returns disambiguation" |
| Dynamic descriptions | `crates/parish-world/src/description.rs:118-232` (placeholders, all-times, exits) | `testing/fixtures/play_weather.txt`; `test_walkthrough.txt` | `eval_baselines.rs:191` `rubric_look_descriptions_are_non_empty`; `headless_script_tests.rs:1077` `test_harness_look_contains_weather_info` | — | No rubric checking NPC-name interpolation; no test that festival placeholder renders |
| Time System / day-periods / seasons | `crates/parish-types/src/time.rs:560-670` (transitions, season-from-date, advance, day-types) | `test_time_progression.txt`; `test_pause_resume_cycle.txt`; `test_speed_assertions.txt` | `headless_script_tests.rs:381` `test_time_progresses_past_morning`; `:415-489` pause/resume/speed | — | No test exercising all 7 day-period transitions in one run; no `/tick` (single-tick) coverage; speed preset effect on `/wait` duration not asserted |
| Weather state machine | `crates/parish-world/src/weather.rs:253-381` (initial, min-duration, transitions, seasonal bias, no-skip, event published) | `testing/fixtures/play_weather.txt` | `headless_script_tests.rs:938` `test_harness_weather_consistent_at_all_locations`; `:1077` look-contains-weather | — | `play_weather.txt` is a `/play` script, not asserted in CI; no rubric validating 7-state coverage; no palette-tinting test |
| Festivals (4 Irish, data-driven) | `crates/parish-types/src/time.rs:594` `test_festival_detection`; `crates/parish-npc/src/tier4.rs:132` `check_festival_in_range`; `crates/parish-persistence/src/journal_bridge.rs:133` filtering | None | None | — | No fixture exercises a festival day; no rubric checks `FestivalStarted` event fires; data-driven path (`game_mod.rs:576` `check_festival`) untested |
| Travel Encounters wiring | `crates/parish-world/src/encounter.rs:135-302` exhaustive table + probability tests; `world_graph_integration.rs:217` distribution | None — no fixture asserts an encounter ever occurs in-game | None | — | **CRITICAL: `check_encounter` has zero callers in `parish-core` — `move_player`/movement code never invokes it.** `grep -rn check_encounter crates apps` shows only tests. |

## 3. Strong spots

- Movement & graph BFS coverage is solid: 14+ integration tests in
  `world_graph_integration.rs` exercise loading, connectivity, multi-hop,
  alias resolution, and indoor/mythological tagging.
- Description rendering has tight unit coverage (`description.rs:118-232`)
  plus a rubric (`eval_baselines.rs:191`) that catches empty / unrendered
  placeholders across all baselined fixtures.
- Weather state machine internals (min-duration gate, no-skip transitions,
  seasonal bias, event publication) are well unit-tested in
  `weather.rs:253-381`.
- The eval-baseline harness (`eval_baselines.rs`) gives deterministic
  capture-on-green/diff-on-red regression sensors for movement, walkthrough,
  and all-locations fixtures.

## 4. Gaps

- **[P0] Travel encounters never fire in-game** — sub-feature: Travel
  Encounters. `parish_world::encounter::check_encounter` has no callers in
  `parish-core::game_session` or `parish-core::ipc::handlers`; the function
  is exercised only by its own internal `#[cfg(test)]` block and by a
  distribution test in `world_graph_integration.rs:217`. Suggested test:
  integration test in `crates/parish-cli/tests/world_graph_integration.rs`.
  Suggested name: `test_move_player_invokes_encounter_check_with_seeded_rng`.
- **[P0] No festival fixture / rubric** — sub-feature: Festivals.
  `Festival::check` is unit-tested but the four Irish festival days are
  never exercised end-to-end. Suggested test: fixture
  `testing/fixtures/test_festival_samhain.txt` plus rubric in
  `eval_baselines.rs`. Suggested name:
  `rubric_festival_event_published_on_festival_date`.
- **[P1] Fuzzy-name fixture is not baselined** —
  `test_fuzzy_names.txt` is excluded from `BASELINED_FIXTURES`
  (`eval_baselines.rs:35`), so fuzzy-match output regressions go silent.
  Suggested: add to baselined list once stable. Test name:
  `baseline_test_fuzzy_names`.
- **[P1] `/tick` (single-tick advance) untested** — none of the headless
  tests in `headless_script_tests.rs` exercise `/tick`; only `/wait <n>` is
  covered (`:381-489`). Suggested test in `headless_script_tests.rs`:
  `test_tick_advances_clock_by_minimum_unit`.
- **[P1] Speed preset effect on real-time tick not asserted** —
  `test_speed_presets_acknowledged` (`:489`) only checks the response
  string; the actual scaling factor (`SpeedConfig`) is not verified
  end-to-end. Suggested unit test in `parish-types/src/time.rs`:
  `test_speed_preset_scales_advance_rate`.
- **[P2] Weather-driven palette tinting untested** — features.md lists
  palette tinting but no test in `crates/parish-palette` or world tests
  asserts the weather-to-palette mapping. Suggested test in palette crate:
  `test_palette_tint_for_each_weather_state`.
- **[P2] Disambiguation on ambiguous fuzzy match unverified** — fuzzy
  tests assert successful resolution but no test covers the "multiple
  candidates" branch. Suggested unit test in
  `crates/parish-world/src/movement.rs`:
  `test_resolve_ambiguous_partial_returns_disambiguation`.

## 5. Recommendations

1. **Wire encounters into `move_player` and add an integration test.**
   This is a P0 silent-feature bug: encounters compile, are unit-tested,
   but never run for the player. Add a deterministic-RNG test in
   `world_graph_integration.rs` that drives a movement and asserts
   `EncounterEvent` surfaces.
2. **Create a festival fixture + rubric** so the four Irish festival days
   are exercised end-to-end against the data-driven path
   (`game_mod.rs:576`), and `FestivalStarted` events are observed via the
   journal bridge.
3. **Promote `test_fuzzy_names.txt` into `BASELINED_FIXTURES`** once
   determinism is confirmed; this closes the largest silent-drift hole in
   the world-graph area.
4. **Add structural rubrics for weather state coverage and `/tick`**
   advancement to `eval_baselines.rs`; both are inexpensive and catch
   whole-class regressions.
