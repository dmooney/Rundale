Evidence type: live gameplay transcript

# Evidence — npc-arrival-event-semantics

Live run of the round-trip fixture against the real CLI binary with an
isolated user-data dir.

## Run command

```sh
PARISH_USER_DATA_DIR=/tmp/parish-arrival-fix-proof \
  cargo run -p parish -- \
  --script parish/testing/fixtures/play_npc-arrival-event-semantics.txt
```

The fixture walks Kilteevan Village → Crossroads → Darcy's Pub →
Crossroads → Kilteevan Village. Before the fix, every `go to` round-trip
made the tier compute re-fire `NpcArrived` for every NPC the player
re-entered the vicinity of, producing duplicate journal entries on every
location and NPC file. After the fix, the event bus only sees
`NpcArrived` / `NpcDeparted` for real schedule-driven transit and Tier 3
LLM relocations.

## Mapping criteria → observed output

### C1: NpcArrived/NpcDeparted come from schedule transit

Schedule-driven moves observed in `sample-loc-015-kilteevan-village.txt`
and partner files:

| Time | Location file | Heading |
|---|---|---|
| 08:00 | kilteevan-village | `Brigid Ni Fhatharta departed` |
| 08:00 | kilteevan-village | `Aoife Brennan departed` |
| 08:00 | kilteevan-village | `Sean Ruadh Kelly departed` |
| 08:00 | kilteevan-village | `Mick Flanagan departed` |
| 08:13 | the-holy-well | `Brigid Ni Fhatharta arrived` |
| 08:15 | the-hedge-school | `Aoife Brennan arrived` |
| 08:21 | murphy-s-farm | `Sean Ruadh Kelly arrived` |

Each departure pairs with one arrival 13–21 minutes later — the schedule
transit time. Confirmed against `sample-npc-019-brigid.txt`: exactly one
"Departed from Kilteevan Village" and one "Arrived at The Holy Well",
matching her physical move.

### C2: cognitive tier promotion no longer fires NpcArrived

`sample-loc-002-darcy-s-pub.txt` records only ONE journal entry — the
`Player arrived` line. The two NPCs co-located there (Niamh, Padraig)
do not generate spurious `arrived` entries when the player walks in;
they were already at the pub. Pre-fix, every player entry to the pub
produced a `Niamh Darcy arrived` and `Padraig Darcy arrived` heading
because their cognitive tier flipped Tier4 → Tier1.

`sample-loc-001-the-crossroads.txt` shows two `Player arrived` entries
(outbound + return) and zero NPC arrival entries — the NPCs the player
passed through the crossroads do not phantom-arrive.

Backed by unit test `tier_promotion_does_not_fire_npc_arrived` in
`parish-persistence/src/snapshot.rs:1136-1198`, which drains the event
bus across a Tier2→Tier1 promotion and asserts zero `NpcArrived`
events.

### C3: Tier 3 LLM moves publish proper events

`apply_tier3_updates` in `parish-npc/src/ticks.rs:1141-1206` now takes
`event_bus: &parish_types::events::EventBus`. When an LLM-driven update
changes `npc.location`, it publishes `NpcDeparted { from }` and
`NpcArrived { new }`. The conditional `new_loc != npc.location` guard
prevents phantom arrivals when the LLM re-asserts the current location.
No Tier 3 ticks fired in this fixture (no inference running under the
script harness) but the path is unit-covered by `test_tier3_apply_basic`
and friends.

### C4: dedup state and helpers deleted

`grep -n "bump_last_arrival\|bump_npc\|bump_player\|last_npc_at\|
last_player_arrival\|scan_existing_npc_arrivals\|
scan_existing_player_arrival\|parse_last_arrival_location"
parish/crates/parish-core/src/{character_log,location_log}.rs` returns
nothing. The structs are stateless beyond `log_dir` and `enabled`.

Every `process_event` arm is a straight append — no early-return
filtering. The unit test `writer_appends_every_event_it_receives`
in `location_log.rs` asserts the new contract: two distinct
`NpcArrived` events bracketing one `NpcDeparted` produce two arrival
entries and one departure entry, with no filtering.

### C5: round-trip walk shows no duplicate entries

Headings produced per location file in the proof run:

```
$ for f in /tmp/parish-arrival-fix-proof/logs/branch-1/loc-*.md; do
    n=$(basename "$f"); c=$(grep -c "###" "$f");
    [ "$c" -gt 0 ] && echo "$n: $c"
  done
loc-001-the-crossroads.md: 2     # 2 player arrivals (out + back)
loc-002-darcy-s-pub.md: 1        # 1 player arrival
loc-006-the-hedge-school.md: 2   # Aoife arrives + Liam arrives
loc-008-hodson-bay.md: 1
loc-009-murphy-s-farm.md: 2      # Liam departs + Sean Ruadh arrives
loc-012-the-bog-road.md: 1
loc-013-connolly-s-shop.md: 1
loc-015-kilteevan-village.md: 5  # 4 departures + 1 player arrival
loc-016-the-forge.md: 1
loc-017-the-holy-well.md: 2      # Brigid + Nora arrive
loc-018-the-mill.md: 1
loc-020-the-boatman-s-cottage.md: 1
```

Pre-fix, the locations the player re-entered (Crossroads, Kilteevan
Village) would each carry several `<NPC name> arrived` entries per
visit; Darcy's Pub would show two phantom NPC arrivals next to the
player's own. Post-fix, every entry is a real, single, distinct event.

### C6: full test + lint suite passes

- `cargo test --workspace`: 2858 passed, 15 ignored.
- `cargo clippy --workspace --all-targets -- -D warnings`: no issues.
- `just check` (fmt + clippy + tests + agent-check): passes.

Acceptance criteria: met
