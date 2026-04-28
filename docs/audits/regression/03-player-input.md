# Player Input — Regression Coverage Audit

**Scope:** NL intents (Move/Talk/Look/Interact/Examine/Unknown — local keyword + LLM
fallback) and the slash-command surface enumerated in `docs/features.md`.
**Sources sampled:** `crates/parish-input/src/{lib.rs,parser.rs,commands.rs,intent_*.rs}`,
`crates/parish-input/tests/llm_fallback_integration.rs`,
`crates/parish-cli/tests/{headless_script_tests.rs,persistence_integration.rs,game_harness_integration.rs,eval_baselines.rs}`,
`apps/ui/src/lib/slash-commands.{ts,test.ts}`, `testing/fixtures/test_*.txt`.

## 1. Sub-features audited

- **NL intent parsing** — `parse_intent_local` (sync keyword shortcut) and
  `parse_intent` (async LLM fallback) → `IntentKind::{Move,Talk,Look,Interact,Examine,Unknown}`.
- **Slash commands** grouped per features.md:
  - Game Control: `/pause /resume /quit /new /status /time /where /npcs /wait /tick /help /about`
  - Save/Load: `/save /fork /load /branches /log`
  - Display: `/map /designer /theme /irish /improv /speed`
  - Feature Flags: `/flags /flag {list,enable,disable}`
  - Provider (base): `/provider /model /key`
  - Provider (cloud legacy): `/cloud /cloud {provider,model,key}`
  - Per-Category dot-notation: `/provider.<cat> /model.<cat> /key.<cat>` for
    `cat ∈ {dialogue, simulation, intent, reaction}`
  - Debug: `/debug /spinner`
  - Show/set unified pattern (no arg = show, arg = set)
- **Mention extraction** (`@Name …`) feeding Talk intents.
- **Branch-name validation** for `/load` and `/fork`.

## 2. Slash-command coverage roll-up

Counting the distinct command tokens in features.md scope (treating each per-category
dot variant as its own token): **41 tokens**.

- **Referenced in any test/fixture:** **34 / 41** (≈83%).
- **Referenced ONLY in unit tests (no fixture / integration / E2E):** /about, /irish,
  /improv, /designer, /theme, /provider, /model, /key, /cloud (+subs),
  /provider.<cat>, /model.<cat>, /key.<cat> (12 cat tokens), /unexplored.
- **Untested anywhere (no grep hit in tests, fixtures, UI tests):**
  - `/help` arg-less behavior is asserted but not "show/set" (none expected).
  - **`reaction` per-category variant** — `parse_input` test_parse_category_*
    exercises only `dialogue`/`simulation`/`intent` (lib.rs:998–1074); the
    `reaction` category is not asserted (file does include it via
    `InferenceCategory`, but no test).
  - **No frontend UI tests** for any slash command except `/unexplored`
    (`apps/ui/src/lib/slash-commands.test.ts` — 23 lines, single command).
  - **No E2E (Playwright) coverage** found for slash-command dispatch — the
    autocomplete registry exists in `slash-commands.ts` but is never exercised
    end-to-end via the UI harness.

## 3. Coverage matrix

| Sub-feature                       | Unit (input crate)                                | Fixture                                  | UI unit                                     | E2E   | Gap                                          |
|-----------------------------------|---------------------------------------------------|------------------------------------------|---------------------------------------------|-------|----------------------------------------------|
| NL local keyword (Move)           | lib.rs:160–470 (≈25 verb tests)                   | test_movement_verbs.txt, test_aliases.txt| —                                           | —     | none                                         |
| NL local keyword (Talk first-person)| lib.rs:223–246                                  | —                                        | —                                           | —     | minor (Talk shape only)                      |
| NL local keyword (Look)           | lib.rs:194–215                                    | test_look_variants.txt                   | —                                           | —     | none                                         |
| NL local keyword (Interact/Examine)| —                                                | —                                        | —                                           | —     | **gap — no `Interact` local test**           |
| NL LLM fallback (success)         | tests/llm_fallback_integration.rs:39–57           | —                                        | —                                           | —     | none                                         |
| NL LLM fallback (HTTP/JSON err)   | llm_fallback_integration.rs:59–96                 | —                                        | —                                           | —     | none                                         |
| Game Control: pause/resume/status | lib.rs:57–66; headless_script_tests.rs:159,424–454| test_commands.txt, test_pause_resume_cycle.txt| —                                       | —     | covered                                      |
| Game Control: quit/help           | lib.rs:30–35,62; headless_script_tests.rs:1126,1133| test_commands.txt                       | —                                           | —     | covered                                      |
| Game Control: new/about           | lib.rs:519–528,591–594                            | —                                        | —                                           | —     | not in fixtures                              |
| Game Control: time/where/npcs/wait/tick | lib.rs:569–598                              | test_new_commands.txt                    | —                                           | —     | covered                                      |
| Save/Load: save/load/fork/branches/log | lib.rs:36–55,86,1086–1107; persistence_integration.rs:17–342 | —                  | —                                           | —     | covered                                      |
| Display: /speed                   | lib.rs:776–828,1170                               | test_speed.txt, test_speed_assertions.txt; headless_script_tests.rs:495–514| —              | —     | covered                                      |
| Display: /map                     | lib.rs:529–557                                    | test_new_commands.txt                    | —                                           | —     | minor: only show-form fixture                |
| Display: /irish /improv /about /designer /theme | lib.rs:483–567,600–627                  | —                                        | —                                           | —     | **fixture/E2E gap — unit only**              |
| Feature Flags                     | (none — parser-side only)                         | test_flags.txt; headless_script_tests.rs:1268–1322| —                                  | —     | **no parser unit test for /flag /flags**     |
| Provider base (`/provider /model /key`)| lib.rs:657–717                               | —                                        | —                                           | —     | **fixture/E2E gap — unit only**              |
| Provider cloud (`/cloud …`)       | lib.rs:719–773,1111–1143                          | —                                        | —                                           | —     | **fixture/E2E gap — unit only**              |
| Per-category `/x.<cat>`           | lib.rs:998–1082 (dialogue/simulation/intent)      | —                                        | —                                           | —     | **`reaction` not asserted; no fixture/E2E**  |
| Debug                             | lib.rs:933–965                                    | test_debug.txt, test_debug_at_locations.txt, test_debug_all_npcs.txt; headless_script_tests.rs:543–816 | — | —     | covered                                      |
| Spinner                           | lib.rs:969–993                                    | —                                        | —                                           | —     | **no integration/fixture coverage**          |
| Show/set unified pattern          | lib.rs (per-command pairs)                        | —                                        | —                                           | —     | not asserted as a *property* across commands |
| Slash autocomplete registry       | —                                                 | —                                        | slash-commands.test.ts:1–23 (only `/unexplored`)| —     | **31/32 commands not asserted in registry** |
| Mention extraction                | lib.rs:831–898                                    | —                                        | —                                           | —     | covered                                      |
| Branch-name validation            | lib.rs:900–929,1098–1166                          | —                                        | —                                           | —     | covered                                      |

## 4. Strong spots

- Movement keyword parsing is exhaustively asserted: ≈25 verbs (saunter, mosey,
  meander, sprint, traipse, …) plus case-insensitive and bare-verb branches
  (`crates/parish-input/src/lib.rs:160–482`). Cross-checked against
  `testing/fixtures/test_movement_verbs.txt` and `test_aliases.txt`.
- LLM fallback HTTP path is mocked via wiremock for success / 5xx / malformed JSON
  / missing-field branches — `crates/parish-input/tests/llm_fallback_integration.rs:39–133`.
- Persistence commands (`/save /load /fork /branches /log`) have a dedicated
  end-to-end integration suite at `crates/parish-cli/tests/persistence_integration.rs`
  exercising real round-trips.
- Feature-flag commands have a fixture (`testing/fixtures/test_flags.txt`) **and**
  headless integration coverage (`headless_script_tests.rs:1268–1322`), including
  invalid-name tolerance.

## 5. Gaps

- **[P0] Per-category `reaction` variant has no test.** `lib.rs:998–1082` covers
  `dialogue`, `simulation`, `intent` only — `/model.reaction`, `/provider.reaction`,
  `/key.reaction` are untested. Add unit cases mirroring `test_parse_category_*`.
  *Test type:* unit. *Suggested name:* `test_parse_category_reaction_show_set` in
  `crates/parish-input/src/lib.rs`.
- **[P0] Slash autocomplete registry is essentially untested.**
  `apps/ui/src/lib/slash-commands.test.ts` asserts only `/unexplored` (1/32). A
  registry drift between Rust parser and frontend list would silently regress.
  *Test type:* UI unit. *Suggested:* parameterized test that every backend command
  in `parse_system_command` appears in `SLASH_COMMANDS` (snapshot or table-driven).
- **[P0] `Interact` IntentKind has no local-parse test.** `intent_local.rs` and
  `lib.rs` tests cover Move/Talk/Look but not Interact/Examine via the local
  shortcut path; only the LLM-mocked `Examine` case exists
  (`llm_fallback_integration.rs:117–132`). *Test type:* unit. *Fixture:*
  `test_local_parse_interact_*` in `lib.rs`.
- **[P1] Provider/Model/Key (base + cloud + per-category) lack fixture coverage.**
  All assertions live in inline unit tests; no `testing/fixtures/test_provider*.txt`
  or `test_cloud*.txt` exists, so the harness never observes the show/set output
  shape end-to-end. *Test type:* fixture + baseline.
- **[P1] Display toggles `/irish /improv /about /designer /theme` lack
  fixture/E2E proof.** Parser-only (`lib.rs:483–627`); the actual UI/state
  side-effect is unverified by any harness. *Test type:* headless integration in
  `headless_script_tests.rs` or a `test_display_toggles.txt` fixture.
- **[P1] `/spinner` only has parser unit tests** (`lib.rs:969–993`); no integration
  proves the spinner duration actually drives loading-overlay state.
  *Test type:* fixture / Playwright E2E asserting overlay visibility.
- **[P1] Show/set unified pattern is implicit, not enforced.** Each show/set pair
  is tested per-command, but no architecture-fitness or table test asserts "for
  every `Show*`/`Set*` command pair, no-arg → Show, with-arg → Set". A new
  show/set command could quietly omit the bare-form. *Test type:* unit
  (table-driven).
- **[P2] No Playwright E2E for slash-command dispatch.** `apps/ui/tests/` (if any)
  is not exercising the autocomplete dropdown, the slash-prefix detection in the
  composer, or the keyboard navigation path. *Test type:* Playwright.

## 6. Recommendations

1. **Close the registry-parity gap (P0).** Add a single UI-unit test that
   asserts every backend command in `parse_system_command` is also in
   `SLASH_COMMANDS`, and vice versa. Pair with a `parish-core` architecture
   fitness sensor that surfaces the canonical list to consumers.
2. **Fill the per-category `reaction` and `Interact` holes (P0).** Two small
   unit tests in `crates/parish-input/src/lib.rs`; cheapest highest-value fix.
3. **Add a fixture-driven baseline for provider/model/key/cloud (P1).** Create
   `testing/fixtures/test_provider_settings.txt` exercising the show → set →
   show round-trip plus the per-category variants, then baseline it via
   `eval_baselines.rs`.
4. **Generalize show/set into a table-driven test (P1).** Replace the per-pair
   tests with a parameterized loop so adding a new show/set command auto-fails
   if the show form is missing — closes the "next provider category" trap.
