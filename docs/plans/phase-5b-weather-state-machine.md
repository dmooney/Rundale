# Plan: Phase 5B — Weather State Machine

> Parent: [Phase 5](phase-5-full-lod-scale.md) | [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Complete** — *palette-tinting tasks below have since been reverted; weather no longer affects the UI palette.*
>
> **Depends on:** Phase 5A (event bus for `WeatherChanged` events)
> **Depended on by:** 5D (weather context in Tier 3 prompts), 5E (weather drives Tier 4 rules)

## Goal

Replace the static `Weather` enum with a dynamic state machine that transitions over time based on season and randomness. Weather changes publish events and affect NPC behavior and palette tinting.

## Tasks

### 1. Expand `Weather` enum (`crates/parish-core/src/world/mod.rs`)

Add two new variants to the existing `Weather` enum:

```rust
pub enum Weather {
    Clear,
    PartlyCloudy,  // NEW
    Overcast,
    LightRain,     // NEW (rename Rain → LightRain)
    HeavyRain,     // NEW
    Fog,
    Storm,
}
```

Update `Display` impl, palette tinting parameters in `palette.rs`, and all match arms.

### 2. `WeatherEngine` (`crates/parish-core/src/world/weather.rs` — new file)

```rust
use chrono::{DateTime, Utc};
use crate::world::Weather;
use crate::world::time::{GameClock, Season};

/// Seasonal transition probability matrix.
/// Each row = current state, each column = next state.
/// Probabilities vary by season.
pub struct WeatherEngine {
    /// Current weather state.
    current: Weather,
    /// Game time when the current state began.
    since: DateTime<Utc>,
    /// Minimum duration in game-hours before a transition is allowed.
    min_duration_hours: f64,
}

impl WeatherEngine {
    /// Creates a new engine starting in the given state.
    pub fn new(initial: Weather, start_time: DateTime<Utc>) -> Self;

    /// Returns the current weather.
    pub fn current(&self) -> Weather;

    /// Returns how long the current weather has persisted (game time).
    pub fn duration_hours(&self, now: DateTime<Utc>) -> f64;

    /// Ticks the weather engine. Returns `Some(new_weather)` if a
    /// transition occurred, `None` if the weather is unchanged.
    ///
    /// Called every game tick. Only evaluates transitions after
    /// `min_duration_hours` have elapsed.
    pub fn tick(
        &mut self,
        clock: &GameClock,
        season: Season,
        rng: &mut impl Rng,
    ) -> Option<Weather>;
}
```

**Transition probability design:**

| Season | Clear bias | Rain bias | Storm probability |
|--------|-----------|-----------|-------------------|
| Spring | 0.35 | 0.30 | 0.05 |
| Summer | 0.50 | 0.15 | 0.03 |
| Autumn | 0.20 | 0.40 | 0.08 |
| Winter | 0.15 | 0.45 | 0.10 |

- Minimum duration: **2 game-hours** before any transition attempt.
- Transition check: once per game-hour after minimum elapsed.
- Adjacent states more likely (Clear → PartlyCloudy → Overcast → LightRain → HeavyRain → Storm); no jumping from Clear to Storm directly.

### 3. Weather affects NPC schedules

Modify `Npc::desired_location()` or `NpcManager::tick_schedules()`:

- If weather is `LightRain`, `HeavyRain`, or `Storm`, and the NPC's scheduled location is outdoors (`!indoor`), override to their `home` location (or nearest indoor location if home is also outdoors).
- NPCs with `occupation: "farmer"` tolerate `LightRain` (no override).

### 4. Weather in Tier 2 prompts

Modify the Tier 2 prompt construction in `npc/ticks.rs`:

- Include current weather in the context: "The weather is {weather}."
- If raining or stormy, add: "People are commenting on the weather."

### 5. Palette tinting updates

Update `palette.rs` to handle the new `PartlyCloudy`, `LightRain`, and `HeavyRain` variants:

| Weather | RGB Multiplier | Desaturation | Brightness |
|---------|---------------|-------------|------------|
| PartlyCloudy | (0.97, 0.97, 0.98) | 8% | 96% |
| LightRain | (0.90, 0.92, 0.96) | 15% | 88% |
| HeavyRain | (0.85, 0.87, 0.93) | 25% | 80% |

### 6. Publish `WeatherChanged` events

When `WeatherEngine::tick()` returns `Some(new_weather)`:

- Update `WorldState.weather`
- Publish `WorldEvent::WeatherChanged { old, new }` via the event bus

### 7. Wire into game loop

- Add `WeatherEngine` to `WorldState`.
- Call `weather_engine.tick()` on each game loop iteration.
- The engine uses the `GameClock` for time tracking; no separate timer needed.

## Tests

| Test | What it verifies |
|------|------------------|
| `test_weather_engine_initial_state` | Engine starts with the given weather |
| `test_weather_no_transition_before_min_duration` | No transitions in the first 2 game-hours |
| `test_weather_transitions_after_min_duration` | Seeded RNG produces expected transition after 2 hours |
| `test_weather_seasonal_bias` | Winter produces more rain states over 100 ticks than summer |
| `test_weather_no_skip_states` | Clear never jumps directly to Storm |
| `test_npc_rain_override` | NPC scheduled outdoors moves indoors when raining |
| `test_farmer_tolerates_light_rain` | Farmer stays outdoors in LightRain |
| `test_palette_new_variants` | PartlyCloudy, LightRain, HeavyRain produce correct tint values |
| `test_weather_changed_event_published` | WeatherChanged event fires on transition |

## Acceptance Criteria

- Weather changes dynamically over time with seasonal variation
- No rapid flipping (minimum 2 game-hour duration)
- NPCs respond to rain/storm by seeking shelter
- Palette visually reflects all 7 weather states
- WeatherChanged events propagate through the event bus
