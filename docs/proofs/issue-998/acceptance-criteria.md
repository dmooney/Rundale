# Acceptance Criteria: issue-998

## Task

The demo auto-player must only see information the GUI would show the same
player at that moment. The current `DemoContextSnapshot` (built in
`parish/crates/parish-tauri/src/commands.rs:2120`) bypasses GUI visibility
gates and leaks two classes of information to the LLM prompt:

1. **NPC identity pre-introduction.** Every co-located NPC is rendered as
   `"  - {display_name} ({occupation})"` regardless of `is_introduced`. The
   GUI hides occupation behind `{#if npc.introduced}` at
   `parish/apps/ui/src/components/MentionDropdown.svelte:24`. Surfaced bug:
   on a fresh save the LLM player addressed Peig Hannigan as "Widow" on
   turn 1, an honorific the player could not have known.

2. **Map geography beyond fog-of-war.** Every neighbour of the player's
   current location is exposed with `travel_minutes` and a `visited` flag,
   regardless of whether the GUI map (built via
   `parish/crates/parish-core/src/ipc/handlers.rs::build_map_data`) would
   show that location at all. `build_map_data` only reveals visited
   locations and the immediate frontier, and tooltip data for frontier
   nodes is limited.

After this change, the prompt-builder must derive from the same
GUI-facing IPC types the frontend consumes (`build_npcs_here`,
`build_map_data`, `WorldSnapshot`, text-log entries) so the two cannot
drift again.

## Criteria

- For an **un-introduced** NPC, the rendered demo-prompt NPC line contains
  only `display_name(false)` (i.e. the `brief_description`). The line
  contains neither the NPC's real `name` nor its `occupation` nor its
  `mood`.
  — observable via: capture the constructed `user_prompt` from a demo
    turn (new `tracing::info!` line in `get_llm_player_action`) and grep
    for the leaked tokens (`"Widow"`, `"Peig"`, etc.) against the
    NPC block.
- For an **introduced** NPC, the rendered demo-prompt NPC line contains
  the real name and occupation, matching the GUI introduced-state shape.
  — observable via: same capture, run after the player has been
    introduced to the NPC (`/introduce`-equivalent flow or scripted
    dialogue).
- The **adjacent-locations block** in the demo prompt is limited to the
  set returned by `build_map_data` (visited + frontier). Locations beyond
  the frontier are omitted. `travel_minutes` is omitted for unvisited
  frontier entries; visited entries still carry it (parity with map
  tooltips).
  — observable via: same prompt capture; assert the set of names in
    `Adjacent locations:` equals the set of names in the corresponding
    `MapData` payload for the same world state.
- A backend **unit / integration test** in `parish-tauri` (or wherever the
  builder lands) builds a `DemoContextSnapshot` for a state containing
  one un-introduced NPC whose `occupation = "Widow"`, renders the prompt,
  and asserts the rendered string does **not** contain the substring
  `"Widow"`.
- A backend **fitness test** asserts that every field reachable from the
  `DemoContextSnapshot` is also reachable from the four GUI-facing IPC
  types (`WorldSnapshot`, `NpcInfo`, `MapData`, `TextLogPayload`). The
  builder must accept only these as inputs.
- **Live demo proof.** A fresh-save `just demo 2 3` run on a location
  with at least one un-introduced NPC produces a captured `user_prompt`
  (via the new tracing line) whose NPC block contains the
  `brief_description` and no occupation token, and whose first-turn
  player dialogue contains neither the NPC's real name nor any
  occupation-derived vocative ("Widow", "Father", "Mister", "Miss",
  etc.).
- **Role-vocative resolver fallback.** When a player addresses an NPC
  by occupation rather than name (e.g. `talk to Widow` /
  `addressed_to: ["Father"]`), the dialogue resolver routes to the sole
  co-located NPC with that occupation. If two or more co-located NPCs
  share the role, the resolver returns empty so the existing
  ambiguity-error path fires rather than silently picking one.
  — observable via: unit tests for `find_by_role_at` and
    `resolve_npc_targets`.

## Verification script

Two-part verification (the headless CLI cannot directly exercise the
demo prompt path — demo orchestration is owned by the Tauri / web
frontend — so the live signal comes from a real demo run, with the
fixture establishing the GUI baseline the prompt must match).

Part A — GUI baseline (`/npcs`, `/map`, `/debug npcs` on fresh save):

```
cargo run --manifest-path parish/Cargo.toml -p parish-cli -- \
  --script parish/testing/fixtures/play_issue-998.txt
```

Expected signals in output:

- `/npcs` line for Peig before any introduction contains
  `"a small, sharp-eyed old woman wrapped in a shawl"` (her
  `brief_description`).
- No `/npcs` line pre-introduction contains `"Widow"`.
- `/map` shows only visited + frontier locations; un-frontier locations
  do not appear in the JSON payload.

Part B — live demo prompt capture:

```
cd parish && rm -f saves/parish_001.db* && just demo 2 3 \
  2>&1 | tee /tmp/issue-998-demo.log
grep -A20 "user_prompt:" /tmp/issue-998-demo.log
```

Expected signals in output:

- `tracing` line `user_prompt: ...` (new) showing the full constructed
  prompt for each demo turn.
- The captured prompt's `NPCs here:` block, for any un-introduced NPC,
  contains the brief_description and contains **none of**: the NPC's
  real name, the occupation string, the mood string.
- The captured prompt's `Adjacent locations:` block lists only the
  visited + frontier set from `MapData`.
- The first emitted player line in the log (`chat [player]` or
  equivalent) does not contain `"Widow"`, `"Peig"`, or any other token
  that requires post-introduction state.
