# Phase 6 — Living world

This local plan implements the [Milestone 6 requirements](../product-specs/mobile-product-technical-spec.md)
(spec §17, "Living world") and supplements the earlier phase gates. "Phase 6" is
product spec Milestone 6, which is `just mobile-verify --phase 6`. Issue
[#2194](https://github.com/dmooney/Rundale/issues/2194) was written as "phase 5"
before the owner split the old Milestone 5 into Milestones 5 (NPC dialogue), 6
(Living world) and 7 (Existing features usable) on 2026-10-24. The living-world
proofs are the exit gate of Milestone 6. This plan is not an execution record:
record results separately, as the [test plans README](README.md) describes,
including failures and gates that could not run.

The world is the canonical three-location, three-NPC world. Each proof uses one
authored fact and adds no NPC or location. The facts are in
`mods/rundale/living-world.json` and the `GOSSIP`, `TASK`, `WEATHER` and
`MOVEMENT` blocks of `mods/rundale/world-sheet.txt`:

| Proof    | Fact                                                                                                                                                  |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| Memory   | An NPC the player has given a name to remembers it; an NPC not told still sees a stranger                                                             |
| Gossip   | "A cattle buyer on the village road yesterday afternoon was paying good money for yearlings." Source Mícheál, path Mícheál to Róisín, never told Peig |
| Task     | Róisín asks the player to tell Peig at the Letter Office to hold any letter that comes for the Connollys until Róisín calls for it                    |
| Weather  | In heavy rain Peig stays at the Letter Office instead of walking to the village                                                                       |
| Movement | Peig walks Letter Office to Kilteevan Village at 07:00 and back at 09:00                                                                              |

## Automated regression matrix

Run `just mobile-verify --phase 6 --simulator <UDID>`. It runs the Phase 1–4
regression suites once, then the six living-world gates below, and lists Phase 5
(NPC dialogue, which has no gate yet) as non-blocking future work.
`just mobile-verify --phase all` runs the same gates. It reports a proof whose
issue has not landed as failed without blocking, so the recorded pass and the
nightly gate stay usable; an explicit `--phase 6` blocks on every proof.

The cases are Rust tests at the C boundary the app calls, against the shipped
world (`limerick/crates/limerick-mobile-ffi/src/tests/living_world.rs`). They
script inference at the Endpoint boundary: the test answers each pending
`rundale-intent` and `rundale-dialogue` call, as the Swift host does, so no model
runs and every reply is fixed. They do not establish live Firebase, App Attest,
model quality or UI behaviour. Cases of a proof whose issue is open sit behind the
`living-world-pending` Cargo feature, which only the Phase 6 gate builds, so the
default `cargo test` and CI stay green. Land a proof by making its cases pass,
then set its `issue` to `None` in `LIVING_WORLD_PROOFS`
(`mobile/scripts/verify.py`) and drop its `pending` module and feature gate.

Every proof has the same shape: a **before** state that shows the behaviour has
not happened, the **action** that causes it, an **after** state, **save/resume**
(close and reopen the save, then check the same state), and a **repeat run** on a
new game that must give the same before and after.

| Case  | Gate                         | Proof    | Status                  |
| ----- | ---------------------------- | -------- | ----------------------- |
| P6-01 | `living-world-memory`        | Memory   | Passes on this checkout |
| P6-02 | `living-world-gossip-start`  | Gossip   | Passes on this checkout |
| P6-03 | `living-world-gossip-spread` | Gossip   | Pending #2197           |
| P6-04 | `living-world-weather`       | Weather  | Passes on this checkout |
| P6-05 | `living-world-movement`      | Movement | Passes on this checkout |
| P6-06 | `living-world-task`          | Task     | Pending #2308           |

### P6-01 Memory ([#2196](https://github.com/dmooney/Rundale/issues/2196))

| Step | Exercise                                                           | Required outcome                                                                                             |
| ---- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| 1    | Before: at the cottage, say "Good morning to you." to Mícheál      | His dialogue input names the player a stranger he has never seen; `/debug memory Mícheál` lists nothing told |
| 2    | Say "My name is Seán and I am looking for work droving yearlings." | `/debug memory Mícheál` lists the name Seán and the fact "I am looking for work droving yearlings."          |
| 3    | After: ask "Do you remember my name?"                              | The next dialogue input says he has met the player: the name Seán and the player's own statement             |
| 4    | Save/resume: close and reopen the save                             | The memory is still there and the next input still carries the name and the statement                        |
| 5    | Say hello to Róisín, who was not told                              | Her input still shows a stranger without the name or the statement                                           |
| 6    | Repeat on a new game                                               | Same before and after                                                                                        |

The memory is saved state read from the engine, not inferred from the generated
reply: the name, and the plain first-person statements the player typed
(`detect_player_facts`: "I am…", "I have…", "I walked…"), at most three per
character, in the player's own words. Both go to the dialogue Endpoint as
`thisMoment.theStranger`. Whether the model's reply uses them is a model
behaviour the live run reports, not something this case asserts.

### P6-02 Gossip, before propagation

| Step | Exercise                                                     | Required outcome                                                  |
| ---- | ------------------------------------------------------------ | ----------------------------------------------------------------- |
| 1    | New game: `/debug gossip`, then for Mícheál, Róisín and Peig | One item, source NPC 2, known by one; only Mícheál holds it       |
| 2    | Close and reopen the save                                    | The same: the save restores its own network and does not reseed   |
| 3    | Talk to Peig at 08:00 and to Róisín at the cottage           | Neither dialogue input mentions the cattle buyer or the yearlings |
| 4    | Repeat on a new game                                         | Same                                                              |

### P6-03 Gossip, propagation ([#2197](https://github.com/dmooney/Rundale/issues/2197))

Gossip spreads only between Tier-2 NPCs in the same place, so keep the player away
from Connolly Cottage while Mícheál and Róisín are there together.

| Step | Exercise                                                         | Required outcome                                            |
| ---- | ---------------------------------------------------------------- | ----------------------------------------------------------- |
| 1    | Stay in the village and wait an hour at a time, up to a game day | Róisín comes to hold the fact; Mícheál still holds it       |
| 2    | `/debug gossip Peig`                                             | No items: the path is Mícheál to Róisín only                |
| 3    | Close and reopen the save                                        | Róisín still holds the fact; Peig still does not            |
| 4    | After: ask Róisín for news; then ask Peig at the Letter Office   | Róisín's dialogue input carries the fact; Peig's never does |
| 5    | Repeat on a new game                                             | The fact spreads again along the same path                  |

The phone's per-turn world pump skips gossip (`GossipMode::Skip`) until the
background inference seam ([#2025](https://github.com/dmooney/Rundale/issues/2025))
lets it run.

### P6-04 Weather behaviour ([#2198](https://github.com/dmooney/Rundale/issues/2198))

Set the weather with the existing `/weather <name>` command
(`limerick-core/src/ipc/commands/weather.rs`). The phone runs it like `/debug`:
the engine's local commands accept it (`limerick-core/src/turn/commands.rs`),
it is not advertised or shown in `/help`, and it adds no player-visible UI.
The engine rule is in `limerick-npc/src/schedule.rs`: a non-farm NPC bound for an
outdoor place in light rain, heavy rain or a storm goes home instead.

| Step | Exercise                                           | Required outcome                                                  |
| ---- | -------------------------------------------------- | ----------------------------------------------------------------- |
| 1    | New game, `/weather heavy rain`, wait to 08:00     | The village is empty and `/people` lists no one                   |
| 2    | Close and reopen the save                          | Still empty, and `/weather` still reports heavy rain              |
| 3    | Go to the Letter Office                            | Peig is there and can be spoken to; her input says it is raining  |
| 4    | Control: new game, `/weather clear`, wait to 08:00 | `/people` lists Peig, she can be spoken to, and a relaunch agrees |
| 5    | Repeat both                                        | Same outcomes                                                     |

### P6-05 Scheduled movement ([#2201](https://github.com/dmooney/Rundale/issues/2201))

| Step | Exercise                                                 | Required outcome                                                                                                        |
| ---- | -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| 1    | New game at 07:00 in the village; `/debug schedule Peig` | Schedule shows 07:00-08:00 Kilteevan Village; the village is empty (she is on the lane), and a relaunch changes nothing |
| 2    | Wait to 08:00                                            | `/people` lists the woman with the satchel; she can be spoken to; input says Kilteevan Village                          |
| 3    | Close and reopen the save, wait to 08:59                 | She is still in the village                                                                                             |
| 4    | Wait to 09:01 (mid-walk back)                            | Nobody is in the village; `/people` is empty; the intent call offers no one to speak to                                 |
| 5    | Close and reopen the save made mid-walk                  | Clock still 09:01, still nobody; after the walk she is at the Letter Office                                             |
| 6    | Go to the Letter Office                                  | Peig is there; input says Letter Office; the save agrees after a relaunch                                               |
| 7    | Repeat on a new game                                     | Same sequence                                                                                                           |

### P6-06 Task ([#2308](https://github.com/dmooney/Rundale/issues/2308))

The engine has no task ledger yet. The case reads one through
`/debug tasks`, which [#2308](https://github.com/dmooney/Rundale/issues/2308) must
provide (or this case changes with the seam it chooses). Stages are the
`TASK` block of the world sheet.

| Step | Exercise                                                     | Required outcome                                               |
| ---- | ------------------------------------------------------------ | -------------------------------------------------------------- |
| 1    | Before: at the cottage, `/debug tasks`                       | No tasks                                                       |
| 2    | Ask Róisín if you can help; she asks you to tell Peig; agree | The ledger holds the task, assigned                            |
| 3    | Close and reopen the save                                    | Still assigned                                                 |
| 4    | Walk to the Letter Office with the task unfinished           | The ledger shows it in progress                                |
| 5    | Give Róisín's message to Peig                                | The ledger shows it completed; a relaunch keeps it             |
| 6    | After: ask Peig about the message; return to Róisín          | Both speak of it as delivered (their inputs carry the message) |
| 7    | Repeat on a new game                                         | Same stages                                                    |

## Not covered here

- The dynamic dialogue snapshot rows (dynamic NPC context §8 rows 23, 24, 25, 34)
  and the recipient behaviour of [#2221](https://github.com/dmooney/Rundale/issues/2221)
  are NPC-dialogue (Milestone 5) concerns. Comments on #2194 asked this gate to
  carry them from when it was the old "phase 5". They belong to the Milestone 5
  gate, which does not exist yet.
- One live Endpoint run per proof, which the proof issues require. The live suite
  is opt-in and separate from these deterministic cases.
- Production-path proof on the simulator and the phase-end demonstration, which
  each proof issue and [#2199](https://github.com/dmooney/Rundale/issues/2199) own.
  No physical-iPhone result is implied.

## Physical sessions and phase-end demo

The phase-end demo follows the [phase demo plan](../product-specs/phase-demo-plan.md)
and shows all five proofs in the running build, each with its before, after,
relaunch and repeat. Simulator demonstrations must be labelled as such.
