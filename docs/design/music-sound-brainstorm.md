# Music & Sound: Creative Vision

> [Docs Index](../index.md) | [Ambient Sound Design](ambient-sound.md) | [ADR-015](../adr/015-ambient-sound-system.md) | [Music Research](../research/music-entertainment.md) | [Audio Sources](../research/ambient-sound-sources.md)

*This is a brainstorming document — a creative vision for how Parish should sound and feel. The technical architecture lives in [ambient-sound.md](ambient-sound.md). The historical research lives in [music-entertainment.md](../research/music-entertainment.md). This document is about the why and the what-if.*

---

## Part 1: Philosophy — What Minecraft and Factorio Teach Us

### The Minecraft Lesson: Silence Is the Instrument

C418's Minecraft soundtrack is one of the most beloved in gaming history, and its secret is **absence**. Music doesn't play constantly. There are long stretches — 10, 15, 20 minutes — of pure environmental sound: wind, footsteps, water, distant cave noises. Then, without warning, a spare piano melody drifts in. "Sweden." "Wet Hands." "Minecraft." The effect is devastating precisely because of the silence that preceded it. The music doesn't narrate your experience — it *accompanies* it, arriving like weather, departing like a thought you can't quite hold.

Key principles to steal:

- **Infrequent music with long cooldowns.** When a piece plays, it should feel like a gift, not wallpaper. Target: music triggers every 15–25 real-world minutes, lasting 2–4 minutes. The rest is environmental sound and silence.
- **No victory fanfares, no danger stingers.** The music doesn't tell you how to feel. It exists alongside you. A melancholy piano piece can play while you're having a cheerful conversation at the pub. The emotional dissonance is part of the magic.
- **The music belongs to the world, not the player.** It's not a soundtrack *for* you — it's as though you happened to overhear the world thinking out loud.

### The Factorio Lesson: Sound as Presence

Factorio's soundtrack does something different but equally relevant: it makes you feel like you're *inside a system*. The ambient industrial textures — drones, mechanical rhythms, slow synth pads — mirror the factory you're building. The music gets denser as your factory grows. It's not reactive in a moment-to-moment way; it's reactive to the *state of the world*.

Key principles to steal:

- **Ambient density reflects world state.** A quiet winter night at the Fairy Fort should sound fundamentally different from a summer evening at the Crossroads with a dance in full swing. Not just different sounds — different *density* of sound.
- **Long-session tolerance.** Parish sessions can run 2–6 hours. Nothing should become grating. Loops must be long (3–5 minutes minimum), varied, and layered so they never feel repetitive.
- **Sound is information.** In Factorio, you can *hear* that something's wrong before you see it. In Parish, you should be able to *hear* that it's getting late (birdsong fading, evening settling sounds), that weather is changing (wind picking up before rain arrives), or that the pub is lively tonight (distant fiddle music).

### The Parish Synthesis: Sound as World-Building

Parish is a text game. The player reads descriptions and types commands. They see a minimap and a sidebar. Their visual channel is occupied by *reading*. This means **sound has an outsized role in creating presence**. It fills the sensory gap that text alone cannot bridge.

You can describe rain. But *hearing* it — a steady patter on a thatched roof, punctuated by the distant toll of the Angelus bell — creates the feeling of *being there*. The player's imagination does the visual work from text; sound provides the emotional and spatial grounding.

**The goal: the player closes their eyes for a moment and they are in 1820s Kilteevan.**

---

## Part 2: The Living Soundscape

### Layer 1: The Breath of the Land (Always-On Ambient)

Every location has a base ambient layer that plays continuously — the "ground truth" of being in that place. This is the equivalent of Minecraft's cave ambience or the gentle wind on a hilltop. It should be so natural that the player only notices it when it changes.

| Location Type | Base Ambient | Character |
|---|---|---|
| **Bog Road** | Wind over open blanket bog, distant heather rustle | Lonely, exposed, vast. The wind is the main character here. |
| **Lough Ree Shore** | Water lapping on stones, wind in reeds | Meditative, rhythmic. The lake breathes. |
| **Hodson Bay** | Sheltered water sounds, rope creaking on moorings, gentler wind | More human — boats imply people even when absent. |
| **Fairy Fort** | Wind through hawthorn (higher pitch, more sibilant), insect hum in summer | Watchful. The silence *between* sounds is what unsettles. |
| **Crossroads** | Open wind, distant sounds bleeding from nearby locations | A mixing point — you hear *everything else* faintly. The crossroads is between places. |
| **Village** | Domestic hum — distant doors, muffled voices, chimney smoke (a crackling quality to the air) | Inhabited. Human warmth. |
| **Church** | Interior: stone reverb, near-silence with faint echo quality. Exterior: graveyard wind, birdsong | Sacred space. Acoustically distinct from everywhere else. |
| **Pub** | Hearth fire (autumn/winter), wooden creaks, glass/bottle ambient | Warm, enclosed. The contrast with outside weather should be striking. |
| **Farms** | Chickens (morning), cattle (background), dog (occasional), tools (intermittent) | Working landscape. These sounds have *schedules* — they follow the farming day. |

**Design note:** The base ambient for each location should have 2–3 variants that rotate randomly, so the player never hears the exact same loop twice in a row. Each variant should be 3–5 minutes long.

### Layer 2: The Clock (Time-of-Day Sounds)

The day has a shape. Sound should trace that shape so the player *feels* time passing without checking the clock.

**Dawn (5:00–7:00)**
The world wakes up in layers:
- First rooster crow (Murphy's Farm, propagates Near) — this is the **alarm clock of the parish**
- Second rooster from the Village (30 seconds later, slightly different pitch)
- Dawn chorus: songbirds building from a single voice to a chorus over ~2 minutes
- First cattle lowing
- Church bell: the Dawn Angelus (three sets of three tolls, then a continuous peal — heard from *everywhere*)

*The feeling: the world stretching, yawning, coming to life. A gentle crescendo.*

**Morning (7:00–11:00)**
The busiest soundscape:
- Full birdsong (spring/summer) or sparse, hardy birds (winter)
- Farm work sounds: threshing, chopping, cattle being moved
- Children's voices from the Village and Hedge School (school days only)
- Cart wheels on roads (intermittent — someone is always going somewhere)
- Donkey braying (carries Medium distance — you hear O'Brien's donkey from the Crossroads)

*The feeling: industry, community, life in motion.*

**Midday (11:00–14:00)**
A lull. The world pauses:
- Birdsong thins (birds rest at midday in summer heat)
- Midday Angelus bell (the parish's heartbeat)
- Insect buzz rises (summer)
- Quieter human activity — the energy dips

*The feeling: pause, breath, the sun at its highest.*

**Afternoon (14:00–17:00)**
Activity resumes but with a different quality — more leisurely:
- Birdsong returns
- Tommy O'Brien walks to the Fairy Fort (his footsteps on the path, if you're nearby)
- Wind often picks up in afternoon
- Sounds of commerce at the Shop (door, voices)

*The feeling: the long afternoon, time stretching.*

**Dusk (17:00–20:00)**
The most atmospheric transition:
- Evening Angelus bell (the third and final tolling of the day)
- Birdsong shifts to evening species (blackbird, robin — later singers)
- Farm animals settle (final lowing, sheep gathering)
- Pub sounds begin — first faint, then building
- Summer: crossroads dance music starts (fiddle tuning up, then a reel)
- Crows returning to roost (a cawing, wheeling sound overhead)
- The wind often drops at dusk — a brief, eerie stillness

*The feeling: transition, the veil between day and night. Everything changes.*

**Night (20:00–23:00)**
The human world concentrates indoors:
- Pub at full volume (if nearby): fiddle, singing, conversation, laughter, glasses
- Outside: quiet. Wind. Occasional owl.
- A dog barking in the distance (one farm, then answered by another)
- Footsteps on a road (someone going home from the pub)
- In winter: absolute silence broken only by wind

*The feeling: the parish draws inward. Warmth behind closed doors.*

**Midnight (23:00–5:00)**
The world belongs to something else:
- Near-silence. Even the wind seems to hold its breath.
- Owl calls (intermittent, haunting)
- Water sounds intensify (the lake seems louder when everything else is quiet)
- The Fairy Fort's ambient shifts to something more charged (see Mythology section)
- Very occasionally: unexplained sounds (see Rare Events)

*The feeling: you are alone with the land. Or perhaps not alone.*

### Layer 3: Weather as Instrument

Weather doesn't just add sounds — it *transforms the entire mix*.

**Rain approaching:** Before rain arrives, there should be a transitional phase:
- Wind picks up gradually over 30–60 seconds
- Birdsong cuts abruptly (birds go quiet before rain — this is real)
- A distant rumble (if storm)
- Then the first drops — sparse, individual impacts — building to steady rain

**Steady rain:** A beautiful ambient layer. Rain on different surfaces sounds different:
- Rain on thatch (pub, cottages) — soft, muffled drumming
- Rain on stone (church) — sharper, more percussive
- Rain on water (Lough Ree) — a shimmering, hissing texture
- Rain on leaves (Fairy Fort hawthorn) — a gentle patter with drips

**Rain also masks other sounds** — the dampening system in ADR-015. In heavy rain, you can't hear the pub music from the Crossroads anymore. The world shrinks to your immediate surroundings. This is atmospherically powerful.

**Fog:** The most unsettling weather. Fog absorbs sound. Everything becomes:
- Muffled, deadened
- Distances distort — a sound that should be far away suddenly seems close
- Your own footsteps seem louder
- The bog in fog should be genuinely eerie — near-silence, with occasional sounds that seem to come from nowhere

**Storm:** Dramatic, immersive, slightly frightening:
- Wind as the dominant force — gusting, not steady
- Thunder (distant first, then closer, then receding)
- Rain hammering
- Tree sounds — branches cracking, leaves thrashing
- Almost all other sounds obliterated
- A storm at the Fairy Fort at midnight should feel genuinely unsafe

**Clear winter night:** A special case — the coldest, most crystalline soundscape:
- Sound carries further on cold, still air (invert the usual dampening — +10% range?)
- Stars seem audible (this is nonsense but the feeling is real — absolute clarity)
- Frost creak (contraction of wood, ice forming on puddles)

---

## Part 3: The Otherworld Has a Soundtrack

Irish mythology isn't decorative in Parish — it's structural. The locations, NPCs, and calendar are already threaded with supernatural hooks. Sound is how we make the player *believe* in the Otherworld without ever showing it directly.

### The Principle: You Never See It. You *Almost* Hear It.

The most frightening sounds are the ones you're not sure you heard. The mythology layer should operate at the threshold of perception — sounds that might be the wind, might be something else. The player should find themselves turning up their volume, leaning in, wondering.

### The Fairy Fort at Night

The Fairy Fort (Location 11) is the epicenter of the supernatural soundscape. During the day, it's just an old ring fort with wind through hawthorn. At night, the sound design should make the player's skin prickle:

**Baseline (any night):**
- The hawthorn wind shifts pitch — it's still wind, but the harmonics are wrong, as if the branches are vibrating at frequencies they shouldn't
- Gaps of total silence — 5–10 seconds where even the wind stops. These are more unsettling than any sound.
- Very faint, almost subliminal low-frequency hum (below 80Hz). Not a drone — more like the earth breathing.

**Samhain approach (late October):**
As the calendar approaches November 1st, the Fort's soundscape intensifies gradually over the preceding week:
- The silence gaps get longer and more frequent
- A faint sound like distant music — is it? You can't quite make it out. It's at the very edge of hearing. (This is *ceol sí* — fairy music — rendered as barely-perceptible melodic fragments, as if carried on a wind that's blowing from somewhere that isn't on the map.)
- On Samhain Eve itself: the music becomes almost audible. Almost. A pentatonic fragment on something that sounds like wire-strung harp — the dead instrument of the dead aristocracy, played by the dead. If you crank your speakers, you might catch a few notes. Maybe.

**Bealtaine (May 1):**
Bealtaine is the other thin-veil festival. The Fort's night soundscape should shift to something less ominous and more *seductive*:
- The music fragments are warmer, more like distant dancing
- Laughter? Was that laughter? It was probably a fox.
- Firelight sounds — crackling, though there's no fire (the bonfire that isn't there)

### The Banshee

The *bean sí* foretells death. If Parish ever implements NPC death events, the banshee should be heard 1–3 in-game days before. Design ideas:

- **Not a scream.** That's the Hollywood version. The real folklore describes a keening — a *caoineadh* — a women's lament, wavering between song and wail. Think of it as sean-nós singing from the wrong side of reality.
- **Distance and direction:** The banshee is heard from the direction of the home of the person who will die. It propagates Far, but with heavy low-pass filtering — you hear a mournful, muffled wailing that you can't quite locate.
- **NPC reactions:** The morning after a banshee is heard, NPCs should reference it in conversation. "Did you hear that last night?" "I heard nothing. I heard *nothing*." The denial is the tell.
- **Rarity:** This should happen maybe 1–2 times per in-game year at most. It must remain terrifying.

### The Púca at the Crossroads

The *púca* is a shapeshifter associated with crossroads and nighttime. Sound ideas:

- After midnight at the Crossroads: very occasionally, the sound of hoofbeats approaching — from all four roads simultaneously. They build, come close, and then... nothing. Silence.
- An animal sound that's not quite right — a horse's whinny that bends into something almost like speech. Just the wind, surely.
- The púca is traditionally associated with November — after Samhain, it's the púca's month. Increase frequency of these sounds in November.

### The Each-Uisce (Water Horse) at Lough Ree

Tommy O'Brien claims to have seen the *each-uisce* as a boy. The lake should occasionally remind the player of why:

- On still nights, a sound from deep in the lake — not a splash, more like something large displacing water very slowly. A swell, a ripple heard from shore.
- Very rarely: a sound like a horse breathing, but wet. From the water. At night.
- Fog at Lough Ree should be particularly unsettling — the water sounds seem closer, as if the lake is creeping toward you.

### Bog Voices

"People say you can hear voices in the wind here on certain nights." The Bog Road (Location 12) should deliver on this promise:

- On windy nights, the bog wind should occasionally produce sounds that resemble human speech — not words, but the *cadence* of speech. Rising and falling patterns, pauses, emphasis. The wind speaking a language you almost understand.
- This could be achieved by layering very quiet, heavily processed vocal samples under the wind sound, so processed they're unrecognizable as voices, but the pattern remains.
- In fog: the voices seem closer. More insistent.
- Near Samhain: they might actually form fragments of Irish words. *Tá sé ag teacht.* (He is coming.) Buried so deep in the mix that most players will never consciously hear it.

### The Church as Sanctuary

Sound design should reinforce St. Brigid's Church as the *opposite* of the supernatural locations:

- Inside the church at night: perfect, deep silence with stone reverb. The safest-sounding place in the parish.
- No supernatural sounds propagate *into* the church, regardless of distance calculations. The church is acoustically sacred.
- The Angelus bell doesn't just mark time — it's an auditory ward. After the evening Angelus, the supernatural sounds are suppressed for 30 in-game minutes, as if the bell pushed something back.

### Design Rule: Never Confirm

The mythology sounds should *never* be unambiguous. There should always be a naturalistic explanation available. The hoofbeats could be a loose horse. The lakewater sound could be a large fish. The bog voices are just wind. The fairy music is tinnitus from the pub. The player's imagination is more powerful than any sound file we could make. Our job is to give their imagination fuel, not answers.

---

## Part 4: The Music of Kilteevan

### Composed/Curated Music — The Emotional Layer

This is the Minecraft-equivalent layer: composed pieces that play infrequently, are emotionally resonant, and feel like they belong to the world rather than to the player.

### The Palette: What Instruments Can We Use?

Strictly period-correct for 1820s Connacht:

| Instrument | Availability | Character | Best Used For |
|---|---|---|---|
| **Solo fiddle** | Common — the parish's instrument | Warm, versatile, intimate to lively | Everything: reels, airs, laments |
| **Uilleann pipes** | Rare — a visiting piper is an event | Reedy, rich, haunting drone + melody | Special occasions, travelling musician events |
| **Wooden flute** | Present but not dominant yet | Breathy, warm, slightly rough | Quiet moments, solo contemplation |
| **Tin whistle** | Common, humble | Bright, simple, pure | Children, morning, lightness |
| **Wire-strung harp** | Extinct in living practice by 1820 | Metallic, resonant, otherworldly | ONLY for supernatural/mythological sounds |
| **Sean-nós voice** | Common — the primary vocal tradition | Unaccompanied, ornamented, deeply emotional | Night, pub, wakes, contemplation |
| **Keening voice** | Specific to death/wakes | Wavering between song and wail | Banshee, wakes, grief |
| **Jaw harp (trumpa)** | Common, humble | Twangy, rhythmic, buzzy | Whimsical moments, children |

**Critical rule:** The wire-strung harp is *never* used for human music in the game. It's extinct. When the player hears harp, it's the sídhe. This makes the instrument itself a mythological signal.

### Diegetic vs. Non-Diegetic Music

Parish should use **both**, but the player should never be quite sure which they're hearing.

**Diegetic (music that exists in the world):**
- Fiddle music from Darcy's Pub (propagates Near, louder at night)
- Crossroads dance music on summer evenings (fiddle + stamping feet)
- Hymns from the church on Sunday mornings (propagates Medium)
- Sean-nós singing from the pub at night (solo voice, occasional)
- A shepherd's whistle on the hillside (very faint, daytime)
- A mother singing to a child in the Village (muffled, through walls, evening)

**Non-diegetic (music that belongs to the experience, not the world):**
- Slow airs that drift in during quiet moments — a solo fiddle playing a lament while you walk the Bog Road at dusk
- A gentle flute melody at dawn, as if the sunrise itself has a voice
- Piano (yes, anachronistic — but hear me out)

### The Case for a Subtle Piano

Minecraft proved that sparse piano is the most emotionally versatile instrument in game soundtracks. It doesn't belong in 1820s Ireland. But non-diegetic music doesn't have to. A single, reverb-heavy piano note behind a slow fiddle air — not as an Irish instrument, but as an emotional underlayer — could be devastating. Think of it as the *player's* instrument, not the world's. The world has fiddles and pipes. The player, looking in from outside time, has a piano.

Use sparingly. Perhaps only 2–3 pieces in the entire soundtrack use piano, and they play only at the most emotionally significant moments (first time the player enters the parish, Samhain night, a death event).

### Music Triggers — When Should Music Play?

Following the Minecraft model of infrequent, semi-random triggering:

**Time-based cooldown:** After any music piece finishes, no new non-diegetic music for at least 12–20 real-world minutes. Diegetic music (pub, crossroads) is exempt — it plays when its conditions are met.

**Contextual triggers (increase probability, don't guarantee):**

| Trigger | Music Pool | Probability Boost |
|---|---|---|
| Dawn transition | Gentle morning pieces (flute, whistle) | +30% |
| Dusk transition | Slow airs (fiddle) | +40% |
| First visit to a new location | Location-themed piece | +50% |
| Walking the Bog Road alone at night | Lonely, contemplative piece | +25% |
| Returning to the pub after time away | Warm fiddle reel (brief) | +30% |
| Samhain night | Wire-strung harp fragment (the only time) | +60% |
| Clear winter night | Crystalline, sparse piano + fiddle | +20% |
| After a significant NPC conversation | Reflective piece | +15% |

**What music should feel like:**
- Morning: hope, dew, possibility. Tin whistle, light flute.
- Afternoon: contentment, warmth, the long golden hours. Gentle fiddle.
- Dusk: melancholy, beauty, transition. Slow air on fiddle, or pipes if available.
- Night at the pub: community, warmth, life. Lively reel, jig, laughter.
- Night alone: contemplation, wonder, slight unease. Solo fiddle, very spare.
- Midnight: awe, fear, beauty. Near-silence. If music plays at all, it should be the quietest, most haunting thing in the game.
- Festivals: energy, joy, communal belonging. Full sets: fiddle + feet + voices.

### The Travelling Piper

When the travelling piper NPC event fires (a piper arriving in the parish — perhaps once every few in-game months), the entire musical palette shifts temporarily:

- Uilleann pipes become available as a sound source
- The pub soundscape transforms — pipe music draws a crowd, the energy is different
- NPC conversation should reference the piper's arrival
- The piper stays for 2–3 in-game days, then moves on
- After the piper leaves, there's a noticeable absence — the parish feels quieter

This creates a memorable event — the player will remember "the time the piper came" the way a real person in 1820 would.

---

## Part 5: Rare Events, Calendar Sounds, and Whimsical Moments

### The Rare Event System

Some sounds should be genuinely rare — things that happen once every few in-game months or even years. These create stories. "I was walking past the Fairy Fort at midnight and I heard *harp music*." The rarity is the point. If it happened every night, it would be wallpaper. If it happens once in 20 hours of play, it's a *moment*.

**Proposed rare sound events:**

| Event | Location | Time | Season | Frequency | Description |
|---|---|---|---|---|---|
| **Fairy music** | Fairy Fort | Night/Midnight | Samhain week | ~Once/year | Wire-strung harp, barely audible. The dead instrument played by the dead. |
| **Banshee keen** | Directional (from NPC's home) | Night | Any | Tied to NPC death events | Distant keening. Propagates Far with heavy filtering. |
| **Púca hoofbeats** | Crossroads | Midnight | November | ~2x/month in November | Hoofbeats approaching from all four roads, then vanishing. |
| **Each-uisce** | Lough Ree Shore | Night (fog) | Any | ~Once every 3 months | Something large moving in the water. A wet breathing. |
| **Bog voices** | Bog Road | Night (wind) | Any | ~Once/month | Wind patterns that sound like speech cadence. |
| **Sluagh Sí (wild hunt)** | Overhead (any outdoor location) | Midnight | Winter | ~Once/year | Distant horns and hooves *above* — the host of the dead riding the sky. Wind rises and falls with them. |
| **The lone piper** | Distant (any location) | Dusk | Autumn | ~Once every 2 months | Faint pipe music from no identifiable direction. No piper is in the parish. |
| **Child laughing at the Fort** | Fairy Fort | Afternoon | Summer | ~Once every 2 months | A child's laugh from inside the rath. There are no children there. |
| **Bell that tolls itself** | Church | 3:00 AM | Any | ~Once/year | A single bell toll with no one to ring it. The dead hour. |

### The Calendar of Sound

The four festivals should be major sonic events — the player should *hear* the calendar turning.

**Imbolc (February 1) — First Day of Spring**
- Dawn on Imbolc: the first skylark of the year (a distinctive, soaring song absent since autumn)
- Morning: lambs bleating from the farms (new sound not heard in winter)
- Snowmelt sounds: dripping, trickling, the bog beginning to thaw
- A brightness to the ambient — slightly higher-pitched wind, more warmth in the texture
- At St. Brigid's Church: special prayers (hymn sound at unusual times)

**Bealtaine (May 1) — First Day of Summer**
- Eve of Bealtaine: bonfire sounds from the Village (crackling, sparks, distant laughter)
- Dawn: an explosion of birdsong — the fullest dawn chorus of the year
- Cattle sounds as they're driven to summer pasture (a tradition — moving livestock on Bealtaine)
- The crossroads dance season officially opens — first dance music of the year
- Night: the Fairy Fort is more active than any night except Samhain. The veil thins both ways in May.

**Lughnasa (August 1) — First Day of Autumn**
- Harvest sounds: scythes on grass, sheaf-gathering, threshing
- Pattern day soundscape if the parish celebrates (music, dancing, crowds, fighting)
- The last, fullest crossroads dances of the summer
- Late afternoon: thunder in the distance (Lughnasa traditionally associated with storms — Lugh wrestling the old gods for the harvest)
- Hurling match atmosphere (cheering, stick on leather, crowd energy)

**Samhain (October 31 – November 1) — First Day of Winter**
- The sonic crown jewel of the year
- Eve: bonfires, but quieter than Bealtaine — more solemn, more watchful
- Prayers at the church (longer, more urgent)
- Night: the entire parish soundscape shifts. All ambient sounds drop by ~15%. The world quiets as if listening.
- Midnight: the Fairy Fort reaches maximum intensity. If fairy music is ever going to be audible, it's now.
- Post-midnight: silence. Deep, absolute silence. Even the wind stops. Then, slowly, the ordinary sounds of a November night return.
- The next morning: winter has arrived. The soundscape is permanently colder, sparser, until Imbolc.

### Sunday Soundscape

Sunday is acoustically distinct from every other day:

- **Morning:** Extended church bell peal (different pattern from the daily Angelus). Propagates everywhere — the entire parish hears it.
- **During Mass (10:00–11:00):** Hymn singing from the church (propagates Medium). The rest of the parish is quiet — most people are at Mass.
- **After Mass (11:00–13:00):** Social buzz — conversation sounds from the Village and Crossroads as people linger. This is the most social sound of the week.
- **Afternoon:** Hurling on Sunday afternoons (when weather allows). Crowd sounds, the clack of hurleys, cheering.
- **Evening:** Pub is livelier than weekday evenings. More voices, more music.

### Market Day / Fair Day

When market or fair events occur (perhaps monthly):

- Connolly's Shop area: bustling, haggling voices, cart arrivals, livestock sounds
- Crossroads: crowds passing, greetings called across the road
- Arriving merchants: new voices, accents (suggesting people from outside the parish)
- Card games: the muffled sound of gambling from somewhere (coins, voices, occasional groans or cheers)

### Whimsical Microdetails

Small, delightful touches that reward attentive listening:

- **The pub cat:** A purring sound, very low, very quiet, at Darcy's Pub in the evening. You'd only notice if you were listening closely.
- **Niamh's humming:** Near Hodson Bay in the morning, a young woman humming a tune (Niamh Darcy watches the boats). Different tunes on different days.
- **Tommy's walking stick:** A rhythmic tap-tap-tap on the path when Tommy O'Brien is at a location. His cane on stone.
- **Fr. Tierney's breviary:** Pages turning inside the church at odd hours. The priest is always reading.
- **The hedge school:** Faint children's voices reciting Irish declensions, or Aoife Brennan's voice teaching. From outside, it sounds like a beehive — a low, collective murmur.
- **Rain on the letter office window:** A specific, intimate sound — rain on glass, distinct from rain on thatch or stone.
- **Mick Flanagan's boots:** The retired constable still walks with military regularity. His footsteps have a distinctive, measured cadence.
- **Siobhan Murphy's spinning wheel:** At Murphy's Farm on winter evenings, if you're at the farmhouse, a rhythmic whir-thump of a spinning wheel.

### Distance and Propagation: Hearing the World Breathe

The graph-based propagation system in ADR-015 is powerful. Here are creative uses:

**The parish heartbeat:** The Angelus bell at dawn, midday, and dusk should be audible *everywhere*. This is the only sound with parish-wide propagation. It's the heartbeat — the thing that binds all 15 locations into one community. The player should come to rely on it for time orientation.

**The pub as beacon:** On cold winter nights, the faint sound of fiddle music from Darcy's Pub reaching the Crossroads or the Village is a lure — warmth, company, life. You *hear* that there's somewhere to go.

**Farm animals as alarm clock:** Murphy's rooster propagates Near. If the player is at the Crossroads (1 hop from Murphy's Farm), they hear the rooster at dawn, slightly quieter. If they're at the Bog Road (2+ hops), they don't. The rooster tells you where you are relative to the farm.

**The donkey as comic relief:** O'Brien's donkey propagates Medium and brays at unpredictable times. It's the one sound that cuts through the atmosphere — a ridiculous, honking bray that can be heard from two locations away. It should make the player smile. Every parish has that one donkey.

**Sound bleeding through weather:** In a storm, the only propagated sound that survives should be the church bell. Everything else is swallowed. The bell ringing through a thunderstorm is powerful — an assertion of human order against chaos.

**Indoor vs. outdoor:** Walking *into* the pub from a rainy night should be an audio event: the rain suddenly muffled (×0.4 as per ADR-015), replaced by warmth, crackling fire, conversation. Walking *out* again: the cold, the rain, the wind. This contrast — the door as an audio threshold — is one of the most immersive things the system can do.

---

## Part 6: Composition and Sourcing Strategy

### Original Composition vs. Found Sound

Ideally, Parish would have a small number of **original composed pieces** supplemented by **royalty-free ambient sound** (see [ambient-sound-sources.md](../research/ambient-sound-sources.md)).

**What should be composed:**
- 5–8 short (2–4 minute) non-diegetic pieces for the Minecraft-style emotional layer
- 2–3 slow airs on fiddle (real fiddle, not synthesized)
- 1 flute piece for dawn
- 1 whistle piece for lighter moments
- 1 fiddle + subtle piano piece for major emotional moments
- 1 wire-strung harp fragment for the sídhe (can be very short — 30 seconds, looped and processed)
- 1 keening/banshee vocal fragment

**What can be sourced:**
- All environmental ambience (wind, rain, water, thunder, animals, birds)
- Pub atmosphere (crowd murmur, glasses, fire)
- Church bells (many excellent CC0 recordings exist)
- Farm animals (rooster, cattle, sheep, dog, donkey, hens)
- Footsteps on various surfaces

### Procedural/Generated Audio Ideas

Some sounds could be generated rather than sampled, giving infinite variation:

- **Wind:** Pink noise + bandpass filter, modulated by weather intensity and season. Never the same twice.
- **Rain:** Layered procedural drops with randomized timing and pitch. Density controlled by weather state.
- **Fire/hearth:** Brown noise + crackle samples triggered at random intervals.
- **Insect hum (summer):** Sine wave cluster with slow frequency wobble. Fades with season.
- **Church bell:** Physical modeling synthesis could produce bells that ring naturally with each toll being slightly different (real bells vary with temperature and force).

### The Seasonal Shift

The overall density and character of the soundscape should follow a year-long arc:

```
         Imbolc        Bealtaine       Lughnasa        Samhain
           │               │               │               │
Winter ────┼── Spring ─────┼── Summer ─────┼── Autumn ─────┼── Winter
           │               │               │               │
Sound:   sparse          building        fullest        thinning
         wind-dominant   birds arrive    insect hum     birds leave
         long silence    dawn chorus     evening dance  fog, rain
         hearth fire     lamb bleating   harvest sounds silence deepens
         storytelling    bonfire         hurling         supernatural peak
```

This arc mirrors the Factorio concept of density reflecting state — but here the state is the living, breathing year.

---

## Part 7: Summary of Guiding Principles

1. **Silence is the most powerful sound.** Use it deliberately. The absence of sound at the Fairy Fort at midnight is more unsettling than any effect.

2. **Music is a gift, not wallpaper.** Follow the Minecraft model: long cooldowns, semi-random triggers, emotional resonance. Every piece should feel like an event.

3. **Sound is information.** The player should learn to read the soundscape — to know it's dawn from the rooster, dusk from the Angelus bell, Sunday from the long peal, summer from the insect hum.

4. **The world sounds like itself, not like a game.** Period instruments only for diegetic music. No orchestral swells. No synth pads. The non-diegetic layer uses the same acoustic palette, just removed from a specific source.

5. **Mythology lives in ambiguity.** Supernatural sounds are never confirmed. There's always a rational explanation. The player's imagination does the heavy lifting.

6. **Distance creates depth.** Hearing faint pub music from two locations away, or a bell tolling through a storm, makes the world feel three-dimensional even in a text game.

7. **Weather transforms everything.** Rain, fog, and storm don't just add layers — they reshape the entire mix. A location in fog sounds like a different place than the same location in sunshine.

8. **The year has a shape.** Sound density follows the agricultural calendar: sparse winter, building spring, full summer, thinning autumn. The player should feel time passing in the soundscape.

9. **Reward attentive listeners.** The purring cat, the turning pages, Tommy's walking stick, Niamh's humming — tiny details that most players will never consciously notice, but that make the world feel alive for those who listen.

10. **The harp belongs to the dead.** Wire-strung harp = the Otherworld, always. This single rule makes every harp note meaningful.
