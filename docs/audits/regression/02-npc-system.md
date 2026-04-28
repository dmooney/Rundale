# Regression Audit: NPC System

Scope: Cognitive LOD (4 tiers), Entity Model, Intelligence Profile (6 dims),
Mood (20+ states), Relationships (7 types), Conversation/streaming, Anachronism
Detection, Improv Mode.

## 1. Sub-features audited

- Cognitive LOD: T1 (full LLM, same location), T2 (overhear, nearby), T3 (batch, distant), T4 (CPU rules, far)
- Entity model (identity, age, occupation, schedule, home/workplace, 20-entry memory ring buffer)
- Intelligence Profile (6 dims × 1–5; prompt-directive translation)
- Mood (20+ emoji states, updates from Tier 2)
- Relationships (7 types, strength −1.0..1.0, append-only history)
- Conversation (NL, streaming, overhear)
- Anachronism Detection (categories: Technology/Slang/Concept/Material/Measurement; hardcoded + mod `anachronisms.json`; word-boundary matching; injected into prompt)
- Improv Mode (`/improv`)

## 2. Coverage matrix

| Sub-feature | Unit | Integration / Fixture | Rubric | UI |
|---|---|---|---|---|
| Cognitive LOD dispatch (T1–T4) | `parish-npc/src/manager.rs`, `transitions.rs`, `tier4.rs` (369 in-source tests in `parish-npc`) | `parish-npc/tests/tier2_llm_integration.rs` (6 tests, mocked HTTP, success/HTTP-error/malformed-JSON/empty-choices) | none | none |
| Entity model + 20-entry memory | `parish-npc/src/memory.rs`, `data.rs` | `headless_script_tests.rs:701` `test_npc_locations_script_runs`; `:713` `test_npc_debug_npcs_lists_all` | none | `apps/ui/src/components/MoodIcon.test.ts` |
| Intelligence Profile (6 dims) | indirect via NPC fixtures in `parish-npc/src/data.rs` | none | none | none |
| Mood (20+ emoji) | `parish-npc/src/mood.rs` (in-source) | `headless_script_tests.rs:538-679` debug-driven mood/schedule/rels assertions | none | `MoodIcon.test.ts` (icon mapping) |
| Relationships (7 types, strength scale) | `parish-npc/src/data.rs` + `manager.rs` in-source tests | `headless_script_tests.rs:595` `test_debug_rels_all_npcs` | none | none |
| Conversation + streaming | `game_harness_integration.rs:213` canned-response tests; `parish-cli/tests/headless_script_tests.rs:1038` multiple-NPC | `apps/ui/src/lib/stream-pacing.test.ts` (UI streaming) | none | `apps/ui/src/components/ChatPanel.test.ts` |
| Overhear (T2 ambient) | `parish-npc/src/overhear.rs` (in-source) | `tier2_llm_integration.rs` 6 mocked-HTTP tests | none | none |
| Anachronism Detection | `parish-npc/src/anachronism.rs` (in-source) | `headless_script_tests.rs:275` `test_npc_response_surfaces_anachronism_terms`; `:310` empty-for-period; fixture `test_anachronism.txt` | `eval_baselines.rs:153` `rubric_anachronisms_are_empty` | none |
| Improv Mode (`/improv`) | none — toggle is a flag; no test references it | none | none | none |
| Gossip propagation | `parish-npc/tests/gossip_integration.rs` (3 tests covering seeded-tier2-event, trivial-noop, transitive-2-rounds) | none | none | none |
| Banshee (mythology hook) | `parish-npc/src/banshee.rs` (in-source) | fixtures `banshee_playtest.txt`, `banshee_flag_off.txt`, `banshee_playtest_close.txt` (run via `headless_script_tests.rs::test_fixture_*`) | none | none |

## 3. Strong spots

- Tier 2 inference pipeline has thorough mocked-HTTP coverage in
  `parish-npc/tests/tier2_llm_integration.rs:83-191` (success, HTTP error,
  malformed JSON, empty choices, missing optional fields).
- Anachronism detection is the **only** subsystem with all four layers:
  unit, fixture, integration assertion, and rubric. This is the gold standard.
- 369 in-source unit tests in `parish-npc` make it the most heavily-covered
  crate by raw count; mood/schedule/relationship debug commands are
  exercised via fixtures with semantic assertions.
- Banshee (mythology hook) is exercised by three dedicated fixtures.

## 4. Gaps

- **[P0] Tier 3 batch inference is unimplemented but unmarked.** features.md
  lists T3 batch (8–10 NPCs/call, daily) under "Cognitive LOD". `parish-npc`
  has no `tier3.rs` and no batch test. If this is "future" per the
  Implementation Status section, the regression risk is that a partial
  implementation merges silently. Suggested: add a guard test in
  `architecture_fitness.rs` asserting `tier3` either exists with tests or
  is documented as planned.
- **[P0] Tier 4 CPU rules engine has no end-to-end fixture.** `tier4.rs`
  has internal unit tests but no fixture exercises a "far away NPC has a
  life event" scenario. Add `testing/fixtures/test_tier4_far_npcs.txt` plus
  rubric `rubric_tier4_events_appear_in_journal`.
- **[P0] Tier promotion/demotion (proximity-driven) has no integration
  test.** Movement → tier reassignment is core to the LOD claim but no
  fixture asserts that walking close to an NPC promotes them to T1.
  Suggested: integration test in
  `crates/parish-cli/tests/world_graph_integration.rs` named
  `test_movement_promotes_nearby_npc_to_tier1`.
- **[P1] Intelligence Profile → prompt-directive mapping is untested.**
  Six dimensions × 5 levels = 30 directive variants; no unit test asserts
  what string lands in the prompt for `Verbal=5`, `Emotional=1`, etc.
  Suggested unit test in `parish-npc/src/data.rs`:
  `test_intelligence_profile_renders_prompt_directives`.
- **[P1] 20-entry memory ring buffer eviction unverified.** No test asserts
  the 21st entry evicts entry 1. Suggested unit test in
  `parish-npc/src/memory.rs`: `test_memory_evicts_oldest_at_capacity`.
- **[P1] `/improv` toggle has no test.** The flag exists in features.md but
  no fixture references `/improv`, no UI test toggles it, no unit test
  asserts the prompt diff between modes. Suggested fixture
  `testing/fixtures/test_improv_toggle.txt`.
- **[P1] Mood-to-prompt-context flow untested.** Mood updates are
  asserted at the data layer (`debug-rels`/`debug-mood` fixtures) but no
  test asserts an NPC's current mood actually appears in the next prompt.
  Suggested unit test in `parish-inference/src/lib.rs`:
  `test_prompt_includes_npc_mood`.
- **[P2] Anachronism category coverage** — the rubric only checks
  emptiness; no test asserts `Slang`, `Material`, or `Measurement`
  categories actually trigger (only `Technology` is exercised by
  `test_anachronism.txt`).

## 5. Recommendations

1. **Decide and document Tier 3 status** explicitly — either implement
   with tests or add an `architecture_fitness.rs` reminder so it can't
   silently half-ship.
2. **Add a tier-promotion integration test.** This is the headline LOD
   feature with no end-to-end proof.
3. **Test the prompt-side of the NPC pipeline.** Intelligence Profile,
   mood, relationship strength all flow into prompts but no test asserts
   what reaches the LLM. A single `parish-inference` test that renders a
   full prompt and asserts string fragments would catch a wide class.
4. **Cover Tier 4 with a fixture + rubric** so daily life-events on
   distant NPCs aren't silently broken.
