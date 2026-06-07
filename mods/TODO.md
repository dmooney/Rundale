# mods/ — Technical Debt

## Open

_(none — TD-001, TD-002, TD-003 resolved 2026-06-07 under #1203; see Done.)_

## In Progress

_(none)_

## Done

| ID     | Category              | Severity | Location                      | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------ | --------------------- | -------- | ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Content Layout        | P2       | `rundale/npcs.json`           | Added `parish-npc-tool` `split-catalog` / `join-catalog` / `validate-catalog` subcommands (`crates/parish-npc-tool/src/catalog.rs`). Split emits one `npc-NNNN-slug.json` per NPC; join re-emits a canonical 4-space `npcs.json` that round-trips the checked-in file **byte-for-byte** (test `split_then_join_round_trips_byte_identical`). (resolved 2026-06-07, #1203)                                                                                                                            |
| TD-002 | Content Layout        | P2       | `rundale/world.json`          | `WorldGraph::validate()` now rejects dangling/self `relative_to` anchors and ambiguous alias collisions (alias-vs-alias and alias-vs-other-name); `parish-npc/tests/rundale_data_consistency.rs` adds a cross-file well-formedness pass for festivals/encounters/transport. Real Rundale content passes. (resolved 2026-06-07, #1203)                                                                                                                                                                |
| TD-003 | Prompt Contract       | P1       | `testbed/prompts/*.txt`       | Added `parish-core/tests/testbed_prompt_placeholders.rs` rendering every testbed prompt with the documented placeholder set and failing on any leftover `{placeholder}` token — closing the gap the rundale-only parish-npc tests left open. (resolved 2026-06-07, #1203)                                                                                                                                                                                                                            |
| TD-004 | Stale Comments        | P3       | `rundale/demo-prompt.txt:3`   | `TODO #1/#30` label converted to `regression ref: TD-004 / #1201` so debt scanners no longer read the intentional prompt content as unresolved work.                                                                                                                                                                                                                                                                                                                                                 |
| TD-005 | Provider Config Drift | P2       | `*-provider/providers/*.toml` | The 20 provider mods manually duplicate the provider schema and preset category conventions. Add a checked-in provider-mod fixture test that validates directory/file/id naming, required fields, featured visibility, non-empty base URLs where required, and preset category coverage so provider catalog edits fail locally before UI/onboarding drift reaches runtime. (resolved 2026-06-06: provider-mod conventions test at parish-config/tests/provider_mod_conventions.rs covers AC-1..AC-8) |

## Progress Log

- 2026-05-25 — Initialized the mods debt ledger after scanning Rundale content, testbed prompts, provider mods, and the active mod registry.
- 2026-06-04 audit: 5 Open items reviewed, 0 migrated to Done, 0 anchors corrected. TD-003 note added: rundale prompt substitution is now tested in parish-npc/src/lib.rs; testbed prompt coverage remains missing.
- 2026-06-06 — TD-004 resolved: demo-prompt.txt movement anchor reworded to plain regression reference (cleanup-1201-p3).
- **2026-06-06**: Re-audit vs current code. Resolved->Done: TD-005. Still open: TD-001, TD-002, TD-003 (partial). Tracking epic re-opened: #1203.
- **2026-06-07** (#1203): Resolved TD-001 (npcs.json split/join/validate tooling, byte-identical round-trip), TD-002 (world.json relative_to + alias validation + cross-file well-formedness test), TD-003 (testbed prompt placeholder contract test). No Open items remain.

## Issue tracking

2026-06-04 audit: open items in this file are tracked under epic(s) #1201 (Dead-code & stale-doc cleanup), #1203 (Runtime path/config & scaling).
2026-06-06 re-audit: still-open items tracked under re-opened epic #1203 (Runtime path/config & scaling).
