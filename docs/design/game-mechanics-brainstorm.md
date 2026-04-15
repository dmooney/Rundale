# Game Mechanics Brainstorm

> Back to [Documentation Index](../index.md) | [Game Ideas Brainstorm](game-ideas-brainstorm.md)

Classic game mechanics that could enrich the Parish / Rundale experience. Each entry notes the design intent, how it fits the Irish rural setting, and what already exists to build on.

See [game-ideas-brainstorm.md](game-ideas-brainstorm.md) for narrative and social system ideas.

---

## What Already Exists (do not re-implement)

| Mechanic | Location |
|---|---|
| Time / seasons / festivals | `parish-types/src/time.rs` |
| Weather (7-state machine) | `parish-world/src/weather.rs` |
| Movement / pathfinding | `parish-world/src/movement.rs` |
| NPC schedules, memory, moods | `parish-npc/src/` |
| NPC relationships | `parish-npc/src/types.rs` |
| Conversation history | `parish-types/src/conversation.rs` |
| Event bus | `parish-types/src/events.rs` |
| NPC illness flag (`is_ill`) | `parish-npc/src/data.rs` |
| Save / load / branch | `parish-core/src/ipc/commands.rs` |

---

## 1. Player Vitals

### 1a. Health
- **What:** HP-like value (0–100). Reaches 0 → game-over (or serious penalty).
- **Sources of damage:** starvation, dehydration, exposure (cold + wet), illness, injury.
- **Sources of recovery:** eating, sleeping, resting indoors, receiving care from an NPC with healing knowledge.
- **Rundale flavour:** A hedge-school doctor or the parish *bean feasa* (wise woman) can tend wounds.

### 1b. Hunger
- **What:** 0 (starving) → 100 (sated). Drains over real time at a rate tied to activity.
- **Effects:** Below 30 → fatigue accumulates faster. Below 10 → health drains.
- **Food items:** bread, potatoes, oat porridge, salted fish, game, butter — each restores a different amount.
- **Rundale flavour:** The potato crop is the primary calorie source; famine years raise stakes dramatically.

### 1c. Thirst
- **What:** 0–100. Drains faster than hunger; water sources at wells, streams, or taverns.
- **Effects:** Below 20 → confusion penalty (NPC name-matching fuzzier, intent-parsing harder).
- **Rundale flavour:** Clean well water vs. brackish ditch water (illness risk).

### 1d. Fatigue / Rest
- **What:** 0 (exhausted) → 100 (well-rested). Falls with travel and activity; rises only by sleeping.
- **Effects:** Below 20 → movement speed halved; below 10 → cannot travel.
- **Sleep system** is its own section (§ 2).

### 1e. Warmth
- **What:** Comfort rating affected by weather, clothing, and shelter.
- **Effects:** Prolonged cold/wet → health drain; reaching a fire or indoors recovers warmth quickly.
- **Rundale flavour:** Turf fires are the primary heat source; cutting and storing turf is seasonal labour.

---

## 2. Sleep System

- **Trigger:** Player types something like *"go to sleep"*, *"lie down"*, *"rest"* at an eligible location.
- **Eligible locations:**
  - Player's own home (full rest bonus)
  - An NPC's home if invited or for a fee (medium bonus)
  - Outdoors / under a ditch (minimal rest; cold/wet penalties still apply)
- **Mechanics:**
  - Advances the game clock to the next reasonable wake time (configurable, e.g. ~6–8 hours).
  - Restores fatigue fully; partial hunger/thirst drain while sleeping.
  - Applies warmth recovery or penalties depending on location.
  - Can be interrupted by events (storm worsens, NPC knocks, dawn bell).
- **Cuaird connection:** Evening visiting (*cuaird*) is already flagged in schedules. Sleeping after a late cuaird visit should be narratively natural.

---

## 3. Inventory System

- **Container:** `PlayerInventory` — a `Vec<ItemStack>` with a configurable `max_weight` (encumbrance).
- **ItemStack:** `{ item_id: ItemId, quantity: u32 }` — stackable commodities.
- **Weight:** Each item has a `weight_kg: f64`; total carried weight affects travel speed.
- **Commands:** `/inventory` (list), *"pick up [item]"*, *"drop [item]"*, *"give [item] to [NPC]"*, *"eat [food]"*.
- **Persistence:** Serialised alongside WorldState in save files.

---

## 4. Item System

### Item Types

| Category | Examples |
|---|---|
| Food | Potato, bread loaf, salted herring, oat cake, butter pat, cabbage, turnip |
| Drink | Water (flask), milk (jug), poitín (jar), ale (mug) |
| Fuel | Turf sod, bundle of sticks, peat brick |
| Tools | Spade, scythe, fishing line, net, knife |
| Clothing | Woollen cloak, brat, leather brogue, shawl |
| Trade goods | Wool bundle, linen yard, tallow candle, rope |
| Currency | Penny, shilling, crown (pre-decimal Irish) |
| Keys / Plot | Letter, sealed document, package for delivery |
| Light sources | Rush candle, tallow candle, oil lantern |

### Item Data Model

```rust
struct Item {
    id:           ItemId,
    name:         String,        // "potato"
    display_name: String,        // "a floury potato"
    description:  String,
    weight_kg:    f64,
    category:     ItemCategory,
    effects:      Vec<ItemEffect>,  // e.g. Eat → hunger +25, warmth +5
    value_pence:  u32,
}
```

---

## 5. Economy & Trade

- **Currency:** Pre-decimal Irish coinage (pence, shilling, half-crown, crown).
- **Market days:** Already modelled as `DayType::MarketDay` in the time system. Vendors appear only then.
- **Shops / stalls:** Location-attached `Vendor { items_for_sale, buy_factor, sell_factor }`.
- **Bartering:** NPCs with high `Practical` intelligence (from the existing 6-dim intelligence model) can haggle.
- **Rent:** Regular deduction from player's purse — creates economic pressure, ties to landlord/agent NPC.
- **Credit:** Shopkeeper NPCs may extend a tab; debt affects relationship strength.

---

## 6. Skills System

Rather than XP bars, skills improve through use (implicit learning):

| Skill | Increases by… | Effect |
|---|---|---|
| Irish Language | speaking Irish with NPCs | unlock deeper dialogue, trust bonuses |
| Farming | tending crops, cutting turf | crop yield, reduced effort |
| Fishing | fishing actions | catch rate, fish species variety |
| Craft / Trade | making or selling goods | better trade prices |
| Social / Charm | successful conversations | NPC mood starts higher |
| Navigation | travelling new routes | travel time reduced |
| Healing | assisting the sick | can provide medical care |

Skills are `u8` (0–100) stored in `PlayerState`. Thresholds (25, 50, 75, 100) unlock new intent outcomes.

---

## 7. Reputation / Standing System

- **Parish Reputation:** A single `i16` (-500 → +500) representing how the community views the player.
  - Starts at 0 (stranger).
  - Rises by: helping NPCs, keeping promises, attending Mass/festivals.
  - Falls by: theft, breaking social norms, gossip spreads bad deeds (existing gossip network!).
- **Faction reputations:** Separate scores for Landlord, Church, Tenants, Traders.
- **Effect on play:** Low reputation → NPCs refuse dialogue, raise prices, refuse shelter. High reputation → discounts, invitations, plot access.
- **Gossip integration:** The existing `gossip_network` already propagates information — wiring reputation changes there is natural.

---

## 8. Status Effects

Short-duration modifiers on `PlayerState`:

| Effect | Cause | Duration | Impact |
|---|---|---|---|
| Soaked | Heavy rain outdoors | Until near fire | Warmth drain, health drain |
| Chilled | Low warmth extended | Gradual | Health drain |
| Ill | Infected water / NPC spread | Days | All vitals drain faster |
| Drunk | Too much poitín/ale | Hours | Intent parsing fuzzy, social penalties |
| Well-fed | Just ate | 1 hour | Fatigue drains slower |
| Rested | Just woke | 2 hours | Travel speed bonus |
| Grieving | Plot trigger | Variable | Social options change |

---

## 9. Lighting & Visibility

- **Candle / lantern mechanic:** At `TimeOfDay::Night` or `TimeOfDay::Midnight` outdoors, player needs a light source.
- **Without light:** Location descriptions are sparse ("pitch dark"), movement to unfamiliar places is risky.
- **Light items:** Rush candle (cheap, short), tallow candle, oil lantern (longest).
- **Fuel depletion:** Lanterns track `fuel_remaining` — adds resource management.
- **Rundale flavour:** Candle-making from tallow or rush-dipping is a seasonal household task.

---

## 10. Shelter & Housing

- **Player home:** A designated `LocationId` owned or rented by the player.
- **Homeless state:** If evicted (rent unpaid), player must find lodging each night.
- **Lodging options:** Travellers' hostel (*teach an bhóthair*), sleeping at a sympathetic NPC's home.
- **Home upgrades:** Thatch repair, adding a bedroom, building a byre — each costs materials + labour + time.
- **Eviction risk:** Ties back to rent payment in the Economy system.

---

## 11. Seasonal Labour & Agriculture

Ties time, skills, and inventory together:

| Season | Activity | Mechanic |
|---|---|---|
| Spring | Plant potatoes, sow oats | Requires spade + seed; starts multi-day task |
| Summer | Hay cutting, turf cutting | Stamina drain; yield depends on skill |
| Autumn | Harvest crops | Time-limited; failure → food shortage |
| Winter | Threshing, spinning, repairs | Indoor tasks; sociability (cuaird) high |

- Tasks are **multi-day processes**: start a task, advance time, return to complete it.
- Crop failure (bad weather + poor skill) can trigger famine pressure.

---

## 12. Disease & Illness

Building on the existing `is_ill` NPC flag:

- **Illness types:** Common cold, fever, the flux, more severe epidemic (plot).
- **Transmission:** Spending time with ill NPCs raises player illness chance.
- **Player illness:** New `PlayerStatus::Ill { severity, days_remaining }`.
- **Treatment:** Rest + warmth + specific items (herbs, poitín as medicine); visiting the *bean feasa*.
- **NPC epidemic spread:** Tier-4 engine already exists for NPC illness ticks.

---

## 13. Transport & Mounts

Building on the existing `TransportMode` struct:

| Mode | Speed (m/s) | Requires | Notes |
|---|---|---|---|
| On foot | 1.25 | — | Default |
| Donkey | 1.5 | Own / hire donkey | Can carry heavy loads |
| Horse | 2.5 | Own / hire horse | Costs upkeep |
| Cart | 1.2 | Horse + cart | Carry bulk goods; slower |
| Currach | varies | Coastal routes only | Needed to cross to islands |

- Animals need feeding (hunger-like mechanic for animals).
- Hiring costs currency; owning costs ongoing feed + stabling.

---

## 14. Quest / Task System

- **Task types:** Errand (deliver X to Y), Fetch (bring item), Timed (before market day), Relational (improve standing with NPC).
- **Task struct:**
  ```rust
  struct Task {
      id:          TaskId,
      giver:       NpcId,
      description: String,
      objective:   TaskObjective,
      reward:      TaskReward,
      deadline:    Option<GameTimestamp>,
      state:       TaskState,  // Active, Complete, Failed
  }
  ```
- **Discovery:** Tasks surface through NPC dialogue naturally — NPC says "Would you ever carry this to Máire?" → intent-parsed as a task offer.
- **Failure:** Missing a deadline → relationship damage with giver.

---

## 15. Failure States & Difficulty

- **Death:** Health reaches 0 → "You died of exposure/starvation/illness" — loads last save.
- **Eviction:** Rent unpaid too long → player loses home.
- **Social exile:** Reputation below threshold → key NPCs refuse all interaction.
- **Difficulty modes:**
  - Story (vitals drain slowly, no death)
  - Normal (standard drain rates)
  - Survival (accelerated drain, permadeath option)

---

## 16. Sound & Atmosphere Hooks (UI layer)

Not strictly mechanics, but complement vitals — see also [ambient-sound.md](ambient-sound.md):

- Heartbeat effect when health is low.
- Ambient rain audio synced with `Weather::HeavyRain`.
- Bell at dawn/dusk matching `TimeOfDay` transitions.

---

## Priority / Implementation Order

| Priority | Mechanic | Rationale |
|---|---|---|
| 1 | Player vitals (health, hunger, fatigue) | Highest gameplay impact; everything else builds on this |
| 2 | Sleep system | Immediate payoff; uses vitals + time |
| 3 | Inventory + basic items (food) | Gives hunger system content |
| 4 | Economy (currency + rent) | Creates stakes and pressure |
| 5 | Status effects (wet, cold, ill) | Pays off the existing weather system |
| 6 | Reputation | Gossip network already built |
| 7 | Skills | Polish layer |
| 8 | Agriculture / seasonal tasks | Deep sim layer |
| 9 | Quests | Narrative glue |
| 10 | Transport / mounts | Quality-of-life + depth |

---

*Document created: 2026-04-11*
