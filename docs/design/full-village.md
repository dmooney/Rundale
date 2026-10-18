# Proposal: size and shape of the full village

> Status: Proposed · Issue: [#2203](https://github.com/dmooney/Rundale/issues/2203) ·
> Milestone: Full village (spec Milestone 6) · Awaiting owner approval

Product spec Milestone 6 grows the canonical tiny world (three locations, three
NPCs) into a full village, then stops for a release. The spec does not set a
target, so this spike recommends one. It authors no content and changes no
engine or app code. When the owner approves a target, each batch below becomes
its own issue in **Full village**. Each issue states its player need, acceptance
criteria and test plan before implementation.

## 1. Recommendation

**Nine locations and twelve NPCs.** The acceptable range is 7–10 locations and
10–14 NPCs. The world is the village of Kilteevan and its nearest landmarks.
Neighbouring villages and towns are off-map references only.

|                                                        | Tiny world (today) | Recommended                   | Pre-reset desktop world    |
| ------------------------------------------------------ | ------------------ | ----------------------------- | -------------------------- |
| Locations                                              | 3                  | 9                             | 22                         |
| NPCs                                                   | 3                  | 12 (11 if batch 6 is dropped) | 23                         |
| Households                                             | 2                  | 6                             | —                          |
| Most exits from one location                           | 2                  | 4                             | 10                         |
| Most people in one place, at the hours the sheet lists | 2                  | 4                             | not capped                 |
| Longest walk between two neighbouring places           | 4 min              | about 10 min                  | 120 min (the engine's cap) |
| Batches to get there                                   | —                  | 6                             | —                          |

### 1.1 Why this size

The sizing unit is a play session. The spec's success test (§20) is a player
who understands where they are and who is around, and its Quality Gate (§18)
expects a normal 20-minute session. Walks of 2–10 game minutes, plus a few
exchanges at each stop, let a player reach five to seven places and meet eight
to ten people in that session. At nine places and twelve people, a tester can
see most of the world in one sitting and hold all of it in their head. At the
pre-reset scale they could not.

What each addition is for:

- **A place** earns its spot if it gives people a reason to cross paths, or a
  reason for the player to walk there: a workplace, a gathering spot, or a
  landmark tied to weather or a task. The legacy Lime Kiln and Weaver's Cottage
  met neither test, and the Hurling Green needs sport mechanics that are not
  planned.
- **A person** earns their spot by adding an edge the living-world mechanics can
  use: a new gossip hop, a schedule that crosses others, a task giver, or a
  weather behaviour. The Milestone 5 proofs each run on one path through three
  NPCs. Twelve people give several distinct gossip paths, and a tester can still
  say for certain who should and should not know a fact.

Where more stops helping:

- **Past about 12 named people,** a session reaches a shrinking share of the
  village. Gossip arrives about strangers, and the tester can no longer explain an
  outcome from memory, which Milestone 5's exit criterion requires.
- **Past about 4 people in one place,** the "Who are you speaking to?" question
  that unaddressed lines get comes with a long list of choices (§4).
- **Past about 10 locations at real distances,** the next places worth adding
  (Lough Ree, the mills, neighbouring villages) are 30 minutes to 2 hours away on
  foot. Each one adds a long walk with nothing new on the way.

### 1.2 Alternatives

**A. Village core only: 6 locations, 8 NPCs.** The tiny world plus the chapel,
the forge and the pub. It is the cheapest option, three batches, and every place
is within a five-minute walk. But it has no landscape for weather to act on: no
bog, no well, no open ground. Gossip paths stay short, and the village road
becomes a six-exit hub unless a second hub is added anyway. Choose this if
release matters more than breadth.

**C. Parish scale: about 14 locations, about 20 NPCs.** The recommendation plus
Lough Ree shore, a mill, a farm household and a ringfort. It comes closest to the
living-parish premise and gives the longest gossip chains. It also approaches the
legacy scale the spec says not to restore wholesale (Milestone 6: "NPC count is
increased gradually rather than restoring the previous population wholesale"). The
lough is a 40-minute walk, the busiest rooms exceed four people, and testers stop
being able to reason about the whole world. If it is ever wanted, it belongs after
Release.

## 2. Shape

### 2.1 Scope of the map

The world is the village core plus two landmarks within a short walk: the holy
well and the turf bog. Kilteevan parish in 1837 had about 2,568 people,
"a great quantity of bog", a Catholic chapel and two private schools, and Lough
Ree as its eastern boundary (Lewis, _Topographical Dictionary of Ireland_,
1837). The proposal keeps the bog, the chapel and the school. The school is a
hedge school, not a location (§4). The lough is left out because of the walking
distance.

**Off-map references:** Roscommon town (a few miles north-west, where the post
and the land agent come from), Lanesborough (north-east along the road),
Knockcroghery (south) and Athlone. They appear in NPC knowledge and dialogue, and
in Peig's letters. They are never destinations. "go to Roscommon" gets the
existing reply for an unknown destination. The legacy world had Knockcroghery and
Curraghboy as places. Knockcroghery was an 85-minute walk from the village, and
Curraghboy about 16.5 km away, which the engine caps at 120 minutes. That is the
wrong cost for a phone session.

### 2.2 Map

```text
                 Letter Office
                       |
 Holy Well -- Chapel -- Kilteevan Village -- Market Cross -- Gallagher's Forge
                       |                        |    \
               Connolly Cottage                 |     Darcy's Pub
                       \                        |
                        `--------- Turf Bog ----'
```

| Location                 | Exits                                                       | Who lives or works here | What it is for                                                            |
| ------------------------ | ----------------------------------------------------------- | ----------------------- | ------------------------------------------------------------------------- |
| Kilteevan Village (have) | Letter Office, Connolly Cottage, Chapel, Market Cross       | —                       | The street. Afternoon trade, people passing through.                      |
| Letter Office (have)     | Kilteevan Village                                           | Peig                    | News from beyond the parish; a gossip source.                             |
| Connolly Cottage (have)  | Kilteevan Village, Turf Bog                                 | Mícheál, Róisín         | Smallholding. The back boreen to the bog closes the loop.                 |
| Chapel                   | Kilteevan Village, Holy Well                                | Fr. Tierney             | Morning mass, the priest's calls. Shelter for the hedge school in rain.   |
| Holy Well                | Chapel                                                      | —                       | Tobernagreaghta, the "well of the wounds". A quiet errand destination.    |
| Market Cross             | Kilteevan Village, Gallagher's Forge, Darcy's Pub, Turf Bog | —                       | Second hub and meeting place. The hedge school meets here in dry weather. |
| Gallagher's Forge        | Market Cross                                                | Séamus, Colm, Aoife     | Workplace and household.                                                  |
| Darcy's Pub              | Market Cross                                                | Pádraig, Niamh          | The evening gathering, and the main place gossip spreads.                 |
| Turf Bog                 | Market Cross, Connolly Cottage                              | Tommy, Donncha          | Turf cutting; weather decides who is out on it.                           |

The Market Cross is a real feature: a limestone wayside cross dated 1632, on the
road into Kilteevan, where a market was once held. The holy well is
Tobernagreaghta, recorded in the parish's folklore collection. Batch issues pin
both with the geo tool (`/rundale-geo-tool`). If a real pin gives a walking time
outside the targets below, the place is placed as `geo_kind: fictional`, relative
to the village, with an honest `geo_source`, as the tiny world already does.

### 2.3 Travel graph rules

- **At most four exits per location.** Most have one or two. The tiny world's
  three locations keep their existing exits. Kilteevan Village gains Chapel and
  Market Cross, and Connolly Cottage gains Turf Bog.
- **Connected places 150–750 m apart: walks of 2–10 game minutes.** The engine
  computes a walk from the distance between coordinates at 1.25 m/s
  (`mods/rundale/transport.toml`), rounded up to whole minutes, with a floor of
  1 minute and a cap of 120 (`meters_to_minutes` in `limerick-world`). So this is
  a placement rule, stated as distance because the 1-minute floor hides placements
  that are too close. The legacy world put Darcy's Pub 26 m from the Crossroads,
  which plays as a 1-minute walk, and had hops of 6–16 km. The tiny world's walks
  are 190 m and 263 m (3 and 4 minutes).
- **No two places further apart than four connections.** The longest trip, Holy Well
  to Darcy's Pub, is four hops, about 20 minutes.
- **One loop:** village, cottage, bog, cross. People on the bog can be met from
  either end, and there is a second way home.

### 2.4 Names

- **Location names stay short:** the longest proposed, "Gallagher's Forge", is
  17 characters, against "Kilteevan Village" today. The compact status header
  shows the name on one line (`mobile/Rundale/ContentView.swift`), so a long name
  truncates at large Dynamic Type sizes. Batch 2 checks its longest name at the
  largest supported size in the simulator. No leading "The", matching "Letter
  Office".
- **Aliases must not collide.** Kilteevan Village already has the aliases "road"
  and "village road", so the legacy "Bog Road" is renamed "Turf Bog", with the
  aliases "bog" and "turf". Each new location gets a distinct one-word alias:
  chapel, well, cross, forge, pub, bog.

## 3. Population

### 3.1 Roster

There are six households. "Major" NPCs have a full schedule, several knowledge
items, three or four authored relationships, and may carry a living-world
mechanic. "Minor" NPCs have a short personality, one or two knowledge items, at
most two relationships, and a schedule that stays mostly at home and work. The
data model is the same for both; minor is an authoring budget, not a new kind.

| NPC                | Age | Role                                  | Home              | Major or minor | Batch        | Pre-reset source                                         |
| ------------------ | --- | ------------------------------------- | ----------------- | -------------- | ------------ | -------------------------------------------------------- |
| Peig Hannigan      | 54  | Letter-office keeper                  | Letter Office     | Major          | have         | Name reused for a new role in the reset                  |
| Mícheál Connolly   | 38  | Smallholder and drover                | Connolly Cottage  | Major          | have         | New in the reset                                         |
| Róisín Connolly    | 31  | Spinner and household bookkeeper      | Connolly Cottage  | Major          | have         | Name reused for a new role in the reset                  |
| Fr. Declan Tierney | 62  | Parish priest                         | Chapel            | Major          | 1            | Reviewed legacy NPC                                      |
| Séamus Gallagher   | 42  | Blacksmith                            | Gallagher's Forge | Major          | 2            | Reviewed legacy NPC                                      |
| Colm Gallagher     | 15  | His son and apprentice; runs errands  | Gallagher's Forge | Minor          | 2            | Reviewed legacy NPC                                      |
| Pádraig Darcy      | 58  | Publican                              | Darcy's Pub       | Major          | 3            | Reviewed legacy NPC                                      |
| Niamh Darcy        | 22  | His daughter; keeps the pub with him  | Darcy's Pub       | Minor          | 3            | Reviewed legacy NPC                                      |
| Aoife Brennan      | 29  | Hedge schoolmistress; Séamus's niece  | Gallagher's Forge | Major          | 4            | Reviewed legacy NPC (moves to the forge as a lodger)     |
| Tommy O'Brien      | 70  | Old cottier; cuts turf, tells stories | Turf Bog          | Minor          | 5            | Reviewed legacy NPC (from O'Brien's Farm to a bog cabin) |
| Donncha Fallon     | 26  | Labourer; hired by the day            | Turf Bog          | Major          | 5            | Replaces legacy Sean Ruadh Kelly (§6.2)                  |
| Martin Concannon   | 33  | Land agent's clerk; lodges with Peig  | Letter Office     | Major          | 6 (optional) | Reviewed legacy NPC                                      |

Ages, roles and names are provisional until each batch's data review.

**Naming rules,** which every batch checks:

- No two NPCs share a first name, and no surname is shared across households.
  Within a household, a shared surname is intended, because it exercises "Which
  Gallagher do you mean?". The legacy world had a Roisin Connolly at Connolly's
  Shop, unrelated to any Connolly household; that pattern is excluded.
- No first name the recipient tests use as a probe. In the
  [recipient contract](dynamic-npc-context.md)'s acceptance matrix (§8, rows 10
  and 13), "ask Seán about the road" is the unknown-name case, and "Mary, mother
  of God" is the exclamation case, run in a fixture world with a Mary elsewhere.
  A Seán or a Mary in the production world would turn those probes into live
  ambiguity. This rules out Seán, Máire and Mary, and makes Brigid and Bríd (as
  in "Brigid's cross") unwise.
- Roles are unique, so a role used as a form of address ("Father", "Smith",
  "Missus") picks out one person.
- People are fictional, as [ADR 009](../adr/009-real-geography-fictional-people.md)
  requires. Real landlords and families named in the parish's historical and
  folklore records (the gentry at Kilteevan House and Beechwood, the Kelly
  smiths) are not used as characters.

### 3.2 Who is where

This table shows where people are in the recommended world, including the
optional Martin. Each batch's world sheet uses its own subset. The original
three keep their tiny-world schedules.

| Hour  | Busiest places (people present)                                                                                                          |
| ----- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| 08:00 | Forge 3 (Séamus, Colm, Aoife) · Cottage 2 · Bog 2 · Village 1 (Peig) · Letter Office 1 (Martin) · Chapel 1 · Holy Well 1 (Niamh) · Pub 1 |
| 10:00 | Cottage 2 · Forge 2 · Pub 2 · Bog 2 · Market Cross 2 (Aoife's school, Martin) · Village 1 (Fr. Tierney) · Letter Office 1                |
| 13:00 | Pub 3 (with Martin) · Village 2 · Market Cross 2 (Colm, Aoife) · Bog 2 · Letter Office 1 · Chapel 1 · Forge 1                            |
| 15:00 | Village 3 (Mícheál, Róisín, Aoife) · Market Cross 2 · Forge 2 · Bog 2 · Letter Office 1 · Chapel 1 · Pub 1                               |
| 19:00 | Pub 4 (Pádraig, Niamh, Séamus, Fr. Tierney) · Letter Office 2 · Cottage 2 · Forge 2 · Bog 2                                              |
| 21:00 | Pub 4 (Pádraig, Niamh, Tommy, Donncha) · Forge 3 · Letter Office 2 · Cottage 2 · Chapel 1                                                |

Rain, once #2198 provides weather-dependent behaviour (§3.3), moves Aoife's
school into the chapel and Tommy into Darcy's until 18:00:

| Hour (wet) | Places that change                                                                                    |
| ---------- | ----------------------------------------------------------------------------------------------------- |
| 10:00      | Pub 3 (Pádraig, Niamh, Tommy) · Chapel 1 (Aoife's school) · Market Cross 1 (Martin) · Bog 1           |
| 13:00      | Pub 4 (Pádraig, Niamh, Martin, Tommy) · Chapel 2 (Fr. Tierney, Aoife) · Market Cross 1 (Colm) · Bog 1 |
| 19:00      | Unchanged: Tommy is home on the bog by then                                                           |

The cap is **four people in one place**, dry or wet, at every hour the sheet
lists. Batches that would exceed it stagger schedules instead.

**Sundays and market days.** Schedules can vary by day type (weekday, Sunday,
market day), and an 1820 Sunday would put most of the village at Mass in the
chapel at once, far over the cap. Saturday market day sends people to town,
which is off the map. The proposal therefore gives every NPC one schedule for all
days, as the tiny world does: an unscoped schedule variant applies on every day
type. A Sunday or market-day variant needs its own proposal, with a way to show
Mass that does not put a dozen people in one room's choices. That is decision 5
in §5.3.

### 3.3 What carries the living-world mechanics

The five Milestone 5 proofs stay with the original three NPCs in the tiny
world, which remains the regression fixture (#2200). The full village carries
the same mechanics with more paths:

- **Gossip:** Peig's letters are the source. NPCs pass gossip only when they
  share a place, and under the original schedules Peig never shares one with the
  Connollys on a weekday. So the route runs through the priest instead: Fr.
  Tierney calls at the Letter Office for his letters at 09:00, takes a glass at
  Darcy's at 19:00 with Pádraig, Niamh and Séamus, and Séamus carries it home to
  Colm and Aoife at the forge. Batch 3 can specify that route. A tester can
  still name, before and after, who should know a fact.
- **Tasks:** candidate givers are Séamus (carry a mended pot to the Connollys),
  Niamh (bring water from the holy well) and Fr. Tierney (bring a letter to Peig).
  At most one new task per batch, each with its own acceptance criteria.
- **Weather:** Aoife's hedge school meets at the Market Cross in dry weather and
  in the chapel in rain. Tommy cuts turf when it is dry and sits by the fire at
  Darcy's when it rains, until 18:00 (§3.2). Both depend
  on the weather-behaviour mechanism Milestone 5 delivers (#2198). Schedules vary
  today only by season and day type, so batches 4 and 5 wait for that mechanism
  or use whatever it provides.
- **Schedules:** every new NPC crosses at least one other at a public place, so
  scheduled movement is visible without waiting through an empty hour.
- **Tension:** the optional land agent's clerk lodges with Peig, who keeps
  private letters private. He has a rival in the labourer Donncha. Batch 6 adds
  him only if earlier batches show the village needs conflict.

## 4. Phone constraints

| Constraint                             | Limit in this proposal                                                         | Why it holds                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| -------------------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/exits` length                        | At most 4 names                                                                | "You can go to: Letter Office, Connolly Cottage, Chapel, Market Cross" wraps to two lines on a small iPhone. The legacy village hub listed 10.                                                                                                                                                                                                                                                                                                             |
| `/people` length                       | At most 4 at the sheet's hours                                                 | Fits one short block; the legacy forge, mill and cottage households held 3 each before visitors.                                                                                                                                                                                                                                                                                                                                                           |
| How often the game asks who you mean   | At most 5 buttons: 4 people and **All**                                        | Under the [recipient contract](dynamic-npc-context.md) (§3.1, step 5), an unaddressed line with two or more present and no partner always asks. Small rooms keep the question short and rare.                                                                                                                                                                                                                                                              |
| Status header                          | No name longer than today's "Kilteevan Village"                                | §2.4; checked at the largest Dynamic Type size in batch 2.                                                                                                                                                                                                                                                                                                                                                                                                 |
| Per-turn dialogue context and cost     | 12 people and 9 places in every call; up to 4 speakers or reactions per line   | The dialogue snapshot (§6 of the contract) carries every NPC as `knownPeople` and every place, with its description, as `knownPlaces`, so each call's input grows with the whole world, not only the room. That is about half the legacy world's 23 and 22. The `present` list and the number of speakers grow with the room, which the four-person cap bounds. Post-turn reactions, once mobile runs them (#2025), make one call for each person present. |
| Recipient decision options             | Up to 4 present, plus up to 11 introduced people elsewhere, plus fixed options | §4.3 of the contract offers every introduced absent person as an option. A larger roster means a longer option list, so batch 3 re-runs the labelled recipient cases with its roster of 8 introduced, and batch 5 repeats it with 11.                                                                                                                                                                                                                      |
| A tester holds the world in their head | One sheet of about 100 lines; 2–4 authored relationships per NPC               | §5.1.                                                                                                                                                                                                                                                                                                                                                                                                                                                      |

## 5. Batches

### 5.1 Plan

Every batch adds at most two locations and three NPCs. Each one leaves a playable,
sheet-consistent world, and passes the per-increment gates (offline play, local
saves, transcript stability, request atomicity, duplicate prevention,
accessibility) through `just mobile-verify --phase 6`. Batches start after
Milestone 5 closes and #2200 lands the fixture, gate and proposal template.

| Batch | Theme                           | Adds                                           | World after | What the world sheet gains                                                                                              | What it tests first                                                                 |
| ----- | ------------------------------- | ---------------------------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| 1     | The chapel and the priest       | Chapel; Fr. Tierney                            | 4 / 4       | CHAPEL block; Chapel on the village's exits; TIERNEY with relationships to all three originals                          | A third exit on the hub; a first resident outside the original households           |
| 2     | The market cross and the forge  | Market Cross, Gallagher's Forge; Séamus, Colm  | 6 / 6       | Two location blocks; the second hub; a household of two with a shared surname                                           | The hub's fourth exit; a place with no residents; "Which Gallagher?"                |
| 3     | The pub and its keepers         | Darcy's Pub; Pádraig, Niamh                    | 7 / 8       | PUB block with evening presence lines at the four-person cap                                                            | Evening gathering; peak-occupancy turn cost; first recipient re-run (§4)            |
| 4     | The schoolmistress and the rain | Aoife (lodger at the forge); no location       | 7 / 9       | AOIFE block; her dry and wet locations, in whatever form #2198 gives the sheet for weather; one gossip path through her | A batch with only an NPC; a second weather behaviour; a cross-household gossip path |
| 5     | The bog and the well            | Turf Bog, Holy Well; Tommy, Donncha            | 9 / 11      | Two location blocks; the loop exit on Connolly Cottage; Niamh's morning trip to the well                                | The loop; the longest walks; weather on the bog; second recipient re-run            |
| 6     | The agent's clerk (optional)    | Martin Concannon (lodger at the Letter Office) | 9 / 12      | CONCANNON block; a second resident at the Letter Office                                                                 | A lodger in an existing household; a rival relationship                             |

Batch 6 is included only if playtesting after batch 5 shows the village lacks
tension. Without it, the target is nine locations and eleven NPCs.

The world sheet ends at about 9 location blocks of four to six lines and 12 NPC
blocks of three lines, plus the Milestone 5 fact lines: about 100 lines, short
enough to read in one sitting. Each NPC keeps two to four authored
relationships, so the `knows:` lines stay short.

### 5.2 What each batch issue must state

The template from #2200, filled in:

- **Player need:** what the player can do or notice that they could not before.
- **Data review:** for each reintroduced place or person, what changed from the
  pre-reset data and why (§6).
- **Acceptance criteria,** including the world sheet and the `world_sheet`
  oracle agreeing, exits and presence within the caps, and walking times in the
  1–10 minute band.
- **Test plan:** the `limerick-core` turn tests on the production world, the
  Phase 6 gate, and a simulator walkthrough of the new places and people.
- **Evidence:** a recording of the walkthrough, paced for a human viewer.

### 5.3 Decisions the owner is asked to make

1. The target: the recommendation (9 locations, 12 NPCs), alternative A
   (6 and 8), or alternative C (about 14 and about 20).
2. Whether batch 6 is in the plan or held back.
3. Whether the production world may change the original three NPCs' schedules
   (for example, sending Mícheál to the bog on dry mornings), now that the tiny
   world is kept as a separate fixture. This proposal assumes it does not.
4. Whether the provisional names are acceptable, given the naming rules in §3.1.
5. Whether the full village keeps one schedule for every day type, as proposed
   (§3.2), leaving Sunday Mass and market days to a later proposal.

## 6. Reuse of pre-reset content

The pre-reset desktop world (22 locations, 23 NPCs) is at
`limerick/testing/fixtures/mods/rundale-legacy/`, matching commit `8d3a77eee`
before the reset in #2084. Spec Milestone 6 reintroduces content "only after
their data is reviewed" and does not assume it is correct "merely because it
previously shipped". The verdicts below decide only what is worth reviewing.
Each reintroduction is reviewed again in its batch.

### 6.1 Locations

| Legacy location                   | Verdict                      | Reason                                                                                                                                                                                                       |
| --------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Kilteevan Village                 | Kept (tiny world)            | —                                                                                                                                                                                                            |
| The Letter Office                 | Kept (tiny world)            | —                                                                                                                                                                                                            |
| Darcy's Pub                       | Review, batch 3              | Needed for the evening gathering. Re-place it: legacy coordinates put it 26 m from the Crossroads, a 0.4-minute walk.                                                                                        |
| St. Brigid's Church               | Review as Chapel, batch 1    | The parish had a Catholic chapel in 1837. The first Ordnance Survey map marks a St. Bridget's well beside Tobernagreaghta, but no church of that name, and the batch review decides the chapel's dedication. |
| The Forge                         | Review, batch 2              | Strong workplace and household; smithies are recorded in the parish in 1837.                                                                                                                                 |
| The Holy Well                     | Review, batch 5              | Tobernagreaghta is recorded. Mythology systems stay unplanned, so it is a place, not a mechanic.                                                                                                             |
| The Bog Road                      | Review as Turf Bog, batch 5  | The parish is mostly bog. Renamed because "road" is a village alias.                                                                                                                                         |
| The Crossroads                    | Replaced by Market Cross     | Legacy hub with 7 exits. The real 1632 Market Cross serves the same role with 4.                                                                                                                             |
| The Hedge School                  | Folded into Aoife's schedule | A hedge school met wherever it could; modelling it as weather-dependent behaviour at the cross and chapel serves play better.                                                                                |
| Connolly's Shop                   | Not reintroduced             | Its name collides with Connolly Cottage, and Róisín already trades goods on the road.                                                                                                                        |
| Murphy's Farm, O'Brien's Farm     | Not now                      | Connolly Cottage already covers smallholding life. Tommy O'Brien moves to the bog.                                                                                                                           |
| The Mill                          | Not now                      | Real mills existed in Belderg and Cloonsellan. A candidate for alternative C or after Release.                                                                                                               |
| The Fairy Fort                    | Not now                      | Ringforts are common in the parish, but mythology needs design before it is planned (spec §13.3). It may appear in descriptions.                                                                             |
| Lough Ree Shore                   | Not now                      | About 40 minutes' walk; alternative C only.                                                                                                                                                                  |
| The Lime Kiln                     | Not reintroduced             | Recorded in 1837 but gives the player nothing to do.                                                                                                                                                         |
| The Hurling Green                 | Not reintroduced             | Needs sport or festival mechanics, which are not planned.                                                                                                                                                    |
| The Weaver's Cottage              | Not reintroduced             | Róisín already spins; a second textile household adds a duplicate role.                                                                                                                                      |
| Hodson Bay, The Boatman's Cottage | Not reintroduced             | Near Athlone, far outside a walkable village.                                                                                                                                                                |
| Knockcroghery, Curraghboy         | Off-map references           | 85 and 136 minutes' walk.                                                                                                                                                                                    |

### 6.2 NPCs

| Legacy NPC                                          | Verdict                    | Reason                                                                                                                                                                                                                                                                                                                                                             |
| --------------------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Padraig Darcy, Niamh Darcy                          | Review, batch 3            | Publican household. Drop Niamh's romance with Brendan Duffy, who is not reintroduced.                                                                                                                                                                                                                                                                              |
| Fr. Declan Tierney                                  | Review, batch 1            | Remove his legacy schedule's visits to the Fairy Fort. Add a morning call at the Letter Office (§3.3).                                                                                                                                                                                                                                                             |
| Seamus Gallagher, Colm Gallagher                    | Review, batch 2            | Household of two.                                                                                                                                                                                                                                                                                                                                                  |
| Aoife Brennan                                       | Review, batch 4            | The legacy world made her Máire Gallagher's younger sister, housed on the village street. With Máire not reintroduced, she becomes Séamus's niece and lodges at the forge. Her legacy personality calls hedge schooling illegal under the Penal Laws; the Relief Act of 1782 had already allowed Catholic schoolmasters to teach, so the review checks that claim. |
| Tommy O'Brien                                       | Review, batch 5 (minor)    | Moves from a farm to a bog cabin; storyteller at Darcy's. Remove his legacy schedule's visits to the Fairy Fort.                                                                                                                                                                                                                                                   |
| Martin Concannon                                    | Review, batch 6 (optional) | Lodges at the Letter Office as before, now with Peig. His rival relationship carries over to Donncha.                                                                                                                                                                                                                                                              |
| Sean Ruadh Kelly                                    | Role kept, person replaced | "Seán" is the recipient tests' unknown-name probe, and the parish's folklore records name a real Kelly family of smiths. Becomes Donncha Fallon.                                                                                                                                                                                                                   |
| Maire Gallagher                                     | Not now                    | Keeps the forge household small, and "Mary" is the exclamation probe.                                                                                                                                                                                                                                                                                              |
| Brigid Ni Fhatharta (midwife)                       | Not now                    | Good role, but the name collides with common speech. A candidate if a batch needs a healer.                                                                                                                                                                                                                                                                        |
| Peig Hannigan (widow), Roisin Connolly (shopkeeper) | Names already reused       | The reset reused both names for different people.                                                                                                                                                                                                                                                                                                                  |
| Siobhan and Liam Murphy                             | Not now                    | Their farm is not reintroduced.                                                                                                                                                                                                                                                                                                                                    |
| Cormac, Nora and Brendan Duffy                      | Not now                    | The mill is not reintroduced.                                                                                                                                                                                                                                                                                                                                      |
| Eamon, Kathleen and Ciaran Walsh                    | Not reintroduced           | Lake household, outside the map.                                                                                                                                                                                                                                                                                                                                   |
| Una Malone                                          | Not reintroduced           | Duplicates Róisín's spinning.                                                                                                                                                                                                                                                                                                                                      |
| Mick Flanagan (retired constable)                   | Not now                    | Before the county constabulary of 1822 the role needs historical review, and he adds no mechanic the roster lacks.                                                                                                                                                                                                                                                 |

## Sources

- Samuel Lewis, [_A Topographical Dictionary of Ireland_, "Kilteevan"](https://libraryireland.com/topog/K/Kilteevan-Ballintobber-Roscommon.php) (1837): population, bog, chapel, schools, Lough Ree boundary, the Lanesborough road.
- Kilteevan Community Development Group, [Kilteevan History and Heritage](https://www.kilteevancommunity.ie/heritage/kilteevan-history-and-heritage), drawing on the Schools' Collection (1938) and the Ordnance Survey Name Books (1836–37): the Market Cross, Tobernagreaghta, smithies, mills, kilns and ringforts.
- [Townlands in Kilteevan](https://www.townlands.ie/roscommon/kilteevan1/): the parish's 23 townlands.
- Pre-reset world data: `limerick/testing/fixtures/mods/rundale-legacy/world.json` and `npcs.json`. Walking times in this proposal are computed from their coordinates at 1.25 m/s.
