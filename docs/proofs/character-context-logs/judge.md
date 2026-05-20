# Judge — character-context-logs

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Per-criterion verification

[C1 — Log directory exists per branch]: transcript lines 285–310 list
`player.md` + 23 `npc-*.md` files under
`/Users/dmooney/Library/Application Support/Rundale/logs/branch-1/`,
total 1543 lines across 24 files. Filenames match the
`npc-NNN-<slug>.md` shape declared in `character_log.rs::npc_log_path`.
Met.

[C2 — Profile rewritten at session start]: transcript lines 80–171
show a full NPC profile block (Padraig Darcy) — markers, name + age +
occupation header, all six Intelligence fields, four `Backstory`
items, ten named `Relationships` (with `kind` + strength bar), and
three seasonal `Schedule` variants. Met.

[C3 — Profile rewrite preserves Journal]: the run-1 snapshot of
`player.md` (transcript lines 263–282) was saved between the two runs.
After run 2's profile rewrite, both run-1 entries are still present
(lines 250 and 253) and run-2 entries are appended below (lines 256,
259). The unit test `profile_rewrite_preserves_journal` proves the
same invariant in isolation. Met.

[C4 — `PlayerMoved` writes "Arrived at …"]: transcript lines 250–260
show exactly four matching headings in `player.md` — two per run × two
runs. Each carries the destination name and a `*From <origin> to
<destination>*` body. Met.

[C5 — Dialogue writes player and NPC lines]: transcript lines 165–171
and 226–232 show `**You:** …` followed by `**Padraig Darcy:** …` /
`**Niamh Darcy:** …` blocks in the correct NPC's log. Routing by
`npc_id` is correct. Met.

[C6 — Game-time headings]: every `### …` heading uses
`Weekday DD Month YYYY, HH:MM`. Year 1820, weekday Monday, `HH:MM`
matches the in-fiction clock advanced by `/wait`. Met.

[C7 — Feature flag gates the writer]: unit test
`disabled_manager_is_noop` (transcript line 314) constructs
`CharacterLogManager::new(…, false)` and verifies neither
`write_all_profiles` nor `process_event` produces files. The
`!flags.is_disabled("character-logs")` → `enabled` hand-off is wired
in `parish-cli/src/headless.rs` and
`parish-cli/src/testing.rs`. Met.

## Dedup correctness

An earlier draft of this writer emitted one `NpcArrived` entry per
tier-recompute, which `assign_tiers` republishes on every
`/wait`-driven schedule tick. A few game-hours produced 280+ identical
"Arrived at Darcy's Pub" lines in one NPC's journal. Two-line fix:

- `CharacterLogManager::bump_last_arrival` (and the player twin
  `bump_last_player_arrival`) tracks the last location written for each
  subject in a `Mutex<HashMap<NpcId, (LocationId, DateTime<Utc>)>>`.
  Duplicate-location events are dropped.
- The `NpcArrived` / `NpcDeparted` body line was removed because the
  heading already carries the location — the previous body was a verbatim
  duplicate.

Transcript lines 285–310 confirm every NPC log is 55–85 lines (profile
section + a handful of arrival/dialogue entries from the two runs), not
the 290+ lines per file the buggy version produced.

## Cargo-test isolation

`GameTestHarness::new()` now keeps writers disabled by default;
`GameTestHarness::new_with_character_logs()` is the opt-in path used
only by `run_script_mode`. Earlier draft enabled writers unconditionally
in `new()` so the hundreds of unit and integration tests that
instantiate a harness all appended to the shared
`branch-1/player.md`, producing ~787 stale arrivals in a single
`just check` run. The opt-in keeps the harness deterministic for tests
while still letting `parish --script` produce the proof bundle.

## Technical debt

Clear. No new `#[allow]`, no `unsafe` left over (an earlier draft of
the env-var approach was rolled back to a clean type-safe builder), no
TODOs, no temporary instrumentation.
