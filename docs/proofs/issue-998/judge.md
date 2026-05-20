Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

The PR closes the demo auto-player's identity-leak surface by moving the
`DemoContextSnapshot` builder out of `parish-tauri::commands` and into
`parish_core::ipc::demo`, with a signature that only accepts the same
GUI-facing IPC payloads the frontend consumes
(`WorldSnapshot`, `&[NpcInfo]`, `&MapData`). The Tauri command becomes
thin wiring that calls `snapshot_from_world` / `build_npcs_here` /
`build_map_data` and forwards them. Architectural rule #12
(cross-runtime orchestration in `parish-core`) is honoured.

The leak that surfaced this issue — `(Widow)` rendered as a parenthetical
title for an un-introduced NPC — is closed two ways:

1. Pre-introduction NPCs render as `{brief_description}, feeling {mood}`.
   `Option<String>` for `occupation` enforces this at the type level
   (`Some` only when `NpcInfo.introduced` is `true`).
2. The render format drops the `Name (Title)` parens entirely in favour
   of a sentence-shaped line. Even if a future change accidentally
   re-exposed occupation, the LLM would no longer parse it as a vocative.

The fog-of-war leak in the adjacent list is closed by reusing
`build_map_data`'s already-filtered fog-of-war set: locations beyond
the frontier are absent, and `travel_minutes` is `None` for any
unvisited frontier entry.

A new `tracing::info!(user_prompt = ...)` line in `get_llm_player_action`
emits the constructed prompt to demo-mode logs so live `just demo`
runs surface what the LLM saw and any future regression is visible.

Acceptance criteria coverage:

[pre-intro NPC line redacts identity]: `rendered-prompt.txt` `NPCs here:`
block contains five NPCs at fresh-save Kilteevan, all rendered as
brief_description + mood with no occupation/real-name tokens. Verified
mechanically by `demo_prompt_at_fresh_save_does_not_leak_widow_or_peig`
against a curated leak list (`Widow`, `Peig`, `Hannigan`, `Publican`,
`Gallagher`).

[post-intro NPC line matches GUI]: covered by unit test
`introduced_npc_exposes_occupation` — the rendered prompt contains
`Padraig O'Brien, Publican, feeling warm` when `introduced = true`.

[adjacent block fog-of-war + travel_minutes hidden for unvisited]:
`rendered-prompt.txt` shows every entry as `— unvisited` at fresh save.
`demo_prompt_adjacent_block_mirrors_map_fog_of_war` asserts the demo
list is a strict subset of `build_map_data`'s adjacent set and that
`travel_minutes` is `None` for every unvisited entry.

[backend unit test "no Widow leak"]:
`rendered_prompt_does_not_contain_widow_pre_intro` passes.

[fitness check, derivable from GUI types only]: the builder signature
`build_demo_context(&WorldSnapshot, &[NpcInfo], &MapData, …)` is itself
the constraint. The Tauri command in
`parish/crates/parish-tauri/src/commands.rs:2104-2143` wires it from
the existing `handlers::*` GUI builders, so production matches the test
shape.

[live demo proof, no leaked tokens in fresh-save prompt]: captured in
`rendered-prompt.txt` from the integration test
`print_fresh_save_prompt_for_proof_bundle`, which runs the production
pipeline against the rundale mod's real world data.

Test summary: 6 demo-specific tests + 401 UI vitest tests + full
`parish-core` (371) and `parish-tauri` (103) test suites all pass.
`cargo fmt --check` clean; clippy clean on touched crates.

Resolver fix (defence in depth): the secondary half of the umbrella
issue — `addressed_to`'s name-only matching — is also closed in this
change. `NpcManager::find_by_role_at` returns the unique co-located NPC
for a given occupation (case-insensitive, refuses ambiguous matches);
`resolve_npc_targets` consults it after a name miss. So even though the
demo auto-player no longer generates role-vocatives, a human player
typing "Good mornin', Widow" now resolves correctly when the role is
unambiguous, and surfaces the existing "no one here answers" message
when two co-located NPCs share the role. Covered by 6 new unit tests
across `parish-npc::manager` and `parish-core::ipc::handlers`.
