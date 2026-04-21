# Night Visions — Dreams When the Player Sleeps

> Back to [Documentation Index](../index.md) | [Game Ideas Brainstorm](game-ideas-brainstorm.md) | [Mythology Hooks](mythology-hooks.md)

> Status: **parked** — design captured for a future implementation. First
> prototyped on branch `claude/serene-bardeen-6VMVM` (PR #489) as a
> standalone `/sleep` command; shelved because the parish engine does
> not yet model player vitals (fatigue) or housing (a bed to lie down in).
> Revisit once those systems land.

## TL;DR

When the player sleeps, surface a short italicised dream fragment
stitched together from their recent experience — a place they visited
this week, today's weather and season, the last NPC they spoke with —
crossed with a table of Irish folklore imagery. The generator is
pure, deterministic, seeded from the game clock, and needs no LLM
call. On festival mornings (Samhain, Bealtaine, Imbolc, Lughnasa) the
dream's closing line swaps to a festival-specific coda.

This is the first gameplay surface that *reflects the player's week
back at them as atmosphere*, rather than stating it factually.

## Why this is parked

The original prototype exposed `/sleep [hours]` as a standalone
command, but during review it became clear that without supporting
systems the feature is a free time-skip with poetry attached rather
than a real mechanic:

- **No fatigue.** `PlayerState` does not yet track tiredness
  ([game-mechanics-brainstorm.md §1d](game-mechanics-brainstorm.md)).
  Sleep should restore fatigue — with nothing to restore, there is no
  reason for the player to sleep beyond triggering a dream.
- **No housing.** There is no player home, no bed, no concept of
  "eligible sleeping location" ([game-mechanics-brainstorm.md §2, §10](game-mechanics-brainstorm.md)).
  Sleep outdoors in the rain should differ from sleep in one's own
  cot; we can't express either end of that scale yet.
- **No warmth / weather exposure.** Sleeping through a storm under a
  ditch should have consequences; the engine can't model them.

Night Visions is a *payoff* layer that rides on top of those systems.
Shipping it first would set the wrong expectation — players would
learn that `/sleep` is a dream-button, then be confused when later
patches make sleep expensive and location-sensitive.

## Prerequisites

These want to land before (or alongside) Night Visions:

1. **Player fatigue** (`PlayerState { fatigue: u8, … }`) — drains with
   travel and activity, restored only by sleeping.
2. **Player home / bed** — a designated `LocationId` that the player
   owns or rents. Eligible-to-sleep locations: own home (full rest),
   invited NPC home (partial), outdoors (minimal + exposure penalty).
3. **Warmth / exposure status effects** — so sleep location actually
   matters. A bed under a roof is materially different from a ditch
   in heavy rain.
4. **Interrupt hooks** — sleep can be broken by storms, NPC knocks,
   dawn bells. The cross-tier event bus already exists; sleep would
   subscribe to it.

Of those, only fatigue and a single "player home" location are
strictly necessary to make `/sleep` meaningful. Warmth and interrupts
can follow.

## Design sketch

### Command surface

```text
/sleep            → sleep until wake time (default ~8 hours)
/sleep [hours]    → sleep for N hours (clamp to [1, 24])
```

Natural-language equivalents (`"lie down"`, `"go to bed"`, `"rest"`)
should be parsed by the existing intent pipeline and reuse the same
handler.

Feature-flagged as `night-visions` (default on, kill-switchable via
`/flag disable night-visions`) so players who find the dream voice
intrusive can opt out without losing sleep itself.

### When a dream surfaces

A dream is generated when **all** of these are true:

- The flag `night-visions` is not explicitly disabled.
- The player is at an eligible sleep location (own home, invited
  home, or sheltered outdoor spot once those exist).
- The start hour is within a plausible sleeping window —
  `TimeOfDay::{Dusk, Night, Midnight, Dawn}`. A midday nap advances
  the clock but does not dream.
- The sleep duration is at least ~4 hours (short naps stay silent).

Dreams should surface *occasionally*, not every night, to keep the
feature special. A reasonable starting point: 60% chance on any
eligible night, rising toward 100% on festival eves and when the
player is near a mythology-hook location (Fairy Fort, Holy Well).

### Generator shape

Template-driven, seeded from the game clock (plus `player_location.0`
as a tiebreaker) so a given night produces a reproducible vision.
Four fragments joined into a single italicised passage:

1. **Opening** — one of a handful of stock lines
   (*"A dream comes upon you:"*, *"Sleep takes you down into a vision:"*).
2. **Setting** — `"You stand at {remembered_place}, in {season_phrase},
   with {weather_phrase}"`.
   - `remembered_place`: a location in `visited_locations` other than
     the current one, so dreams feel like *memory*, not a re-dressing
     of the room you fell asleep in.
   - `season_phrase` / `weather_phrase`: flat match on
     `Season` and `Weather`, one line each.
3. **Figure** — the most recent NPC from `ConversationLog`, drawn from
   a small table of dream-verbs (*"stands with their back to you,
   speaking low in Irish…"*). Falls back to the player's own name if
   introduced, else to a folklore archetype (old woman in a grey
   shawl, barefoot child, red-haired man at a ford).
4. **Omen** — one line from a table of Irish folklore images
   (banshee keen, white horse, fairy-fort lights, Cailleach's shawl,
   salmon swallowing the moon, …).
5. **Coda** — generic (*"You wake before you can name it."*) unless
   the current date is a festival, in which case a dedicated coda
   fires: Samhain's *"…the distance between worlds feels like nothing
   at all."*, Bealtaine's *"…fires on the hills, and a May dew on the
   sill."*, Imbolc's Brigid line, Lughnasa's ripe corn.

### Why pure templates (at least to start)

The prototype deliberately avoids an LLM call for dream generation:

- **Deterministic** — same seed → same dream, so saves/replays
  reproduce exactly and tests can assert on output text.
- **Fast & offline** — no network, no GPU; works in the simulator.
- **Low-stakes tone** — short, vague, rhythm-balanced lines read as
  dream logic regardless of recombination. The LLM can hallucinate a
  tone-breaking dream; the table cannot.
- **Easy to kill-switch** — a pure function is trivial to gate.

An optional LLM enrichment pass could layer on top later (e.g. one
Tier-3 call per night to reword the template output into prose) without
changing the shape of the API. The generator should always fall back
to the template if inference fails.

## Example output (from the prototype)

Running the prototype's play-test script (see the archived branch)
produced dreams like these. These are included here so future
implementers have a concrete tone target to hit.

A spring night after walking the parish:

```text
> /sleep 8
  You lie down and close your eyes for 8 hours...
  It is now 05:58 Dawn.

  The dark behind your eyes loosens, and a dream begins:

    *You stand at Darcy's Pub, in spring light before the grass
     remembers itself, with a low sky pressing close*
    *A red-haired man stands at a ford and will not let you cross*
    *You see a salmon silver in a dark stream, swallowing the moon
     whole.*

  The dream ebbs, but something in it stays with you.
```

Samhain Eve — the coda switches to the festival line:

```text
> /sleep 8
  Sleep takes you down into a vision:

    *You stand at The Fairy Fort, in autumn half-dark, smoke curling
     low along the fields, with heavy rain drumming on a roof you
     cannot see*
    *An old woman in a grey shawl lifts her head and knows you at once*
    *You see a salmon silver in a dark stream, swallowing the moon
     whole.*

  You wake on Samhain night, and the distance between worlds feels
  like nothing at all.
```

## Tables (starter content)

Captured here so a future implementer doesn't have to rewrite them.
Keep each line short, rhythm-balanced, and vague enough to read as
dream logic in any combination.

### Folklore images

- a woman keening far off, then a silence that will not lift
- a white horse with no rider, trotting west along the bog road
- faint music beneath a hill — almost a reel, almost a hymn
- lights in the fairy fort, blue and wandering, that refuse to be counted
- a figure of turf and stream-water stepping from the rushes
- the Cailleach's grey shawl brushing the thatch above your head
- a crow that lands on your chest and will not be named
- the hush of dew, a heartbeat before the first cock crow
- a salmon silver in a dark stream, swallowing the moon whole
- a candle that will not blow out, no matter how you cup your hand
- the sound of many small feet in the loft above a stilled house
- a white hawthorn in blossom where none grew before
- the tide going out from a field that never saw a sea
- a door opening in a stone wall, and beyond it only more stone

### Weather phrases (one per `Weather` variant)

| Weather | Phrase |
|---|---|
| Clear | a still, clear air |
| PartlyCloudy | clouds drawn thin across the moon |
| Overcast | a low sky pressing close |
| LightRain | a light rain that wets without sound |
| HeavyRain | heavy rain drumming on a roof you cannot see |
| Fog | a fog that eats the road ten steps ahead |
| Storm | a wind that rises and rises and does not break |

### Season phrases (one per `Season` variant)

| Season | Phrase |
|---|---|
| Spring | spring light before the grass remembers itself |
| Summer | long summer dusk, the sky the colour of whey |
| Autumn | autumn half-dark, smoke curling low along the fields |
| Winter | bare winter cold, the stars hard as flint |

### Festival codas

| Festival | Coda |
|---|---|
| Samhain | You wake on Samhain night, and the distance between worlds feels like nothing at all. |
| Bealtaine | You wake into Bealtaine morning — fires on the hills, and a May dew on the sill. |
| Imbolc | You wake into Imbolc's first thin light, and Brigid's name is on your breath. |
| Lughnasa | You wake into Lughnasa, the corn ripe in your fingers though no corn is there. |

## Connections to existing systems

| System | How Night Visions uses it |
|---|---|
| `GameClock` / `TimeOfDay` / `Festival` | Sleeping-hour gate; festival coda selection; seed derivation. |
| `Weather` / `Season` | Setting-line phrasing. |
| `WorldState::visited_locations` + `locations` | Pool of "remembered" places to name in dreams. |
| `ConversationLog` | Most-recent-speaker name in the figure line. |
| `WorldState::player_name` | Fallback when there is no recent conversation. |
| `FeatureFlags` (`night-visions`) | Kill-switch. |
| (future) `PlayerState::fatigue` | Actual reason to sleep; scales dream probability. |
| (future) player home / eligible sleep locations | Gates whether sleep is even possible here. |
| (future) mythology hooks / liminal sites | Raises dream probability near the Fairy Fort, Holy Well, graveyard, etc. |

## Relationship to the `/omen` work (PR #487)

`/omen` triggers liminal moments at mythological *sites* — the Fairy
Fort at dusk, the Holy Well on a pattern day. Night Visions triggers
on *time* — a sleeping player at any eligible location. The two are
complementary: `/omen` is place-bound, Night Visions is time-bound,
and they can share the same folklore tables and tone.

## Open questions

- Should dreams also draw on **gossip** the player has overheard
  rather than only direct conversation? This would make information
  the player never experienced first-hand leak through as dream
  imagery, which is evocative but potentially confusing.
- Should the generator ever surface a **prophecy** — a dream that
  foreshadows a scripted NPC lifecycle event (c.f. the Banshee idea in
  [game-ideas-brainstorm.md §10](game-ideas-brainstorm.md))? That
  tilts from "atmosphere" into "quest hook", which is a different
  design pillar.
- Does `/sleep` itself live in gameplay or in a meta-layer? If the
  player has to *say* "I want to lie down" via natural language, the
  intent parser needs a new verb; if it's only a slash command, it
  sits next to `/wait`.
- What's the right **dream frequency**? Every eligible night is too
  much; once a week is too rare. A cooldown + context bonuses
  (festival eve, near a mythology site, after a funeral or wedding)
  probably wins.

## Implementation notes from the shelved prototype

The prototype on branch `claude/serene-bardeen-6VMVM` had all the
generator pieces working against the simulator provider:

- `parish_world::night_vision::{NightVision, generate_vision,
  is_sleeping_hour}` — 8 unit tests
- `Command::Sleep(u32)` + `/sleep [hours]` parsing with clamp to
  `[1, 24]` — 5 unit tests
- `handle_command` wiring with feature-flag kill-switch — 4 unit tests
- Integration test in `tests/headless_script_tests.rs` asserting the
  full beat list against a fixture script

Those snippets are salvageable once the prerequisite systems land;
reviving them should mostly be a `git show` from that branch plus the
gating work for fatigue / home-location.

