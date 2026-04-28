# Regression Audit: Mod System

Scope: Factorio-style separation of engine and content. Mod files: `mod.toml`,
`world.json`, `npcs.json`, `prompts/*.txt`, `anachronisms.json`,
`festivals.json`, `encounters.json`, `loading.toml`, `ui.toml`,
`pronunciations.json`, `transport.toml`. Default mod: `mods/rundale/`.

## 1. Sub-features audited

- Engine/content separation
- Per-file loaders for each of 11 mod artefacts
- Default mod load: `mods/rundale/` (22 locations, 23 NPCs, 4 festivals, 25 phrases)
- Prompt template `{placeholder}` interpolation
- Mod swap (multiple mods)
- Schema versioning

## 2. Coverage matrix

| Mod artefact | Loader test | Schema/error test | Integration (full mod load) |
|---|---|---|---|
| `mod.toml` (manifest) | `parish-config/src/engine.rs` in-source (load_engine_config) — 66 in-source tests in `parish-config` | partial — TOML errors covered for `parish.toml` (`provider.rs:1032` invalid-toml) but not for `mod.toml` specifically | implicit via every fixture that boots the harness |
| `world.json` | `parish-world/src/lib.rs:131` `from_parish_file` + `:303-422` 8 in-source tests; `parish-cli/tests/world_graph_integration.rs:27-379` 28 tests | `world_graph_integration.rs:54` `test_parish_all_connections_bidirectional` (catches asymmetric edges) | `world_graph_integration.rs:37` location-count, `:43` crossroads-hub, `:125` reachability |
| `npcs.json` | `parish-npc/src/data.rs` (in-source) | none — no malformed-NPC test discovered | indirect via `headless_script_tests.rs` debug fixtures |
| `prompts/tier1_*.txt`, `tier2_system.txt` | `parish-core/src/prompts/` (in-source) | none — no test asserts prompt interpolation `{placeholder}` substitution | none |
| `anachronisms.json` | `parish-npc/src/anachronism.rs` (in-source — hardcoded fallback dictionary) | none — no test for malformed mod `anachronisms.json` | `eval_baselines.rs:153` `rubric_anachronisms_are_empty` (data-driven path covered) |
| `festivals.json` | `parish-types/src/time.rs:594` `test_festival_detection`; mod path in `parish-core/src/game_mod.rs:576` `check_festival` | none — no malformed test, no per-festival assertion | none |
| `encounters.json` | `parish-world/src/encounter.rs:201` `test_encounter_with_table_uses_mod_text` | none | none — and the function isn't called at runtime (cross-ref Game World audit) |
| `loading.toml` (spinner phrases/colors) | none discovered | none | none |
| `ui.toml` (sidebar labels, accent) | none discovered | none | none |
| `pronunciations.json` | none discovered | none | none |
| `transport.toml` | `parish-world/src/transport.rs` (in-source) | none for malformed mod transport file | `game_harness_integration.rs:543-630` (3 tests: scaling, label surfacing, slower transport) |

## 3. Strong spots

- `world.json` is the most thoroughly validated artefact: parsing,
  reachability, bidirectional-connection invariants, alias resolution,
  and computed travel times all enforced (`world_graph_integration.rs:27-422`).
- The default Rundale mod is implicitly load-tested by every
  fixture-driven test that boots the harness — so a totally broken
  manifest would fail loudly. ~74 fixture tests in
  `headless_script_tests.rs` constitute this implicit smoke test.
- Transport-file integration is tested end-to-end via three integration
  tests in `game_harness_integration.rs`.

## 4. Gaps

- **[P0] No malformed-mod tests.** None of `mod.toml`, `npcs.json`,
  `festivals.json`, `loading.toml`, `ui.toml`, `pronunciations.json`,
  `transport.toml`, `encounters.json`, or `anachronisms.json` has a
  test that loads a deliberately broken file and asserts a useful
  error (vs. panic, silent-empty, or partial-load). For a mod-driven
  engine this is the highest-value missing class. Suggested: a
  `parish-config/tests/mod_validation.rs` table-driven test using
  `testing/fixtures/mods/<bad-name>/*` files.
- **[P0] Prompt template `{placeholder}` interpolation is untested.**
  features.md highlights template interpolation but no test asserts
  that `{npc_name}`, `{location}`, `{weather}`, `{time}` etc. are
  replaced. A bug here silently breaks every NPC prompt. Suggested
  unit test in `parish-core/src/prompts/`:
  `test_render_template_substitutes_all_placeholders`.
- **[P0] No "round-trip mod-load assertion".** No test loads
  `mods/rundale/` and asserts `locations.len() == 22`,
  `npcs.len() == 23`, `festivals.len() == 4`, `loading_phrases.len() == 25`.
  features.md states these counts; if a content edit drifts them, no
  test catches it. Suggested:
  `parish-config/tests/rundale_mod_smoke.rs::rundale_loads_with_expected_counts`.
- **[P1] Mod swapping (loading a different mod) has no test.** The
  whole engine/content separation premise is untested at the
  multi-mod level. Suggested integration test that loads a minimal
  `testing/fixtures/mods/empty/` mod and verifies engine boots.
- **[P1] Schema versioning is not present.** `mod.toml` has no version
  field and no migration story. Suggested: add `schema_version` field
  and a unit test asserting unknown versions reject cleanly.
- **[P1] `loading.toml`, `ui.toml`, `pronunciations.json`, `npcs.json`
  loaders have zero tests.** Each is read at startup but a regression
  in any would either panic or silently degrade.
- **[P2] Unknown-key handling unspecified.** Serde defaults to ignoring
  unknown keys; no test asserts whether typos in a mod file are
  surfaced or swallowed.

## 5. Recommendations

1. **Add a `parish-config/tests/mod_validation.rs` table-driven malformed-input
   suite** covering all 11 mod artefacts with one bad-input row each. This
   is the highest-leverage single PR for this area.
2. **Add `rundale_loads_with_expected_counts`** — pins the default mod's
   shape so accidental content edits surface. Costs 10 lines.
3. **Test prompt-template interpolation directly** — silent NPC
   degradation is a bad failure mode.
4. **Decide on schema versioning** before mods diverge in the wild.
