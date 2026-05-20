Evidence type: live gameplay transcript

# Evidence — character-context-logs

The transcript at [transcript.txt](transcript.txt) is the concatenation
of two consecutive `parish --script` runs of
[`parish/testing/fixtures/play_character-context-logs.txt`](../../../parish/testing/fixtures/play_character-context-logs.txt),
followed by an on-disk inspection of
`~/Library/Application Support/Rundale/logs/branch-1/` and the
`parish-core::character_log` unit-test suite. The runs exercised:

- starting at Kilteevan Village with the Rundale mod loaded
- movement Kilteevan → Crossroads → Darcy's Pub
- canned dialogue with Padraig Darcy and Niamh Darcy via `/stub`
- ~4 hours of `/wait` time (mood + schedule transitions fire)

`run_script_mode` calls `GameTestHarness::new_with_character_logs()` so
the live `--script` path emits logs. Plain `GameTestHarness::new()`
keeps writers disabled — earlier draft enabled them in `new()`, which
caused several hundred test-harness instances under `cargo test` to all
append to the shared `branch-1/player.md`, producing 700+ stale
arrival lines. The opt-in constructor + the in-writer dedup map
(`CharacterLogManager::bump_last_arrival`) together produce the clean
output below.

## Acceptance criteria → transcript evidence

### C1 — Log directory exists per branch

Lines 285–310 of `transcript.txt` list `player.md` and 23 `npc-*.md`
files under `~/Library/Application Support/Rundale/logs/branch-1/`,
total 1543 lines across 24 files. Slugified filenames match
`character_log::slugify(npc.name)`.

### C2 — NPC profile rewritten at session start

Lines 80–171 of `transcript.txt` are the verbatim profile section of
`npc-001-padraig-darcy.md`:

- `<!-- PROFILE_START -->` opens; `<!-- PROFILE_END -->` closes.
- Header: `# Padraig Darcy — Character Log`, `*Publican · Age 58 ·
  Home: Darcy's Pub*`.
- `## Intelligence` with all six dimensions.
- `## Backstory` lists Padraig's `knowledge` entries.
- `## Relationships` names ten NPCs, including `kind` and a strength
  bar.
- `## Schedule` renders three seasonal/day-type variants with
  `HH:00–HH:00 @ <location> — <activity>` lines.

### C3 — Profile rewrite preserves Journal across sessions

`transcript.txt` lines 235–260 are `player.md` after run 2; lines
263–282 are the player.md snapshot saved between run 1 and run 2. Both
run-1 entries (`Arrived at The Crossroads` at 08:20, `Arrived at
Darcy's Pub` at 08:51) are present in the post-run-2 file (lines 250
and 253) alongside the run-2 duplicates (lines 256, 259). The PROFILE
section between the markers was rewritten on run-2 startup — the file's
contents were not truncated.

The same invariant is covered by
`character_log::tests::profile_rewrite_preserves_journal` (line 316).

### C4 — `PlayerMoved` writes "Arrived at …" lines to `player.md`

`transcript.txt` lines 250–260 (post-run-2 player.md):

```
### Monday 20 March 1820, 08:20 — Arrived at The Crossroads
*From Kilteevan Village to The Crossroads*

### Monday 20 March 1820, 08:51 — Arrived at Darcy's Pub
*From The Crossroads to Darcy's Pub*

### Monday 20 March 1820, 08:20 — Arrived at The Crossroads
*From Kilteevan Village to The Crossroads*

### Monday 20 March 1820, 08:51 — Arrived at Darcy's Pub
*From The Crossroads to Darcy's Pub*
```

Exactly four arrivals (two per run × two runs) — one entry per
`GameEvent::PlayerMoved` published by
`parish_core::game_session::apply_movement`. No duplicates from the
in-writer dedup map.

### C5 — `DialogueOccurred` writes player + NPC lines to NPC logs

`transcript.txt` lines 165–171 (Padraig) and 226–232 (Niamh):

```
### Monday 20 March 1820, 08:51
**You:** say Niamh, how is the day with you?
**Padraig Darcy:** Ah, God bless ye, come in out of the morning.
```

```
### Monday 20 March 1820, 08:51
**You:** say Niamh, how is the day with you?
**Niamh Darcy:** Good day to ye, traveller.
```

Both NPC files received `**You:** …` followed by `**<NPC name>:** …`
blocks. The writer routed each `GameEvent::DialogueOccurred` to the
correct NPC file based on `npc_id`. Bodies are the verbatim
`player_said` / `npc_said` text.

### C6 — Journal headings carry game-time timestamps

Every `### …` heading in the journal uses
`Weekday DD Month YYYY, HH:MM`. The year is 1820 (Rundale mod
`start_date`), the weekday is Monday, and `HH:MM` advances with
`/wait`. Not wall-clock UTC.

### C7 — Feature flag gates the writer (unit-test evidence)

The `--script` harness does not load `parish-flags.json`, so an
in-process flag toggle does not survive across script runs. The
criterion is proven by
`character_log::tests::disabled_manager_is_noop` (line 314 of
`transcript.txt`), which constructs `CharacterLogManager::new("…", 1,
false)` and verifies that neither `write_all_profiles` nor
`process_event` writes anything. The hand-off from
`!flags.is_disabled("character-logs")` to `CharacterLogManager::new`'s
`enabled` parameter is in
[`parish-cli/src/headless.rs`](../../../parish/crates/parish-cli/src/headless.rs)
and
[`parish-cli/src/testing.rs`](../../../parish/crates/parish-cli/src/testing.rs).

## Dedup safeguard

Lines 285–310 of `transcript.txt` show every NPC log is between 55–85
lines. An earlier (buggy) draft of this writer emitted one
`NpcArrived` entry per tier-recompute, producing 280+ duplicate "Arrived
at Darcy's Pub" lines in Padraig's journal in the same number of game
minutes. Fixed by
`CharacterLogManager::bump_last_arrival` — a `Mutex<HashMap<NpcId,
(LocationId, DateTime<Utc>)>>` that drops an `NpcArrived` /
`NpcDeparted` event when the NPC's last journal-recorded location
matches.

The dedup is location-only and per-CharacterLogManager-instance — it
collapses the repeat tier-recompute pings into a single arrival entry
while still letting an NPC who actually moves elsewhere produce a new
entry on their return.
