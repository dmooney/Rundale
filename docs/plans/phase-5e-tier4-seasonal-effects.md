# Plan: Phase 5E — Tier 4 Rules Engine & Seasonal Effects

> Parent: [Phase 5](phase-5-full-lod-scale.md) | [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Done** (runtime wiring landed)
>
> **Depends on:** Phase 5A (event bus), Phase 5B (weather for weather-driven rules), Phase 5D (tier assignment split)
> **Depended on by:** None (terminal sub-phase)

## Runtime wiring note

`tick_tier4` is dispatched inline inside the background tick scope (not via `spawn_blocking` as the original plan suggested). Measured CPU cost is sub-millisecond for typical NPC counts (~30 NPCs), so `spawn_blocking` would add complexity without benefit. See `crates/parish-tauri/src/lib.rs`, `crates/parish-server/src/lib.rs`, and `crates/parish-cli/src/headless.rs` for the call sites.

## Goal

Implement the pure CPU Tier 4 rules engine for far-away NPCs (no LLM), seasonal schedule overrides, and festival event hooks. This completes the four-tier cognitive LOD system.

## Tasks

### 1. Tier 4 Rules Engine (`crates/parish-core/src/npc/tier4.rs` — new file)

```rust
use rand::Rng;
use crate::npc::{Npc, NpcId};
use crate::world::WorldState;
use crate::world::time::Season;

/// A life event produced by the Tier 4 rules engine.
#[derive(Debug, Clone)]
pub enum Tier4Event {
    /// A child is born to two NPCs.
    Birth { parent_ids: (NpcId, NpcId) },
    /// An NPC has died (natural causes).
    Death { npc_id: NpcId },
    /// A trade was completed between two NPCs.
    TradeCompleted { buyer: NpcId, seller: NpcId },
    /// An NPC's schedule changed due to the season.
    SeasonalShift { npc_id: NpcId, new_schedule_desc: String },
    /// An NPC fell ill.
    Illness { npc_id: NpcId },
    /// An NPC recovered from illness.
    Recovery { npc_id: NpcId },
}

/// Runs a Tier 4 tick: deterministic/random state transitions with no LLM.
///
/// Called once per in-game season (~30-45 real minutes).
/// Must run on `tokio::task::spawn_blocking` to avoid blocking the async runtime.
pub fn tick_tier4(
    npcs: &mut [&mut Npc],
    world: &WorldState,
    season: Season,
    rng: &mut impl Rng,
) -> Vec<Tier4Event>
```

**Rules:**

| Event | Probability | Conditions |
|-------|-------------|------------|
| Illness | 2% per NPC per season | Any NPC |
| Recovery | 80% per ill NPC per season | NPC is currently ill |
| Death | 0.5% per NPC per year (÷4 per season) | Age > 60: 2% per season; age > 75: 5% |
| Birth | 5% per married couple per season | Both NPCs healthy, at least one age 18-45 |
| Trade | 10% per merchant NPC per season | NPC has occupation containing "shop" or "trade" |
| SeasonalShift | 100% for affected occupations | Farmers, teachers (see below) |

**Implementation**: Pure `match` + `rng.gen_range()`. No network calls, no async.

### 2. NPC health state

Add a health field to `Npc`:

```rust
pub struct Npc {
    // ... existing fields ...
    /// Whether the NPC is currently ill. Set by Tier 4 rules.
    pub is_ill: bool,
}
```

### 3. Seasonal schedule overrides (`crates/parish-core/src/npc/types.rs`)

```rust
/// Returns seasonal schedule overrides for an NPC based on their occupation.
pub fn seasonal_schedule_override(
    occupation: &str,
    season: Season,
    home: LocationId,
    workplace: LocationId,
) -> Option<DailySchedule>
```

| Occupation | Season | Override |
|------------|--------|---------|
| Farmer | Summer | Start at 5am (was 7am), end at 21:00 (was 18:00) |
| Farmer | Winter | Start at 8am, end at 16:00 |
| Teacher | Summer | No school — stay home all day |
| Publican | Winter | Open later (11am), close later (midnight) |

### 4. Festival event hooks

Modify `GameClock` or add a festival check function:

```rust
/// Checks if a festival falls within the given time range.
/// Returns the festival name if one starts during [from, to).
pub fn check_festival(from: DateTime<Utc>, to: DateTime<Utc>) -> Option<Festival>
```

Festivals (already defined in `time.rs`):

| Festival | Date | Effect |
|----------|------|--------|
| Imbolc | Feb 1 | Community gathering, spring anticipation |
| Bealtaine | May 1 | Celebration, bonfires, outdoor activity |
| Lughnasa | Aug 1 | Harvest fair, trading, games |
| Samhain | Nov 1 | Solemn mood, supernatural atmosphere |

When a festival is detected:

- Publish `WorldEvent::FestivalStarted { name }` via event bus.
- Inject festival context into NPC prompts for Tier 1/2: "It's {festival}. The community is {description}."
- Tier 4: boost relationship strength between NPCs at same location (+0.05).

### 5. Tier 4 tick scheduling in `NpcManager`

```rust
impl NpcManager {
    last_tier4_game_time: Option<DateTime<Utc>>,

    /// Tier 4 ticks once per in-game season.
    pub fn needs_tier4_tick(&self, current_game_time: DateTime<Utc>, season: Season) -> bool;

    pub fn record_tier4_tick(&mut self, time: DateTime<Utc>);

    /// Returns all NPCs assigned to Tier 4.
    pub fn tier4_npcs(&self) -> Vec<NpcId>;
}
```

### 6. Apply Tier 4 events

After `tick_tier4()` returns:

- `Illness`: set `npc.is_ill = true`, change mood to "unwell".
- `Recovery`: set `npc.is_ill = false`, restore mood.
- `Death`: remove NPC from manager, publish event (gossip fuel).
- `Birth`: create new NPC with blended traits from parents (future — for now, just publish event).
- `SeasonalShift`: apply schedule override.
- `TradeCompleted`: adjust relationship +0.1 between buyer/seller.
- Publish each as `WorldEvent::LifeEvent` for gossip propagation (Phase 5C).

### 7. Run on blocking thread

```rust
let events = tokio::task::spawn_blocking(move || {
    let mut rng = rand::thread_rng();
    tick_tier4(&mut tier4_npcs, &world_snapshot, season, &mut rng)
}).await?;
```

## Tests

| Test | What it verifies |
|------|------------------|
| `test_tier4_deterministic_with_seed` | Seeded RNG produces repeatable events |
| `test_tier4_illness_probability` | Over 10,000 runs, illness rate is ~2% |
| `test_tier4_death_age_scaling` | Elderly NPCs die at higher rate than young |
| `test_tier4_no_birth_if_no_couples` | No births without married couples |
| `test_tier4_seasonal_shift_farmer` | Farmer gets longer hours in summer |
| `test_tier4_seasonal_shift_teacher` | Teacher stays home in summer |
| `test_festival_detection` | check_festival finds Imbolc on Feb 1 |
| `test_festival_between_dates` | Festival detected when range spans the date |
| `test_festival_context_injection` | Festival name appears in NPC prompt context |
| `test_tier4_event_publishes_world_event` | Each Tier4Event generates a WorldEvent::LifeEvent |
| `test_tier4_runs_on_spawn_blocking` | Async test verifies non-blocking execution |

## Acceptance Criteria

- Tier 4 runs without any LLM calls
- Life events occur at statistically correct rates
- Seasonal schedule overrides apply correctly for each occupation
- Festivals fire at correct calendar dates and inject context
- Tier 4 events propagate via event bus for gossip and persistence
- No blocking of the async runtime (spawn_blocking)
- All tests passing
