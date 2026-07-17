# TODO.md Audit — 2026-06-04

> **Historical snapshot — superseded for execution tracking.** The findings
> below describe the repository on 2026-06-04. The debt sweep subsequently
> cleared the crate/app/mod Open ledgers. Current executable work lives in
> GitHub Issues; the portfolio reset is tracked by
> [#1684](https://github.com/dmooney/Rundale/issues/1684). Do not use the counts
> in this report as the current backlog.

Static audit of every Open item across all 20 `TODO.md` files: each claim verified against its cited `file:line`, cross-checked against `git log` for fix attempts, and cross-referenced to GitHub issues (`dmooney/rundale`). TODO files were updated in place (fixed -> Done, stale anchors corrected). This report is the synthesis.

Method: 18 parallel Sonnet audit agents (16 crate/app + bench + root), 751 tool calls.

## Verdict summary

| Verdict      | Meaning                                                            | Count   |
| ------------ | ------------------------------------------------------------------ | ------- |
| fixed        | Already done; migrated to Done table / marked RESOLVED             | 31      |
| stale-anchor | Defect persists but cited line moved; anchor corrected, still open | 40      |
| partial      | Partially addressed; annotated, still open                         | 9       |
| still-open   | Confirmed still open                                               | 76      |
| **total**    |                                                                    | **156** |

Net still-open after audit (stale-anchor + partial + still-open + unverifiable): **125**. Genuinely closed-out: **31**.

## GitHub issue coverage

- Items with ANY referenced/matched issue: **8** — all point to **closed** issues/PRs (stale back-refs, not live tracking).
- Items with **no** tracking issue whatsoever: **148 / 156**.
- Conclusion: the TODO files are the _only_ tracker for this debt. None of the open work is mirrored as an open GitHub issue.

## Per-file results

### `mods/TODO.md` — 5 items {'still-open': 4, 'partial': 1}

_All 5 Open items remain open. No item was migrated to Done. TD-003 is partial: rundale prompt substitution is now tested in parish-npc/src/lib.rs, but testbed prompt placeholders (npc_name, npc_brief_description, etc.) have no corresponding test. A note was appended to TD-003 and a dated audit line was added to the Progress Log._

| id     | verdict    | claim                                                                  | evidence                                                                                    | git                          | gh           |
| ------ | ---------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ---------------------------- | ------------ |
| TD-001 | still-open | rundale/npcs.json is a single monolithic 174 KB JSON file causing larg | mods/rundale/npcs.json: 4172 lines, 179094 bytes — still a single file at the cited locati  | 084cd32e fix: event-time loc | none/none    |
| TD-002 | still-open | rundale/world.json is a monolithic file mixing geography, aliases, rou | mods/rundale/world.json: 699 lines, 27217 bytes — still a single file at the cited locatio  | b176159d fix(world): dedupe  | none/none    |
| TD-003 | partial    | No content-level contract test that every checked-in prompt placeholde | parish/crates/parish-npc/src/lib.rs:1281,1406,1735 — three tests cover rundale prompt subs  | 5c2aa9d6 fix(npc): forbid mi | none/none    |
| TD-004 | still-open | demo-prompt.txt:3 contains 'MOVEMENT (TODO #1/#30):' label that debt s | mods/rundale/demo-prompt.txt:3 — label 'MOVEMENT (TODO #1/#30):' is present verbatim in th  | f12c7c11 fix(demo): strength | 1, 30/closed |
| TD-005 | still-open | 20 provider mods lack a fixture test validating directory/file/id nami | No test in parish/crates/ walks the mods/\*-provider/ directories or asserts naming/field/p | d7a167e5 Runtime-loaded LLM  | none/none    |

### `parish/apps/ui/TODO.md` — 29 items {'still-open': 18, 'fixed': 4, 'stale-anchor': 6, 'partial': 1}

_4 items confirmed fixed and migrated to Done (TD-032 parishPage fixture now consumed, TD-034 formatBytes/formatDuration imports removed, TD-035 emitEvent/SNAPSHOTS imports removed, TD-058 sm_probe.test.ts deleted). 5 stale line-number anchors corrected in-place (TD-033, TD-042, TD-044, TD-052/TD-053/TD-054). The remaining 24 open items are still valid against current HEAD._

| id     | verdict      | claim                                                                    | evidence                                                                                   | git                          | gh        |
| ------ | ------------ | ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-031 | still-open   | ui/README.md is still the default npx sv create scaffold, never descri   | parish/apps/ui/README.md:1-42 — first line is '# sv', second block says 'If you're seeing  | 855f2a96 refactor: relocate  | none/none |
| TD-032 | fixed        | parishPage Playwright fixture defined but no spec file consumes it       | parish/apps/ui/e2e/bug-report-hotkey.spec.ts:17,40 — uses `parishPage: page` in both tests | 65776fab test(ui): regressio | none/none |
| TD-033 | stale-anchor | mobilePanel state has unreachable 'map'/'sidebar' values; all assignme   | parish/apps/ui/src/routes/+page.svelte:26,606,622 — declaration at :26, both assignments a | a3aedaab feat(tooling): non- | none/none |
| TD-034 | fixed        | SetupOverlay.svelte imports formatBytes and formatDuration but never u   | parish/apps/ui/src/components/SetupOverlay.svelte:18-30 — only formatElapsed, formatDownlo | a26110a0 refactor: resolve 2 | none/none |
| TD-035 | fixed        | screenshots.spec.ts imports emitEvent and SNAPSHOTS that are never ref   | parish/apps/ui/e2e/screenshots.spec.ts:9-16 — imports only test/expect/installTauriMock/ap | a3aedaab feat(tooling): non- | none/none |
| TD-036 | still-open   | setup-messages.ts:7 re-exports LONG_WAIT_MESSAGES but every consumer i   | parish/apps/ui/src/lib/setup/setup-messages.ts:7 — `export { LONG_WAIT_MESSAGES }` still p | a3aedaab feat(tooling): non- | none/none |
| TD-037 | still-open   | StreamManager interface exposes 7 internal helpers not called from +pa   | parish/apps/ui/src/lib/setup/stream-manager.ts:74-82 — ensureTurnEntry/finalizeStreamingEn | 296c783d fix(ui): serialize  | none/none |
| TD-038 | still-open   | storage.ts exports SETUP_COMPLETE/ACTIVITY_SESSION_KEY constants and S   | parish/apps/ui/src/lib/setup/storage.ts:7-15 — constants and type still exported. grep acr | a26110a0 refactor: resolve 2 | none/none |
| TD-039 | still-open   | loading=true/error/try/catch/finally pattern duplicated across SaveIns   | parish/apps/ui/src/components/editor/SaveInspector.svelte:19,34,50 — three occurrences of  | c00d760c refactor: migrate 9 | none/none |
| TD-040 | still-open   | loadingCount++/try/catch/loadingCount-- pattern across 7 handlers in S   | parish/apps/ui/src/components/SavePicker.svelte — loadingCount incremented at lines 22,50, | a26110a0 refactor: resolve 2 | none/none |
| TD-041 | still-open   | screenshots.spec.ts two describe blocks duplicate the same page setup    | parish/apps/ui/e2e/screenshots.spec.ts:28-43 and :64-79 — both blocks contain identical in | a3aedaab feat(tooling): non- | none/none |
| TD-042 | stale-anchor | InputField.svelte still too large; contenteditable/history/completion    | parish/apps/ui/src/components/InputField.svelte — 1245 lines (claimed 1235); defect unchan | a3aedaab feat(tooling): non- | none/none |
| TD-043 | still-open   | LocationDetail.svelte 785 lines; map-init/drag block should be extract   | parish/apps/ui/src/components/editor/LocationDetail.svelte — 785 lines, matching the claim | a3aedaab feat(tooling): non- | none/none |
| TD-044 | stale-anchor | SetupOverlay.svelte grown to ~1003 lines; needs splitting                | parish/apps/ui/src/components/SetupOverlay.svelte — 998 lines (claimed 1003); defect uncha | a3aedaab feat(tooling): non- | none/none |
| TD-045 | still-open   | download-rate.ts, setup-messages.ts, storage.ts lack direct unit tests   | parish/apps/ui/src/lib/setup/ — directory lists only stream-manager.test.ts; no download-r | a26110a0 refactor: resolve 2 | none/none |
| TD-046 | still-open   | MentionDropdown, ModelDropdown, SlashDropdown, MapTooltip have no \*.te  | Confirmed by absence: no test files found for those 4 components in src/components/.       | none found                   | none/none |
| TD-047 | still-open   | editor/ components missing tests for SaveInspector error path, Validat   | parish/apps/ui/src/components/editor/ — only LocationDetail.test.ts present; SaveInspector | none found                   | none/none |
| TD-048 | still-open   | ipc.ts command<T>() HTTP transport has zero unit tests                   | parish/apps/ui/src/lib/ipc.test.ts — only WebSocket lifecycle tests present; no test exerc | none found                   | none/none |
| TD-049 | partial      | trimTextLog boundary (501→500, 1000→500, no-op below) has no direct un   | parish/apps/ui/src/stores/game.test.ts:154-178 — tests 600-turn bounded growth (indirect c | 134c6085 fix(ui): resolve to | none/none |
| TD-050 | still-open   | knownNouns derived store priority sort has no store-level test (nouns.   | parish/apps/ui/src/stores/ — nouns.test.ts does not exist; confirmed by ls.                | 134c6085 fix(ui): resolve to | none/none |
| TD-051 | still-open   | SETUP_HISTORY_LIMIT exported from setup-messages.ts but never imported   | parish/apps/ui/src/lib/setup/setup-messages.ts:5 — `export const SETUP_HISTORY_LIMIT = 80` | a3aedaab feat(tooling): non- | none/none |
| TD-052 | stale-anchor | +page.svelte 708 lines, mixes orchestration with markup                  | parish/apps/ui/src/routes/+page.svelte — 784 lines (claimed 708); defect unchanged, file h | a3aedaab feat(tooling): non- | none/none |
| TD-053 | stale-anchor | types.ts 538 lines manually mirrors Rust serde payloads with no genera   | parish/apps/ui/src/lib/types.ts — 603 lines (claimed 538); defect unchanged, file has grow | a3aedaab feat(tooling): non- | none/none |
| TD-054 | stale-anchor | ipc.ts 552 lines combines multiple unrelated concerns                    | parish/apps/ui/src/lib/ipc.ts — 722 lines (claimed 552); defect unchanged, file has grown  | a3aedaab feat(tooling): non- | none/none |
| TD-055 | still-open   | Historical TODO #6/#20/#31a anchors still in inline comments across 3    | parish/apps/ui/src/lib/auto-pause.ts:28,71 and src/routes/+page.svelte:223 — all three TOD | 19aeca82 fix(ui): suppress a | none/none |
| TD-056 | still-open   | MoodIcon MOOD_EMOJI fallback missing bitter→😒 and sharp→😤 entries adde | parish/apps/ui/src/components/MoodIcon.svelte:29-58 — MOOD_EMOJI array has no 'bitter', 's | b7f45293 fix(ui): MoodIcon h | none/none |
| TD-057 | still-open   | MoodIcon.test.ts omits bitter/sharp assertions so suite stays green de   | parish/apps/ui/src/components/MoodIcon.test.ts — grep for 'bitter' and 'sharp' returns zer | b7f45293 fix(ui): MoodIcon h | none/none |
| TD-058 | fixed        | sm_probe.test.ts untracked debug probe with zero expect() assertions     | parish/apps/ui/src/lib/setup/ — sm_probe.test.ts is not present in the directory listing;  | none found                   | none/none |
| TD-059 | still-open   | MoodIcon uses ?? instead of // so empty-string emoji prop renders blan   | parish/apps/ui/src/components/MoodIcon.svelte:74 — `let rendered = $derived(emoji ?? resol | b7f45293 fix(ui): MoodIcon h | none/none |

### `parish/crates/parish-client/TODO.md` — 3 items {'partial': 1, 'still-open': 2}

_All 3 Open items remain open. TD-001 is partial: render.rs gained tests in commit b76ff2b6 (#1156) but client.rs, repl.rs, and session.rs are still untested — the Open row was updated to reflect this. TD-002 and TD-003 are unchanged with no relevant commits or issues found._

| id     | verdict    | claim                                                                  | evidence                                                                                   | git                          | gh          |
| ------ | ---------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | ----------- |
| TD-001 | partial    | No crate-local tests for response rendering, command serialization, se | src/render.rs:72-102 — #[cfg(test)] block with two tests (travel_line_singular_for_one_min | b76ff2b6 fix: pluralize "min | 1156/closed |
| TD-002 | still-open | Wire response structs in parish-client manually mirror parish-server:: | src/client.rs:23-77 — defines CommandResponse, OutputLine, TravelDetail, StateBundle, Worl | f13e9ce5 feat: synchronous / | none/none   |
| TD-003 | still-open | Session cookie storage falls back to $HOME/parish/session when platfor | src/session.rs:4-8 — session_path() calls dirs::state_dir().or_else(dirs::data_local_dir). | f13e9ce5 feat: synchronous / | none/none   |

### `parish/crates/parish-config/TODO.md` — 3 items {'still-open': 1, 'stale-anchor': 2}

_All 3 Open items (TD-015, TD-016, TD-017) remain genuinely open. No items were migrated to Done. Two stale line anchors were corrected: TD-016 engine.rs grew from 1531 to 1541 lines, and TD-017 CwdGuard shifted slightly to lines 995-1007. No tracking GitHub issues exist for any of the three items._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-015 | still-open   | provider.rs is ~2473 lines combining schema, registry, alias normaliza | parish/crates/parish-config/src/provider.rs: wc -l reports exactly 2473 lines; all describ | d7a167e5 Runtime-loaded LLM  | none/none |
| TD-016 | stale-anchor | engine.rs is ~1531 lines keeping all config structs, defaults, resolut | parish/crates/parish-config/src/engine.rs: wc -l reports 1541 lines (grew by 10 from cited | b8629534 fix(npc): raise rec | none/none |
| TD-017 | stale-anchor | Provider tests include CwdGuard helper that mutates current_dir() — br | parish/crates/parish-config/src/provider.rs:995-1007: CwdGuard struct with current_dir() s | none found                   | none/none |

### `parish/crates/parish-core/TODO.md` — 2 items {'stale-anchor': 2}

_Both Open items (TD-030, TD-031) are still open. No code change has addressed either defect. Line anchors in both rows were stale and have been corrected: TD-030 headless.rs:1630→:1556 and setup.rs:1250→:1143; TD-031 location_log.rs:274-300→:280-305. A dated audit line was appended to the Discovery note section._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh          |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | ----------- |
| TD-030 | stale-anchor | Tier-2 gossip minting block (apply_tier2_event_with_config + create_go | parish-engine/src/headless.rs:1556-1572 and parish-tauri/src/setup.rs:1143-1161 both conta | eb15d10c feat(npc): publish  | 1113/closed |
| TD-031 | stale-anchor | location_log.rs:274-300 — WeatherChanged and FestivalStarted loop over | location_log.rs:280-287 (WeatherChanged loop) and :296-305 (FestivalStarted loop) still pe | 18f4ed01 fix(logging): Weath | none/none   |

### `parish/crates/parish-engine/TODO.md` — 2 items {'stale-anchor': 2}

_Both Open items (TD-036, TD-037) are still open and confirmed by direct code inspection. No items were migrated to Done. Two stale line-number anchors were corrected in the TODO.md Open rows, and a dated audit note was appended to the Discovery note section._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh          |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | ----------- |
| TD-036 | stale-anchor | run_script_mode builds via build_with_mod() which hardcodes enable_cha | testing.rs:1571 — run_script_mode calls build_with_mod(game_mod) which calls Self::build(f | 248ed218 chore: clear 17 ope | 1123/closed |
| TD-037 | stale-anchor | headless.rs idle-message selection uses pre-increment (idle_counter+=1 | headless.rs:976-977 — app.idle_counter += 1; let idx = app.idle_counter; confirmed present | b7bfd497 refactor: unify per | none/none   |

### `parish/crates/parish-geo-tool/TODO.md` — 3 items {'still-open': 3}

_All 3 open items (TD-021, TD-022, TD-023) remain fully open. extract.rs is still 954 lines, realign_rundale_coords.rs is still 863 lines, and the best.unwrap() pattern in connections.rs:165 is unchanged. No GH issues or commits address any of these items. TODO.md was not modified._

| id     | verdict    | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ---------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-021 | still-open | extract.rs (954 lines) mixes Overpass filtering, tag classification, r | parish/crates/parish-geo-tool/src/extract.rs: still 954 lines (wc -l confirms), anchor 1-9 | ec72ab65 fix(geo-tool): reso | none/none |
| TD-022 | still-open | realign_rundale_coords.rs (863 lines) bundles CLI parsing, graph resol | parish/crates/parish-geo-tool/src/bin/realign_rundale_coords.rs: still 863 lines (wc -l co | ec72ab65 fix(geo-tool): reso | none/none |
| TD-023 | still-open | connect_nearby uses best.unwrap() inside a conditional guarded by best | parish/crates/parish-geo-tool/src/connections.rs:165 — grep confirms `if best.is_none() // | ec72ab65 fix(geo-tool): reso | none/none |

### `parish/crates/parish-inference/TODO.md` — 5 items {'still-open': 1, 'stale-anchor': 4}

_All 5 Open items (TD-031 through TD-035) are still open. No items were migrated to Done. Four items had stale file:line anchors corrected: TD-032 LOC grew from 2157 to 2390; TD-033 inference_client.rs was deleted and allows shifted/multiplied across lib.rs and openai_client.rs; TD-034 both provider client files grew well beyond their cited ranges; TD-035 second regression comment moved from line 675 to 719._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-031 | still-open   | setup.rs is the largest file in the crate, mixing process management,  | setup.rs:1-3150 — file is exactly 3150 lines with 65 test annotations; cited range matches | d7a167e5 Runtime-loaded LLM  | none/none |
| TD-032 | stale-anchor | lib.rs mixes exports, queue types, logs, helpers, client aggregation,  | lib.rs:1-2390 — file grew from cited 2157 to 2390 lines; debt still holds, LOC anchor was  | 780f0f19 fix(inference): del | none/none |
| TD-033 | stale-anchor | Multiple constructors/builders need #[allow(clippy::too_many_arguments | inference_client.rs no longer exists (deleted per git log 780f0f19); allows shifted to lib | 780f0f19 fix(inference): del | none/none |
| TD-034 | stale-anchor | Provider clients mix wire types, builders, retry/stream logic, SSE par | anthropic_client.rs is 1437 lines (cited range 54-543 was stale); openai_client.rs is 1193 | 780f0f19 fix(inference): del | none/none |
| TD-035 | stale-anchor | Historical `.expect()` regression comments in openai_client.rs read li | openai_client.rs:22 still has the Historically comment (valid); second comment moved from  | 6736eb6a feat(inference): pl | none/none |

### `parish/crates/parish-input/TODO.md` — 2 items {'still-open': 1, 'stale-anchor': 1}

_Both Open items remain open. TD-017 (parser.rs complexity) is still-open: the file has grown to 1216 lines with no family split. TD-018 (stale TODO #41/#46/#53 comments) is stale-anchor: the comments still exist but the secondary line anchor shifted from 491-555 to 47 and 506-583 after first-person movement fixes in PRs #1094 and #1141; anchor corrected in the file._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-017 | still-open   | parser.rs is a broad 1208-line file mixing command dispatch, per-comma | parish/crates/parish-input/src/parser.rs: file is 1216 lines (grown slightly). mod tests s | 6c80a8da feat: add /exit sla | none/none |
| TD-018 | stale-anchor | Inline TODO #41/#46/#53 comments in intent_local.rs look like open deb | parish/crates/parish-input/src/intent_local.rs: TODO #41/#46/#53 strings still present at  | 01abc444 fix(input): parse m | none/none |

### `parish/crates/parish-mcp/TODO.md` — 3 items {'still-open': 2, 'stale-anchor': 1}

_All three Open items remain open. TD-001 (missing IPC wiring parity test) and TD-003 (monolithic jsonrpc.rs module) are confirmed still-open with no relevant fixes in git history. TD-002 (GenericTauriBackend stub) is still-open but had a stale line anchor (196-215 -> 199-212), which was corrected in the Open table. No items were migrated to Done._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-001 | still-open   | No parity test against canonical server/Tauri IPC route registry; new  | parish/crates/parish-mcp/src/tools.rs:425-447 — `registry_exposes_full_contract_names_in_o | 6b7804b4 feat: in-app bug re | none/none |
| TD-002 | stale-anchor | GenericTauriBackend is an exported unimplemented placeholder; should s | parish/crates/parish-mcp/src/backend.rs:199-212 — struct and impl still present, still ret | 3e4cdda3 test: expand Rust c | none/none |
| TD-003 | still-open   | JSON-RPC framing, request/response structs, protocol errors, async std | parish/crates/parish-mcp/src/jsonrpc.rs:1-386 — all of framing (serve, dispatch_line, writ | 3e4cdda3 test: expand Rust c | none/none |

### `parish/crates/parish-npc-tool/TODO.md` — 13 items {'fixed': 3, 'still-open': 9, 'partial': 1}

_TD-016, TD-017, and TD-018 are fixed — all three were addressed in commit 3e4cdda3 (test: expand Rust coverage #969), which added targeted tests for every previously-uncovered validate_db rule, the --parish/--all mutual-exclusion path, and the promote_npc not-found branch. The remaining 10 items (TD-019 through TD-028) remain open with anchors verified against current source; TD-020 received a partial-coverage note and TD-022/TD-028 had minor anchor corrections._

| id     | verdict    | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ---------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-016 | fixed      | validate*db missing tests for missing_households, invalid_age, broken* | src/main.rs:1000-1044 — tests test_validate_detects_missing_household, test_validate_detec | 3e4cdda3 test: expand Rust c | none/none |
| TD-017 | fixed      | --parish + --all mutual-exclusion error path untested                  | src/main.rs:1047-1059 — test_validate_rejects_parish_and_all_together calls validate_db(co | 3e4cdda3 test: expand Rust c | none/none |
| TD-018 | fixed      | promote_npc not-found path untested                                    | src/main.rs:973-985 — test_promote_rejects_missing_target calls promote_npc(&conn, 9_999_9 | 3e4cdda3 test: expand Rust c | none/none |
| TD-019 | still-open | weighted_occupation has no direct unit test                            | src/main.rs:402-412 — function present, grep of test section finds no test calling weighte | none found                   | none/none |
| TD-020 | partial    | escape_like has no direct unit test                                    | src/main.rs:509-513 — no isolated string-only test; test_search_wildcard_chars_match_liter | 3e4cdda3 test: expand Rust c | none/none |
| TD-021 | still-open | DataTier::as_i64 and from_i64 have no direct tests                     | src/main.rs:111-128 — functions present; grep of test section finds no test calling as_i64 | none found                   | none/none |
| TD-022 | still-open | literal 1820 duplicated in generate*parish (line 335) and import_npcs* | src/main.rs:335 has `let now_year = 1820_i64;` and src/main.rs:789 has `1820 - npc.age`; n | none found                   | none/none |
| TD-023 | still-open | generate_parish silently auto-creates roscommon county with .expect()  | src/main.rs:305-314 — .expect("inserting default county should succeed") and hard-coded 'r | none found                   | none/none |
| TD-024 | still-open | DEFAULT_DB is a relative path                                          | src/main.rs:11 — `const DEFAULT_DB: &str = "data/parish-world.db";` still a relative path  | none found                   | none/none |
| TD-025 | still-open | import_npcs JSON-error path untested                                   | src/main.rs:809-818 — no test in the test module calls import*npcs or exercises the serde* | none found                   | none/none |
| TD-026 | still-open | comment at line 319 references import_npcs instead of import_npcs_inne | src/main.rs:319 — comment reads "Matches the pattern used by `import_npcs`." which should  | none found                   | none/none |
| TD-027 | still-open | validate_db complexity: inner with_filter fn + count! macro + duplicat | src/main.rs:620-641 — with_filter inner fn at line 620 and count! macro at line 632 both s | none found                   | none/none |
| TD-028 | still-open | single-file CLI at 1680 lines needs splitting into modules             | src/main.rs:1-1681 — file is 1681 lines, still a single file with no sub-modules declared  | none found                   | none/none |

### `parish/crates/parish-npc/TODO.md` — 11 items {'stale-anchor': 7, 'still-open': 4}

_All 11 open items are still-open. No items qualify for migration to Done. Seven items had stale line anchors corrected in the TODO file (TD-002, TD-026, TD-027, TD-029, TD-031, TD-032, TD-033); the remaining four (TD-010, TD-011, TD-028, TD-030) had accurate-enough anchors or descriptions and were left unchanged. No GitHub tracking issues exist for any open TD._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-002 | stale-anchor | Local NPC fixture wrappers duplicated across ticks.rs, emoji_reactions | ticks.rs:1634 has name-aware wrapper (delegates to test_helpers::make_test_npc); arrival_r | 9ac5ecb1 refactor(parish-npc | none/none |
| TD-010 | still-open   | arrival_reactions.rs is a large multi-concern module (1240 lines) need | arrival_reactions.rs:1-1248 — 1248 lines, multi-concern structure confirmed by reading: te | 6736eb6a feat(inference): pl | none/none |
| TD-011 | still-open   | build_tier1_system_prompt() contains a long inline format!() prompt te | lib.rs:568-673 — function still has ~100-line inline format!() template with nested placeh | 780f0f19 fix(inference): del | none/none |
| TD-026 | stale-anchor | ticks.rs is the largest file in the crate, combining multiple tiers an | ticks.rs is now 3218 lines — grown since the anchor was written; complexity claim still fu | 780f0f19 fix(inference): del | none/none |
| TD-027 | stale-anchor | lib.rs crate root mixes too many concerns (cited 1711 lines)           | lib.rs is now 2099 lines — grown since anchor was written; the multi-concern claim still f | 780f0f19 fix(inference): del | none/none |
| TD-028 | still-open   | Inline comments use historical TODO #NN anchors not mapped to this cra | ticks.rs:252 TODO #27, ticks.rs:273 TODO #29, ticks.rs:299 TODO #54, ticks.rs:338 TODO #21 | none found                   | none/none |
| TD-029 | stale-anchor | build_enhanced_context_with_config() takes nine parameters, needs #[al | ticks.rs:518-529 — function at line 518 (not 473), #[allow(clippy::too_many_arguments)] at | none found                   | none/none |
| TD-030 | still-open   | NpcManager main impl mixes too many responsibilities in manager.rs     | manager.rs is 1531 lines; impl NpcManager at line 93 confirmed mixing collection storage,  | ed1c5146 feat(npc): publish  | none/none |
| TD-031 | stale-anchor | create_gossip_from_tier2_event empty-participants bail is untested in  | ticks.rs:1176 (cited 1162) is the current location of create_gossip_from_tier2_event; goss | eb15d10c feat(npc): publish  | none/none |
| TD-032 | stale-anchor | record_tier2_parse_failure() fires twice for a single dropped location | ticks.rs:906 (first attempt), 923 (retry fire on parse fail), 946 (retry-failure branch fi | e07042b6 feat(metrics): expo | none/none |
| TD-033 | stale-anchor | try_tier2_inference and its retry pass None for response_format instea | ticks.rs:222-231 — generate_stream_with_format called with None (5th arg) for both initial | f3f13d1f fix(npc): retry Tie | none/none |

### `parish/crates/parish-persistence/TODO.md` — 3 items {'still-open': 2, 'stale-anchor': 1}

_All 3 Open items (TD-024, TD-025, TD-026) remain open. No debt has been resolved since the 2026-05-25 refresh. TD-026's line-number anchors were stale and have been corrected to match current file state; snapshot.rs has grown from 1218 to 1258 lines since the debt was recorded._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-024 | still-open   | database.rs is a 1430-line hotspot mixing schema/migrations, sync ops, | parish/crates/parish-persistence/src/database.rs:1-1431 — file is 1431 lines, single-file, | 1806bb1b refactor(parish-per | none/none |
| TD-025 | still-open   | snapshot.rs (1218 lines) mixes snapshot structs, conversion helpers, r | parish/crates/parish-persistence/src/snapshot.rs:1-1258 — file is now 1258 lines (grown be | 72aca2ea fix(logging): playe | none/none |
| TD-026 | stale-anchor | Large inline test modules in lock.rs, database.rs, and snapshot.rs mak | lock.rs mod tests starts at line 325 (cited 337); database.rs at 585 (cited 596); snapshot | 345ae08e refactor(parish-per | none/none |

### `parish/crates/parish-server/TODO.md` — 8 items {'stale-anchor': 5, 'still-open': 3}

_All 8 Open items remain open. No items migrated to Done — no clear evidence that any described defect has been fully resolved. 4 anchors were corrected: TD-033 LOC grew to 3300; TD-035 routes.rs helper moved to line 1702; TD-037 state.rs function moved to line 285; TD-039 session.rs grew to 1822 lines; TD-040 anchor moved from autosave tick to world tick at line 1125. A Discovery note was appended._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh         |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | ---------- |
| TD-033 | stale-anchor | routes.rs is the largest file — 3044 lines mixing all route families   | src/routes.rs: wc -l reports 3300 lines; defect (monolithic file mixing all route families | 780f0f19 fix(inference): del | none/none  |
| TD-034 | still-open   | run_server() builds all route registrations, middleware layering inlin | src/lib.rs:447-675: Router::new() chain with 40+ routes, Tower-session setup, legal routes | 6b7804b4 feat: in-app bug re | none/none  |
| TD-035 | stale-anchor | mods_root() and mods_root_path() are duplicate helpers in editor_route | src/editor_routes.rs:69-86 has `fn mods_root(state: &AppState)` and src/routes.rs:1702-171 | 780f0f19 fix(inference): del | none/none  |
| TD-036 | still-open   | CSP still requires script-src 'unsafe-inline' for SvelteKit bootstrap  | src/lib.rs:114: `script-src 'self' 'unsafe-inline'` still present. tests/security_headers. | ffe999f6 fix: extract shared | 543/closed |
| TD-037 | stale-anchor | build*app_state() takes 17 parameters, needs #[allow(clippy::too_many* | src/state.rs:285-304: `#[allow(clippy::too_many_arguments)]` at line 285, `pub fn build_ap | 780f0f19 fix(inference): del | none/none  |
| TD-038 | still-open   | Startup helpers parent-walk from current_dir() to find mods/, UI dist, | src/main.rs:87-120: `find_data_dir()` and `find_ui_dist_dir()` both call `std::env::curren | f13e9ce5 feat: synchronous / | none/none  |
| TD-039 | stale-anchor | session.rs mixes registry, lifecycle, inference queue, tick scheduling | src/session.rs: wc -l reports 1822 lines (up from cited 1448); top-level functions span Se | 21a59a8d refactor: extract t | none/none  |
| TD-040 | stale-anchor | Server tick never calls create_gossip_from_tier2_event; GossipSpread n | src/session.rs:1252-1260 is now the autosave tick (not the world tick). World tick is at 1 | 21a59a8d refactor: extract t | none/none  |

### `parish/crates/parish-tauri/TODO.md` — 10 items {'stale-anchor': 8, 'partial': 1, 'still-open': 1}

_All 10 Open items remain open — no item had sufficient evidence to migrate to Done. Eight stale line anchors were corrected in-place (TD-006, TD-007, TD-008, TD-009, TD-010 partial claim correction, TD-011, TD-012, TD-014, TD-015). One factual error was fixed: TD-010 incorrectly claimed EVENT_STREAM_TURN_END was unused, but it is imported and used at commands.rs:1167. A Discovery note section was appended to the file._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh         |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | ---------- |
| TD-006 | stale-anchor | Stale doc comments in setup.rs citing lib.rs line numbers for the inli | setup.rs:12 still cites 'lib.rs:898-1900'; setup.rs:989 and setup.rs:1126 still cite 'main | 21a59a8d refactor: extract t | none/none  |
| TD-007 | stale-anchor | find_default_mod() called per-handler in editor_commands.rs and comman | editor_commands.rs:24-36 (mods_root), :115-129 (is_active_default_mod), :131-153 (reload_l | caa6efda fix(tauri): resolve | none/none  |
| TD-008 | stale-anchor | do_list_branches_text and do_branch_log_text in commands.rs duplicate  | Both functions confirmed still present at commands.rs:2162 and :2193; routes.rs:1046 and : | none found                   | none/none  |
| TD-009 | stale-anchor | create_branch/do_create_branch in commands.rs accepts unsanitized bran | commands.rs:1525 (create*branch) and :1534 (do_create_branch) confirm no call to validate* | none found on commands.rs fo | 335/closed |
| TD-010 | partial      | stream*npc_response has zero callers; EVENT_STREAM_TURN_END and EVENT* | events.rs:187 stream_npc_response confirmed zero callers workspace-wide. EVENT_STREAM_TURN | 1356bcc8 fix(ws): turn-in-fl | none/none  |
| TD-011 | stale-anchor | spawn_world_tick is 471 lines with 6+ nesting levels, far above the 10 | setup.rs: spawn_world_tick runs from line 765 to 1202 = 438 lines (was cited at :333 / 471 | 21a59a8d refactor: extract t | 283/closed |
| TD-012 | stale-anchor | Three command handlers exceed the 100-line threshold: handle_movement  | handle_movement at commands.rs:1012–1214 = 203 lines; handle_game_input at :879–1011 = 133 | 780f0f19 fix(inference): del | none/none  |
| TD-013 | still-open   | tests/command_logic.rs:11 table header still reads 'Commands covered ( | command_logic.rs:10-11 still reads 'Commands covered (3 of 32)' / 'Commands deferred (29 o | none found                   | none/none  |
| TD-014 | stale-anchor | commands.rs is 3429 lines mixing multiple command families             | commands.rs is now 3849 lines — larger than the cited 3429. The debt (mixed concerns, no s | 780f0f19 fix(inference): del | none/none  |
| TD-015 | stale-anchor | Inline TODO #NN anchors at commands.rs:2148, 2425-2437, 2561-2716 look | Lines 2148, 2425-2437, and 2561-2716 do not contain TODO #NN comments. Actual TODO #NN loc | 206854f1 fix(demo): forbid n | none/none  |

### `parish/crates/parish-world/TODO.md` — 3 items {'still-open': 2, 'stale-anchor': 1}

_All 3 Open items (TD-026, TD-027, TD-028) remain open. No items were fixed. TD-027 had a stale line-range anchor (891 vs actual 899) which was corrected in the TODO. A Discovery note section was appended to the file._

| id     | verdict      | claim                                                                  | evidence                                                                                   | git                          | gh        |
| ------ | ------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------- | --------- |
| TD-026 | still-open   | src/graph.rs:1-1347 is a hotspot mixing graph schema, loading/validati | parish/crates/parish-world/src/graph.rs: file is exactly 1347 lines, confirmed by wc -l. S | 3e4cdda3 test: expand Rust c | none/none |
| TD-027 | stale-anchor | src/movement.rs:1-891 combines movement parsing, target resolution, tr | parish/crates/parish-world/src/movement.rs: file is 899 lines (not 891 as cited). Debt sti | b76ff2b6 fix: pluralize "min | none/none |
| TD-028 | still-open   | src/description.rs:83 references TODO #28 as a regression anchor for d | parish/crates/parish-world/src/description.rs:83: `(TODO #28)` comment still present verba | 803e7e63 fix(rundale): allow | none/none |

### `rundale-bench/TODO.md` — 23 items {'still-open': 20, 'fixed': 2, 'partial': 1}

_2 of the 14 unchecked checkbox items are verified done (Round-4 drain landed in commit c3bcd609; bench-bug=0 axes invariant enforced in judge_bundle.py:209-223) and have been ticked. 1 item (MLX_VENV README doc) is partial and annotated. The remaining 11 checkbox items and all 6 tech-debt ledger items remain open with no artifact evidence of completion._

| id                                                      | verdict    | claim                                                                  | evidence                                                                                    | git                          | gh        |
| ------------------------------------------------------- | ---------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ---------------------------- | --------- |
| Pydantic on Bundle / Result / Summary shapes            | still-open | Promote validate_result to Pydantic BundleSchema/ResultSchema/SummaryS | judge_bundle.py:158-273 — validate_item exists and checks axes/overall but uses plain dict  | none found                   | none/none |
| Round-4 drain                                           | fixed      | Dispatch wave-N subagents for all 5 round-4 models, ingest --finalize, | commit c3bcd609 title: 'bench: rounds 3-5 local MLX sweeps + Sonnet-subagent judge lock (#  | c3bcd609 bench: rounds 3-5 l | none/none |
| bench-bug = 0 axes invariant                            | fixed      | Enforce in judge_bundle.py validate_item that bench_bug=true items hav | judge_bundle.py:209-223 — explicit checks: 'if bench_bug and any(v != 0 for v in axes_out.  | c3bcd609 bench: rounds 3-5 l | none/none |
| Outlines / lm-format-enforcer integration for MLX serve | still-open | Constrain MLX generation to per-slice JSON grammar; add grammar files  | rundale-bench/grammars/ directory does not exist; no lm-format-enforcer or outlines refere  | none found                   | none/none |
| Subagent post-validate wrapper                          | still-open | Orchestrator checks expected output file after Agent return; regex-ext | rundale_bench.py — no regex-extract-from-reply-text or post-Agent file-existence check pat  | none found                   | none/none |
| code-switch slice                                       | still-open | New eval slice measuring bidirectional Irish/English register-switchin | No code-switch slice file in rundale-bench/v1/; MANIFEST.json lists only dialogue, gaeilge  | none found                   | none/none |
| Gaeilge slice expansion                                 | still-open | Expand gaeilge slice from 10 prompts; add multi-turn, Connacht vs Muns | v1/gaeilge.jsonl: 11 lines; v1/gaeilge.holdout.jsonl: 1 line; MANIFEST.json shows records=  | none found                   | none/none |
| MLX_VENV env-var documented in README                   | partial    | Document MLX_VENV env-var in README setup section; auto-detect common  | local_runner.py:61-62 — 'Override with `MLX_VENV=/abs/path` if the venv lives elsewhere.'   | c3bcd609 bench: rounds 3-5 l | none/none |
| Runtime RAM-cap kill switch                             | still-open | Add --max-ram-gb flag to local_runner.py; SIGKILL mlx_lm.server if pea | local_runner.py:348-379 argparse block — no --max-ram-gb argument; RamSampler at line 434   | none found                   | none/none |
| Bundled-slice metric surfaces pending_judge warning     | still-open | Show pending_judge warning in leaderboard.md row instead of silently s | build_leaderboard_page.py:99-102 — pending_judge rows are skipped entirely (continue), not  | none found                   | none/none |
| Tokenizer audit script                                  | still-open | tokenizer_audit.py script measuring tokens/char across candidate base  | ls rundale-bench/ — no tokenizer_audit.py file present                                      | none found                   | none/none |
| Per-slice cost ledger                                   | still-open | Surface judge_compute_minutes or similar; cloud rows show cost.usd but | rundale_bench.py and build_site_data.py — no judge_compute_minutes field found; cost track  | none found                   | none/none |
| HF preflight script                                     | still-open | preflight.py script: given mlx-community repo URL, check architecture, | ls rundale-bench/ — no preflight.py file present                                            | none found                   | none/none |
| Disk-cleanup discipline as TOML metadata                | still-open | Add delete_after_bench per candidate in candidates_local_mlx.toml; hon | grep candidates_local_mlx.toml for delete_after_bench — no output; local_runner.py has no   | none found                   | none/none |
| Round 5 / round 6 candidate pre-registration            | still-open | Pre-register next sweep candidates: Qwen3-VL-disabled, ExaONE-Deep, Ge | candidates_local_mlx.toml exists but no explicit round-5/6 pre-registration section found;  | none found                   | none/none |
| Bench-site model-detail page bench-bug rate column      | still-open | Add bench-bug rate column to model detail page so DS-R1 0.00 overall c | bench-site/src/components/Leaderboard.svelte:62 — shows a badge '🐛 N bugs' inline on the r | none found                   | none/none |
| Reproducibility manifest                                | still-open | Per-sweep capture of harness_sha, mlx_lm version, vllm-mlx version, ML | local_runner.py has harness_sha() call (line 403) written into run JSON but no separate re  | none found                   | none/none |
| TD-001                                                  | still-open | Split rundale_bench.py (1163 lines) into per-slice runners, CLI comman | rundale-bench/rundale_bench.py exists as a single file; no sub-module split observed        | none found                   | none/none |
| TD-002                                                  | still-open | Split build_site_data.py (753 lines) into source discovery, catalog en | rundale-bench/build_site_data.py exists as a single file; no sub-module split               | none found                   | none/none |
| TD-003                                                  | still-open | Add build_site_data.py --check / test fixture that fails when committe | build_site_data.py has no --check flag; no test fixture for bench.json staleness found in   | none found                   | none/none |
| TD-004                                                  | still-open | Add schema/consistency test for candidates_local_mlx.toml (unique IDs, | No schema test file targeting candidates_local_mlx.toml found in rundale-bench/tests/       | none found                   | none/none |
| TD-005                                                  | still-open | Extract pure planning/readiness/result-shaping helpers from local_runn | local_runner.py is a single file; no unit tests importing it found in rundale-bench/tests/  | none found                   | none/none |
| TD-006                                                  | still-open | Update docs claiming v1-dev has 155 prompts; MANIFEST.json now records | README.md:11 still says '155 prompts total'; MANIFEST.json sums to 309 records (159+21+11+  | none found                   | none/none |

### `TODO.md` — 28 items {'fixed': 22, 'partial': 3, 'still-open': 3}

_37 of 56 findings confirmed fixed by concrete code evidence. The top-10 priority clusters are largely resolved: movement parser, repetition penalty, name hallucination/location leak, streaming serialization, auto-pause spam, mood-emoji map, Tier 2/3 surfacing, and time-of-day label all have matching commits. Remaining open items (NPC reply rate, travel time accounting, headless server auto-load, map filter design, NPC quality variance, player role-flip serialization) have no matching commit and no GH tracking issue._

| id                                       | verdict    | claim                                                                  | evidence                                                                                     | git                          | gh          |
| ---------------------------------------- | ---------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- | ---------------------------- | ----------- |
| cluster-movement (#1/#30/#41/#46/#53)    | fixed      | Auto-player never moves / movement parser silently rejects valid phras | parish/crates/parish-input/src/intent_local.rs:25-98 — `move_phrases` list includes `i'll    | 0b247306 fix(input): parse f | none/none   |
| cluster-repetition (#10/#23/#34)         | fixed      | NPC dialogue degenerates into repetition loops (anaphora chains, trail | parish/crates/parish-core/src/game_loop/npc_turn.rs:140-160 — `queue.send_with_penalty(...   | 6736eb6a feat(inference): pl | none/none   |
| cluster-name-hallucination (#11/#24/#35) | fixed      | NPC hallucinates absent characters; player addressed by previous NPC n | parish/crates/parish-npc/src/ticks.rs:343-351 — `location_anchor_block()` injects `WHERE Y   | d89ae98a refactor(parish-cor | 1027/closed |
| cluster-streaming (#45)                  | fixed      | Two NPC stream reveals pump simultaneously — no cross-turn serializati | parish/apps/ui/src/lib/setup/stream-manager.ts — commit 296c783d explicitly titled 'serial   | 296c783d fix(ui): serialize  | none/none   |
| cluster-stranding (#12)                  | fixed      | Player stranded with empty location for multiple turns, no system hint | e1d31c14 commit message 'direct auto-player to move when NPCs here: none (TODO #12)'. demo   | e1d31c14 fix(demo): direct a | none/none   |
| cluster-time-cue (#5/#13/#28)            | fixed      | Time label hides clock progression from LLM; NPC greets with wrong tim | parish/crates/parish-tauri/src/commands.rs:2401-2410 — `game_time` now formatted as `Wedne   | 2a1f133e fix(demo): unblock  | none/none   |
| cluster-tier2-tier3 (#27/#29/#54)        | fixed      | Tier 2 JSON parse failure aborts off-screen NPC silently; Tier 3 cance | f3f13d1f 'retry Tier 2 inference once on JSON parse failure'; e07042b6 'expose tier2_parse   | f3f13d1f fix(npc): retry Tie | none/none   |
| cluster-mood-emoji (#3/#20)              | fixed      | Mood→emoji map miscategorises bitter/sharp; map inconsistent across cy | parish/crates/parish-npc/src/mood.rs:67-73 — `bitter` maps to 😒, `sharp` maps to 😤 (in irr | 5cafc389 fix(npc): map bitte | none/none   |
| cluster-auto-pause (#6/#19/#31/#31a)     | fixed      | Frontend auto-pause spam from user computer activity; duplicate system | parish/apps/ui/src/lib/auto-pause.ts:71-84 — `isWindowFocused` guard added; when `!focused   | 19aeca82 fix(ui): suppress a | none/none   |
| finding-2                                | fixed      | MCP port not opened by `just demo`                                     | parish/justfile:142 — `DEMO_ARGS` includes `--mcp-port $MCP_PORT` where `MCP_PORT=${PARISH   | 5d7a935c fix(demo): unblock  | none/none   |
| finding-7                                | fixed      | NPC reply truncated mid-sentence in recent-events buffer with no ellip | Commit b8629534 explicitly titled 'raise recent-events memory cap + switch suffix to ellip   | b8629534 fix(npc): raise rec | none/none   |
| finding-8                                | fixed      | parish/.demo-run.log shows up in git status (not gitignored)           | .gitignore:75 — `parish/.demo-run.log` present in .gitignore.                                | 5d7a935c fix(demo): unblock  | none/none   |
| finding-15                               | fixed      | Weather field duplicated in prompt (in location_description and as sta | parish/crates/parish-core/src/ipc/demo.rs:178-186 — standalone `Weather:` line removed; co   | none found                   | none/none   |
| finding-18                               | fixed      | Auto-player emits empty action and burns a turn (no retry, no WARN)    | parish/crates/parish-tauri/src/commands.rs:2848-2863 — bounded single retry at temperature   | none found                   | none/none   |
| finding-21                               | fixed      | NPCs mis-identify their location (say Curraghboy when at Kilteevan)    | parish/crates/parish-npc/src/ticks.rs:340-352 — `location_anchor_block()` function comment   | d89ae98a refactor(parish-cor | none/none   |
| finding-22                               | fixed      | Gaelic validator over-flags real Irish word poitín                     | parish/crates/parish-npc/src/quality.rs:291 — `poitín` in allow-list. Test at line 637-644   | 803e7e63 fix(rundale): allow | none/none   |
| finding-39                               | fixed      | NPC mid-reply self-introduction redundancy when already introduced     | Commit 3773669a titled 'introduced-anchor stops mid-reply self-introduction'. ticks.rs:541   | 3773669a fix(npc): introduce | none/none   |
| finding-47                               | fixed      | Roleplay narration style (third-person/past-tense) treated as dialogue | Commit 206854f1 titled 'forbid narrative action style in auto-player prompt (TODO #47)'. d   | 206854f1 fix(demo): forbid n | none/none   |
| finding-53                               | fixed      | Modal phrasings ('Might I venture to X', 'I shall go to X') not parsed | parish/crates/parish-input/src/intent_local.rs:47-61 — modal first-person phrases (`might    | 01abc444 fix(input): parse m | none/none   |
| finding-55                               | fixed      | Modern-register anachronism validator flags NPC for echoing player's o | parish/crates/parish-core/src/ipc/handlers.rs:862-866 — `format_player_register_alert(play   | 0a8e15b2 fix(npc): alert NPC | none/none   |
| finding-4                                | fixed      | NPCs sign off mid-conversation with Slán abhaile                       | mods/rundale/prompts/tier1_system.txt:28 — `NEVER FAREWELL MID-CONVERSATION` directive add   | 03074a0a fix(npc): constrain | none/none   |
| finding-9-17                             | partial    | Standalone parish-server returns all-null save-state on boot           | Finding #17 self-revokes for the MCP-bridge path. No commit found that auto-loads most-rec   | none found                   | none/none   |
| finding-14                               | fixed      | NPC mixes greetings + multiple goodbyes + ongoing chat in one reply (m | parish/crates/parish-core/src/ipc/handlers.rs:874-879 — `Respond to the live exchange abov   | 03074a0a fix(npc): constrain | none/none   |
| finding-32                               | still-open | Movement time accounting appears off (15-min travel vs observed 5-hour | No commit found addressing travel time accounting discrepancy. The game loop and world adv   | 21a59a8d refactor: extract t | none/none   |
| finding-40-56                            | still-open | NPC reply rate at single-NPC locations remains low (10-40% of turns)   | No commit found addressing the NPC reply-decision criteria (when npc_turn fires vs skips).   | none found                   | none/none   |
| finding-48                               | still-open | NPC reply quality varies dramatically across personas (Padraig good, D | frequency_penalty fix (6736eb6a) applies uniformly to all Tier 1 dialogue, which should pa   | 6736eb6a feat(inference): pl | none/none   |
| finding-51                               | partial    | LLM-as-player roleplays as NPC when awaiting NPC response              | demo-prompt.txt:13 adds 'CRITICAL: Always respond with first-person speech...NEVER use com   | 259bfad6 fix(demo): reject c | none/none   |
| finding-33-36                            | partial    | Map endpoint hides reachable-via-transit locations; adjacent list inco | parish/crates/parish-core/src/ipc/demo.rs:111-124 — adjacent list is filtered to `loc.adja   | none found                   | none/none   |
