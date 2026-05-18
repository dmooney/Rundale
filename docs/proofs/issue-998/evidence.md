# Evidence: issue-998

Evidence type: live gameplay transcript

This bundle covers two live signals that together prove the demo
auto-player no longer sees information the GUI hides:

1. **GUI baseline** — `fixture-output.txt` is the captured headless
   CLI run of `parish/testing/fixtures/play_issue-998.txt`. It
   establishes what the player can see in Kilteevan at fresh save.
2. **Demo prompt** — `rendered-prompt.txt` is the actual `user_prompt`
   the demo auto-player would receive at the same world state. It is
   produced by the new `build_demo_context` →
   `render_user_prompt` pipeline running against the rundale mod's
   real world data (integration test
   `print_fresh_save_prompt_for_proof_bundle`).

The pipeline is the same one `parish-tauri::commands::get_demo_context`
and `get_llm_player_action` use at runtime. The new `tracing::info!`
line in `get_llm_player_action` (`user_prompt = ...`) emits the exact
same string to `just demo` logs, so live demo runs continue to surface
the prompt for inspection.

## Mapping each acceptance criterion to evidence

### 1. Pre-introduction NPC line contains only `display_name(false)`

`rendered-prompt.txt`, `NPCs here:` block:

```
  - a lean, red-haired young man with hard eyes, feeling bitter
  - a young woman, feeling passionate
  - a small, sharp-eyed old woman wrapped in a shawl, feeling sharp
  - an older woman with sharp eyes and herb-stained fingers, feeling watchful
  - an older man, feeling watchful
```

Each line is `{brief_description}, feeling {mood}`. No occupation
appears for any NPC. No real name appears either — `mods/rundale/npcs.json`
has these NPCs as Sean Ruadh Kelly, Aoife Brennan, Peig Hannigan, Brigid
Ni Fhatharta, Mick Flanagan respectively, and none of those names is
present in the prompt.

### 2. Post-introduction NPC line contains real name + occupation

Unit test `parish_core::ipc::demo::tests::introduced_npc_exposes_occupation`
asserts the rendered prompt contains `Padraig O'Brien, Publican, feeling warm`
for an introduced NPC.

### 3. Adjacent-locations block applies fog-of-war; travel_minutes hidden for unvisited frontier

`rendered-prompt.txt`, `Adjacent locations:` block:

```
  - Knockcroghery Village — unvisited
  - The Lime Kiln — unvisited
  - The Forge — unvisited
  ...
```

Every entry is `{name} — unvisited` — no travel-minute leak at fresh save.

The `demo_prompt_adjacent_block_mirrors_map_fog_of_war` integration test
asserts the set is a strict subset of what `build_map_data` exposes and
that travel_minutes is `None` for every unvisited entry.

Before the fix the OLD prompt printed `(13 min, unvisited)` for every
neighbour regardless of fog-of-war state — see the original
`commands.rs:2154-2176` adjacency builder. Compare with the headless
fixture's `look` output:

```
You can go to: The Crossroads (13 min on foot), The Forge (1 min on foot), ...
```

— that `look` text is a separate path the GUI also shows, so the demo
prompt can still reach it via `recent_log` once the player runs `look`.
The fix removes the unconditional leak from the adjacent block; if the
player has not yet looked around, the LLM only sees names.

### 4. Backend unit test asserts no `"Widow"` leak

`parish_core::ipc::demo::tests::rendered_prompt_does_not_contain_widow_pre_intro`:

```
test ipc::demo::tests::rendered_prompt_does_not_contain_widow_pre_intro ... ok
```

Companion: `pre_introduction_npc_hides_occupation_and_real_name` and
`rendered_prompt_avoids_parenthetical_vocative_format` confirm the
shape of the redaction and that no `(Title)` parens remain.

### 5. Fitness check: prompt is derivable only from GUI IPC types

The builder signature in
[`parish/crates/parish-core/src/ipc/demo.rs`](../../../parish/crates/parish-core/src/ipc/demo.rs)
is:

```rust
pub fn build_demo_context(
    snapshot: &WorldSnapshot,
    npcs: &[NpcInfo],
    map: &MapData,
    game_time: String,
    season: String,
    extra_prompt: Option<String>,
) -> DemoContextSnapshot
```

`WorldSnapshot`, `NpcInfo`, `MapData` are the same GUI-facing IPC types
the frontend already consumes. The signature itself is the test — no
`WorldState`, `NpcManager`, or other raw game state can reach the
builder. The Tauri command in
[`parish/crates/parish-tauri/src/commands.rs`](../../../parish/crates/parish-tauri/src/commands.rs)
now does:

```rust
let world_snapshot = parish_core::ipc::handlers::snapshot_from_world(&world);
let npcs = parish_core::ipc::handlers::build_npcs_here(&world, &npc_manager);
let map = parish_core::ipc::handlers::build_map_data(&world, state.transport.default_mode(), false);
build_demo_context(&world_snapshot, &npcs, &map, game_time, season, extra_prompt)
```

so the production path goes through the GUI-facing payloads too.

### 6. Live demo proof: no leaked tokens in prompt at fresh-save Kilteevan

Integration test
`demo_prompt_at_fresh_save_does_not_leak_widow_or_peig`
loads the rundale mod, builds the snapshot exactly as production does,
renders the prompt, and asserts that none of `Widow`, `Peig`,
`Hannigan`, `Publican`, `Gallagher` appears. Passing.

Coupled with `rendered-prompt.txt`, this is the live signal: the actual
string the LLM would see is captured, inspectable by eye, and free of
the tokens that produced the original `Good mornin', Widow.` failure.

### 7. Role-vocative resolver fallback (defence in depth)

The demo prompt no longer suggests role-vocatives, but a human player
can still type "Good mornin', Widow" or "Father, a word." Issue #998's
umbrella description called the resolver fix a secondary item; it's
bundled here so the symptom ("No one here answers to that name just
now.") is fully closed.

`NpcManager::find_by_role_at` (in
[`parish/crates/parish-npc/src/manager.rs`](../../../parish/crates/parish-npc/src/manager.rs))
matches an occupation case-insensitively against co-located NPCs and
returns `Some(&Npc)` **only** when exactly one NPC has the matching
role. `resolve_npc_targets` calls it as a fallback after the name
lookup misses.

Tests:
- `manager::tests::test_find_by_role_at_unique_match_resolves` — "Widow"
  resolves to the sole co-located widow.
- `test_find_by_role_at_case_insensitive` — "father" / "FATHER" both
  resolve.
- `test_find_by_role_at_ambiguous_returns_none` — two co-located
  farmers force the resolver to refuse rather than guess.
- `test_find_by_role_at_wrong_location_returns_none` — role match
  respects player location.
- `resolve_npc_targets_role_vocative_resolves_when_unambiguous` /
  `_refuses_when_ambiguous` / `_case_insensitive` — end-to-end through
  the IPC resolver.

## Test summary

```
parish-core unit (ipc::demo)      6/6 passed
parish-core unit (resolver)       6/6 passed
parish-core integration tests     3/3 passed
parish-npc unit (manager + role)  451/451 passed
parish-tauri lib + suites         all green
parish/apps/ui vitest            33/33 files, 401/401 tests passed
cargo fmt                         clean
cargo clippy (core + tauri)       no issues
```
