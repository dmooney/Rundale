# Critical Review — Character + Location Logs

Scrutiny of the per-character and per-location markdown logs across
the four proof bundles produced this session
(`location-context-logs`, `npc-arrival-event-semantics`,
`tier2-events-on-bus`, `tier2-npc-interaction-event`) plus the
30-/40-turn `just demo` transcripts.

Each finding is tagged **logging** (writer bug / UX gap) or **game**
(underlying simulation bug exposed by the log) and triaged
P0/P1/P2.

---

## Resolution status — P0 (verified 2026-05-27)

All eight P0 findings (F17–F21, F1–F3) are **resolved** in the current
tree. Verified by reading the live code paths, not by re-running the
demo:

| Finding | Resolution | Evidence |
| --- | --- | --- |
| F17 | `tier2_groups()` filters solo NPCs | `parish-npc/src/manager.rs:504` — <code>groups.retain(|_, ids| ids.len() >= 2)</code> (#1025) |
| F18 | Pronouns interpolated into Tier 2 prompt | `parish-npc/src/ticks.rs:765-768` — dramatis personae carry `(pronouns)`; prompt says "Refer to each character with the pronouns shown in parentheses" |
| F19 | Prompt constrains naming to participants + post-hoc filter | `parish-npc/src/ticks.rs:769` — "Only name characters listed… never a proper name"; `summary_mentions_absent_npc` drops violations |
| F20 | Free-text name auto-extraction wired in demo/headless | `parish-engine/src/headless.rs:942-946` calls `detect_player_name()` (regex at `parish-npc/src/lib.rs:703`) and sets `world.player_name` |
| F21 | Absent-aware routing refuses to mis-route | `parish-core/src/game_loop/npc_turn.rs:485-560` — emits "{name} is not here." and publishes `AddressedAbsentNpc` (#985, #1137) |
| F1 | Profile strips `{time}`/`{weather}` placeholders | `parish-core/src/location_log.rs` profile section; render only in journal entries (#1030) |
| F2 | `player.md` heading uses the named character | `parish-core/src/character_log.rs:339-342` — `format!("{} arrived", name)` from `world.player_name` |
| F3 | Departure body carries destination | `parish-core/src/location_log.rs:190` — `format!("*Headed to {}*", …)` (#1098) |

---

## New P0 findings from the 40-turn demo (NpcInteraction wired)

### F17. Tier 2 fires on solo NPCs and produces template filler *(game, P0)*

Aoife alone at the Hedge School from 08:23 onwards. Tier 2 fires
every ~12 game-minutes and the LLM dutifully produces:

```
### 08:36 — Interaction (1 present)
**Aoife Brennan:** Aoife Brennan goes about their business at The Hedge School.

### 09:09 — Interaction (1 present)
**Aoife Brennan:** Aoife Brennan goes about their business at The Hedge School.

### 09:21 — Interaction (1 present)
**Aoife Brennan:** Aoife Brennan goes about their business at The Hedge School.
... 10 more identical lines ...
```

10 identical entries in 90 game-minutes. The Tier 2 prompt is meant
to capture *group* dynamics — solo NPCs have no group to interact
with. Two cheap fixes:

1. `tier2_groups()` in `parish-npc/src/manager.rs` should require
   `len >= 2` before scheduling an LLM call. Solo NPCs fall through
   to Tier 3 / Tier 4.
2. If solo Tier 2 stays for other reasons (e.g. inner monologue),
   the writer should publish `NpcInteraction` only when
   `participants.len() >= 2`. Either cap is fine; (1) saves the LLM
   round-trip.

### F18. Solo Tier 2 narration uses singular "they/their" for known-gender NPCs *(game, P0)*

> "Aoife Brennan goes about **their** business"

Aoife is `gender: female` in the mod data. The Tier 2 prompt
either doesn't pass gender or the LLM is hedging. Either way the
log records a grammar mistake as canonical narrative.

Likely a Tier 2 prompt-template gap (`build_tier2_prompt` in
`parish-npc/src/ticks.rs:528`). Should interpolate
`he/she/they` from `npc.pronouns` if the field exists, or just say
"Aoife goes about her business".

### F19. Tier 2 narration hallucinates NPCs into the scene *(game, P0)*

Darcy's Pub log entry at 08:36:

> **Niamh Darcy, Padraig Darcy:** Padraig Darcy stands at the bar,
> minding his daughter Niamh as she chats animatedly with **Aoife
> Brennan**.

Aoife is at the Hedge School at 08:36 (see her own log — she
arrived there at 08:23 and stayed). She's not at Darcy's Pub. The
LLM is name-dropping NPCs it has seen in the prompt context.

The location log records the hallucination verbatim. Two fixes,
both at the Tier 2 prompt:

1. Constrain output to only name NPCs from the `participants` list
   (or include a system-prompt rule "do not mention any other named
   character").
2. After-the-fact filter — strip or flag mentions of NPCs not in
   `participants`.

(1) is the right place to attack it. The writer is correct to
record what the LLM said; the LLM is wrong.

### F20. Player-name extraction never fires under demo mode *(game, P0)*

Demo transcript: the player introduces themselves as "Aiden" four
times to Peig Hannigan and then to Fr. Tierney. The character log
shows every dialogue heading as `**A stranger:** ...` for the full
40 turns. `world.player_name` is never set, so
`player_diary_label_for` returns the fallback indefinitely.

Either the demo prompt is supposed to send a `/name Aiden` system
command and isn't, or the engine has no auto-extraction from
free-text introductions. Worth confirming — `mods/rundale/demo-prompt.txt`
likely never sets it.

### F21. Vocative addressing routes to the wrong NPC when the target has left *(game, P0)*

Demo at 09:12, after Peig Hannigan departed at 09:00:

> **A stranger:** Tell me, Mrs. Hannigan, what brings ye to stay in
> Kilteevan all yer life?
> **Fr. Declan Tierney:** Ah, the tales of Kilteevan do weave a fine
> tapestry...

Player explicitly addresses Mrs. Hannigan; engine routes to Fr.
Declan because Peig is no longer present. Logging-wise this is
correct (the dialogue did go to Declan). Game-wise, the right
behaviour is to refuse or warn ("Mrs. Hannigan isn't here"), not
silently mis-route.

This combines with issue #1019 (which I filed earlier in this
session) — the broader fix should both detect vocative NPC names
without `@` AND refuse to route to a different NPC when the
addressed one isn't present.

---

## P0 — wrong / misleading data

### F1. Profile-section description placeholders are never substituted *(logging, P0)*

Every location profile renders `{time}` and `{weather}` literally:

> Kilteevan Village — *"The {weather} sky hangs over the quiet street. It is {time}."*
> Darcy's Pub — *"It is {time} and the weather outside is {weather}."*

These tokens are intended for runtime render (see
`parish-world/src/description.rs::render_description`). The log
writer skips that step and stores the raw template, so the file
reads as broken to anyone opening it. Two cleanest fixes:

1. Strip placeholders entirely from the profile section — the file
   is meant to be a stable "vital stats" pane, not a render of the
   current moment.
2. Render the placeholders with the world state at profile-write
   time and add a note like "*(snapshot at session start)*".

(1) is the smaller change.

### F2. Player log records every arrival as "Player" instead of the named character *(logging, P0)*

Once the player has been introduced to any NPC, `world.player_name`
is set. The `PlayerMoved` handler in `character_log.rs` writes
"Player arrived" regardless. The NPC-log dialogue arm already uses
`player_diary_label_for` for the same purpose. Symmetric fix:
substitute the name in the player.md heading once set.

### F3. NpcDeparted with no destination context *(logging, P1)*

A non-resident appears in a location log only as "<NPC> departed"
with no body. Example from Kilteevan:

```
### 08:00 — Aoife Brennan departed
### 08:00 — Mick Flanagan departed
```

Aoife is the Hedge School teacher; Mick is a retired constable. Both
are visible departing Kilteevan because they were there overnight (a
schedule wrap from the previous evening). Neither is in
`associated_npcs`. Reader has no idea who they are or where they're
headed.

Fix: include `*Headed to <destination>*` in the departure body, the
same way `PlayerMoved` includes `*Arrived from <prev>*`. The
schedule already knows the destination at publish time (it's the
`to` field on `ScheduleEventKind::Departed`).

---

## P1 — missing or weak signal

### F4. WeatherChanged + FestivalStarted are logged only to the player's current location *(logging, P1)*

Weather is global. Festivals usually anchor to a specific location
that may not be the player's. Current code:

```rust
GameEvent::WeatherChanged { new_weather, .. } => {
    let Some(path) = path_for(world.player_location) else { return Ok(()); };
    ...
}
```

For weather, the right call is probably "log to every location" or
"log to none — keep weather only on the player log". For festivals,
the event itself should carry a `location: LocationId` field; right
now it doesn't.

### F5. No log of NPC activity at the destination *(logging, P1)*

When Brigid arrives at the Holy Well at 08:13, her log records the
arrival but not what she's doing there. Her schedule says "*gathering
watercress and praying*". This activity text is on
`ScheduleEntry.activity` and never reaches the bus.

The recently added `GameEvent::NpcInteraction` covers tier-2-driven
collective scenes. For solo schedule activity, a parallel
`GameEvent::NpcActivity { npc_id, location, activity, timestamp }`
fired once when an NPC arrives at a window with non-empty activity
text would close the gap. Same shape, cheap.

### F6. Tier 4 illness/recovery/death events publish locally but never reach the bus *(game, P1)*

`parish-npc/src/tier4.rs::apply_illness_event` returns
`Vec<GameEvent>` (line 372 — `events.push(GameEvent::LifeEvent
{...})` then `events.push(GameEvent::MoodChanged {...})`) but the
caller (`tick_banshee` and its peers) never publishes them. Result:
when Maire Gallagher catches consumption, her NPC log gets no entry,
the village log gets no entry, only the in-memory `WorldState.text_log`
shows it. Same anti-pattern the original `NpcArrived` bug used.

Fix: same shape — `apply_illness_event` takes `&EventBus` and
publishes directly, or callers fan the returned Vec out.

### F7. Gossip never reaches the bus *(game, P1)*

`create_gossip_from_tier2_event` adds a `Gossip` to
`world.gossip_network` whenever `|delta| > 0.3`. No `GameEvent` is
ever fired. So when Brigid hears at the Holy Well that Padraig was
arguing about the spring rents, the gossip propagates silently.
Neither log captures it.

Add `GameEvent::GossipSpread { source, content, timestamp }`
(or similar). Cheap once the pattern is established.

### F8. Tier 3 short-term memory → long-term memory promotion is invisible *(game, P1)*

`try_promote` runs on every memory eviction. The persistent file is
fed by the LTM bag but the log never records "X became a permanent
memory for Brigid". Less critical than F6/F7 but worth surfacing for
debugging memory churn.

---

## P2 — UX / craft

### F9. Player-introduction loops in demo are visible as silence in NPC logs *(game, P2)*

The 30-turn demo at Kilteevan: turns 1-15 are the player saying
"Good mornin'..." into the void because Peig isn't being addressed
correctly. Looking at the world.text_log: `> [system] No one here
answers to that name just now.` That's the engine refusing to route
the input.

This is the bug behind issue #1019. The character log captures none
of these attempts — `DialogueOccurred` only fires when a route
succeeds. So a reader of the log sees Peig's first reply at 08:34
out of nowhere, when in fact the player tried 14 times.

**Logging-wise**: should failed dialogue attempts be logged? Probably
not in the NPC's log (the NPC wasn't addressed), but the player's
log should show "*Tried to address an NPC by description; engine
didn't route.*" so the transcript reflects what the player did.

### F10. Profile section schedule duplicates Default and Sunday windows side by side *(logging, P2)*

Brigid's profile lists both her Default schedule and her Sunday
schedule. There's no indication of which is "today". Two cleanest
fixes:

1. Render only the active variant for the current `Season + DayType`.
2. Tag the active variant inline: `### Default · (active today)`.

(2) preserves the historical-reference value.

### F11. Schedule windows missing transit times *(logging, P2)*

Brigid's schedule entry: `08:00–09:00 @ The Holy Well — at the holy
well, gathering watercress and praying`. She actually arrived at
08:13 (13 minutes transit from Kilteevan). The profile renders
`08:00–09:00` as if she's there for the full hour. Either:

1. Add the travel time to the rendered window — `08:00 depart →
   08:13 arrive · 09:00 done`.
2. Add a one-line note to the section header: *"start_hour = depart-at,
   not arrive-at"*.

### F12. NPC and location slug filenames strip accents *(logging, P2)*

`npc-019-brigid-ni-fhatharta.md` — the actual name is *Ní
Fhatharta*. The slug drops the fada. Filesystem-friendly but loses
the orthography. If the user opens the file in an editor and sees
the title is `# Brigid Ni Fhatharta`, that's not an Irish name
anymore. The H1 in the file should keep the accents (does it
currently — yes, the H1 uses `npc.name` verbatim) but the slug for
filename should ideally use NFD-normalized ASCII (which it does).
This is fine; mentioning for completeness.

### F13. Identical heading + body events at the same minute collapse to one *(logging, P2)*

`append_journal_entry` has full-string idempotence to guard against
replays. If two genuine events with the exact same heading + body
fire at the same in-fiction minute (e.g. two NPCs both "arrived" at
the same place with no body), the second would be silently dropped.

NPCs with distinct names produce distinct headings (`Padraig Darcy
arrived` vs `Niamh Darcy arrived`), so the realistic collision is
very narrow. Not worth fixing unless it bites.

### F14. Player's `## Visited locations` section in profile is alpha-sorted by id, not by first-visit order *(logging, P2)*

A traveller's log should read chronologically. The current sort:

```rust
let mut visited: Vec<&LocationId> = world.visited_locations.iter().collect();
visited.sort_by_key(|id| id.0);
```

Sorts by numeric id — the order locations were authored, not the
order the player saw them. Track first-visit timestamp in
`WorldState` (or just append-only-Vec the visit list) and sort by
that.

### F15. Aliases section duplicates "town" and "the town" *(authoring data, P2)*

Kilteevan profile: `Also known as: town, the town, village, kilteevan`.
"town" and "the town" are both there. Authoring nit in
`mods/rundale/world.json`, not a logger bug, but visible.

### F16. Dialogue heading just says "Dialogue" *(logging, P2)*

`### 08:34 — Dialogue` could be `### 08:34 — Dialogue with Peig Hannigan`
in the location log (it's known which NPC was the partner). The
NPC's own log already encodes it implicitly (it's their file), but
the location log doesn't.

---

## Spot-checks that look correct

- C1 of npc-arrival-event-semantics holds: every "departed" is paired
  with an "arrived" 13–21 minutes later (matches schedule travel
  times in Rundale's world graph).
- Round-trip walks no longer produce duplicate `<NPC> arrived`
  headings — the original bug we fixed stays fixed under 40-turn
  load.
- Character log dialogue NPC-POV labels work: "*A stranger*" before
  introduction, "*<Player name>*" after. Verified by Padraig saying
  "I don't believe I've seen you before. I'm Padraig Darcy" — and
  the next dialogue line uses "Padraig Darcy".
- Branch-scoped directory (`logs/branch-1/`) survives session
  restarts without polluting the global user-data dir.

---

## Recommended next moves

1. **F1, F2, F3** — small, mechanical fixes; one PR each.
2. **F4** — needs an `Option<LocationId>` field on `FestivalStarted`
   (small schema change) and a decision about per-location weather
   logging.
3. **F5, F6, F7** — three new `GameEvent` variants
   (`NpcActivity`, propagated `LifeEvent`/`MoodChanged` from Tier 4,
   `GossipSpread`). Each follows the bridge pattern established by
   the NpcInteraction work. Logical follow-up to issue #1019.
4. **F9** — depends on issue #1019 fix; together they make the
   demo-mode transcript a fair record of player intent.
5. **F10, F11** — profile formatter rewrites; localized.

None of these is a blocker for the work already merged. Most are
gaps in what events the bus carries, not bugs in the writer.
