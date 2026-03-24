# Ambient Sound System Design

> [Docs Index](../index.md) | [ADR-015](../adr/015-ambient-sound-system.md) | [Audio Sources](../research/ambient-sound-sources.md) | [Music Research](../research/music-entertainment.md)

## Overview

The ambient sound system plays period-correct audio through the player's speakers based on their location, the time of day, season, and weather. Sound sources are tied to location types and propagate through the world graph — church bells carry across the entire parish, pub music spills into the street and adjacent locations, farm animals are heard from neighboring fields.

Audio playback is **GUI-mode only** using the `rodio` crate, gated behind a `audio` cargo feature flag. TUI and headless modes are unaffected.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  GuiApp::update()  (every frame)                │
│                                                 │
│  1. Read game state (location, time, weather)   │
│  2. Call AmbientEngine::tick(game_state)         │
│  3. AmbientEngine decides what to play/stop     │
│  4. AudioManager adjusts Sinks                  │
└─────────────────────────────────────────────────┘

┌──────────────┐     ┌───────────────┐     ┌──────────────┐
│ AmbientEngine│────▶│  SoundCatalog │────▶│ AudioManager │
│              │     │               │     │              │
│ - game state │     │ - asset paths │     │ - rodio      │
│ - propagation│     │ - filters     │     │ - Sinks      │
│ - scheduling │     │ - variants    │     │ - mixing     │
└──────────────┘     └───────────────┘     └──────────────┘
```

### Module Structure

```
src/audio/
├── mod.rs           # AudioManager, feature gate, public API
├── catalog.rs       # SoundCatalog: what sounds exist and when they play
├── ambient.rs       # AmbientEngine: orchestrates sound selection and scheduling
└── propagation.rs   # Graph-based sound propagation and volume calculation
```

## Core Types

### LocationKind

Added to `src/world/mod.rs` and `LocationData` in `src/world/graph.rs`. Classifies each location for sound purposes.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    Pub,
    Church,
    Farm,
    Crossroads,
    Waterside,
    Bog,
    Village,
    Shop,
    School,
    SportField,
    FairyFort,
    LimeKiln,
    PostOffice,
    Road,
}
```

### SoundEntry

A single sound asset with its playback conditions.

```rust
pub struct SoundEntry {
    /// Path to the audio file relative to assets/audio/
    pub path: &'static str,
    /// What location type produces this sound
    pub source_kind: LocationKind,
    /// How far the sound carries through the graph
    pub propagation: Propagation,
    /// When this sound can play (time of day)
    pub time_filter: TimeFilter,
    /// Seasonal constraint
    pub season_filter: SeasonFilter,
    /// Weather constraint
    pub weather_filter: WeatherFilter,
    /// Whether this is a looping ambient layer or a one-shot event
    pub loop_mode: LoopMode,
    /// Relative volume (0.0–1.0) before distance attenuation
    pub base_volume: f32,
}
```

### Propagation

How far a sound carries through the world graph.

```rust
pub enum Propagation {
    /// Only audible at the source location.
    Local,
    /// Audible at source + direct neighbors (1 edge).
    Near,
    /// Audible up to 2 edges away.
    Medium,
    /// Audible up to `max_minutes` total traversal time from source.
    /// Church bells use Far(60) to cover the entire parish.
    Far(u16),
}
```

### TimeFilter / SeasonFilter / WeatherFilter

```rust
pub enum TimeFilter {
    Any,
    Only(Vec<TimeOfDay>),
    Except(Vec<TimeOfDay>),
}

pub enum SeasonFilter {
    Any,
    Only(Vec<Season>),
}

pub enum WeatherFilter {
    Any,
    Only(Vec<Weather>),
    Except(Vec<Weather>),
}
```

### LoopMode

```rust
pub enum LoopMode {
    /// Plays continuously, crossfading when restarting
    Loop,
    /// Plays once, then the engine may select it again later
    OneShot,
}
```

## Sound Propagation Algorithm

When the ambient engine ticks, it determines which sounds the player can hear:

```
function audible_sounds(player_location, graph, time, season, weather):
    candidates = []

    // 1. Local sounds from player's location
    for entry in catalog where entry.source_kind == player_location.kind:
        if entry matches time, season, weather filters:
            candidates.add(entry, distance=0)

    // 2. Propagated sounds via BFS
    visited = {player_location}
    frontier = [(player_location, 0)]  // (location, cumulative_minutes)

    while frontier is not empty:
        (loc, dist) = frontier.pop()
        for (neighbor, edge_minutes) in graph.neighbors(loc):
            new_dist = dist + edge_minutes
            if neighbor not in visited:
                visited.add(neighbor)
                for entry in catalog where entry.source_kind == neighbor.kind:
                    if entry.propagation reaches new_dist:
                        if entry matches time, season, weather:
                            candidates.add(entry, distance=new_dist)
                // Only continue BFS if some sound could still propagate further
                if any sound has propagation > new_dist:
                    frontier.add((neighbor, new_dist))

    // 3. Volume calculation
    for each candidate (entry, distance):
        volume = entry.base_volume * attenuation(distance, entry.propagation)
        volume *= weather_dampening(weather)
        if player_location.indoor:
            volume *= 0.4  // Walls muffle exterior sounds

    return candidates with volumes
```

### Volume Attenuation

```rust
fn attenuation(distance_minutes: u16, propagation: &Propagation) -> f32 {
    match propagation {
        Propagation::Local => if distance_minutes == 0 { 1.0 } else { 0.0 },
        Propagation::Near => {
            if distance_minutes == 0 { 1.0 }
            else { 0.5 }  // Neighbors hear at half volume
        }
        Propagation::Medium => {
            // Linear falloff over ~15 minutes
            (1.0 - (distance_minutes as f32 / 15.0)).max(0.0)
        }
        Propagation::Far(max) => {
            // Logarithmic falloff — bells are loud
            let ratio = distance_minutes as f32 / *max as f32;
            (1.0 - ratio.sqrt()).max(0.0)
        }
    }
}
```

### Weather Dampening

Weather affects how far sound carries:

| Weather | Dampening Factor | Notes |
|---------|-----------------|-------|
| Clear | 1.0 | No effect |
| Overcast | 0.95 | Negligible |
| Rain | 0.6 | Rain masks distant sounds |
| Fog | 0.5 | Fog absorbs sound |
| Storm | 0.3 | Only nearby sounds audible through storm |

Fog and storm also add their own sounds (wind, rain, thunder) which layer on top.

## Sound Catalog

### Pub (LocationKind::Pub)

| Sound | Propagation | Time | Season | Loop | Notes |
|-------|-------------|------|--------|------|-------|
| Fiddle reel/jig | Near | Night, Dusk | Any | Loop | Primary pub music. Loudest sound. |
| Sean-nós singing | Near | Night | Any | OneShot | Solo voice, occasional. |
| Crowd murmur | Local | Afternoon–Night | Any | Loop | Background conversation layer. |
| Glasses clinking | Local | Afternoon–Night | Any | OneShot | Intermittent. |
| Hearth crackling | Local | Any | Autumn, Winter | Loop | Low volume base layer indoors. |

### Church (LocationKind::Church)

| Sound | Propagation | Time | Season | Loop | Notes |
|-------|-------------|------|--------|------|-------|
| Bell toll (Angelus) | Far(60) | Dawn, Midday, Dusk | Any | OneShot | 3 sets of 3 tolls. Heard everywhere. |
| Bell toll (Sunday) | Far(60) | Morning | Any | OneShot | Extended peal. Only on Sundays [1]. |
| Hymns | Medium | Morning (Sunday) | Any | Loop | Faint singing during service. |
| Silence / stone echo | Local | Any | Any | Loop | Default: near-silence, echo quality. |

[1] Sunday detection requires checking the game clock's day-of-week. The `GameClock` struct should expose this.

### Farm (LocationKind::Farm)

| Sound | Propagation | Time | Season | Loop | Notes |
|-------|-------------|------|--------|------|-------|
| Rooster crow | Near | Dawn | Any | OneShot | Iconic dawn sound. Multiple variants. |
| Cattle lowing | Near | Morning–Dusk | Any | OneShot | Intermittent throughout day. |
| Sheep bleating | Local | Morning–Dusk | Any | OneShot | Seasonal: more in Spring (lambing). |
| Dog barking | Near | Any | Any | OneShot | Alert/guard, intermittent. |
| Donkey braying | Medium | Any | Any | OneShot | Loud, carries far. Occasional. |
| Hens clucking | Local | Morning–Afternoon | Any | Loop | Low background layer. |

### Waterside (LocationKind::Waterside)

| Sound | Propagation | Time | Season | Loop | Notes |
|-------|-------------|------|--------|------|-------|
| Water lapping | Local | Any | Any | Loop | Base layer for lakeshore. |
| Wind in reeds | Local | Any | Any | Loop | Layered with water. |
| Waterfowl | Local | Dawn, Dusk | Spring, Summer | OneShot | Herons, ducks. |

### Bog (LocationKind::Bog)

| Sound | Propagation | Time | Season | Loop | Notes |
|-------|-------------|------|--------|------|-------|
| Wind over bog | Local | Any | Any | Loop | Base layer. Intensity varies with weather. |
| Curlew call | Near | Dawn, Dusk | Spring, Summer | OneShot | Haunting, emblematic. |
| Bog silence | Local | Night, Midnight | Any | Loop | Near-silence. Unsettling. |

### Crossroads (LocationKind::Crossroads)

| Sound | Propagation | Time | Season | Loop | Notes |
|-------|-------------|------|--------|------|-------|
| Crossroads dance music | Near | Dusk, Night | Summer | Loop | Fiddle + dancing feet. Summer evenings only. |
| Wind at crossroads | Local | Any | Any | Loop | Default: wind and silence. |

### Village (LocationKind::Village)

| Sound | Propagation | Time | Season | Loop | Notes |
|-------|-------------|------|--------|------|-------|
| Rooster | Local | Dawn | Any | OneShot | Village roosters. |
| Children playing | Local | Morning, Afternoon | Not Winter | OneShot | Distant voices, laughter. |
| Door/footsteps | Local | Morning–Dusk | Any | OneShot | Domestic activity. |
| Evening settling | Local | Dusk | Any | Loop | Quiet, smoke, last sounds of day. |

### Fairy Fort (LocationKind::FairyFort)

| Sound | Propagation | Time | Season | Loop | Notes |
|-------|-------------|------|--------|------|-------|
| Hawthorn wind | Local | Any | Any | Loop | Wind through thorny branches. Eerie quality. |
| Uncanny silence | Local | Night, Midnight | Any | Loop | Even wind drops. Deeply quiet. |
| Samhain atmosphere | Local | Night | Autumn | Loop | Heightened eeriness near Oct 31. |

### Weather Overlay

Weather sounds play globally regardless of location:

| Sound | Weather | Loop | Notes |
|-------|---------|------|-------|
| Light rain | Rain | Loop | Constant patter. Volume varies by indoor/outdoor. |
| Heavy rain | Storm | Loop | Intense. Masks most other sounds. |
| Wind | Rain, Storm | Loop | Layered with rain. |
| Thunder | Storm | OneShot | Intermittent rumbles. |
| Fog silence | Fog | Loop | Subtle dampening effect on all other sounds. |

## AudioManager

The `AudioManager` owns the rodio `OutputStream` and manages a fixed set of audio channels (Sinks):

```rust
pub struct AudioManager {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    channels: AudioChannels,
}

struct AudioChannels {
    /// Base ambient layer (wind, water, silence). Always playing.
    ambient: Sink,
    /// Music layer (fiddle, hymns). Fades in/out.
    music: Sink,
    /// Nature layer (birds, animals). Intermittent.
    nature: Sink,
    /// Weather overlay (rain, wind, thunder). Fades with weather changes.
    weather: Sink,
    /// One-shot events (bell toll, rooster crow, door slam).
    events: Sink,
}
```

Each `Sink` has independent volume control. The `AmbientEngine` sets volumes per channel based on the propagation calculation.

### Crossfading

When the player moves to a new location, sounds must transition smoothly:

1. Calculate new sound set for the destination
2. Sounds that continue (e.g., weather) keep playing
3. Sounds that change (ambient layer) crossfade over ~2 seconds
4. Sounds that stop fade out over ~1 second
5. New sounds fade in over ~1 second

## AmbientEngine

The engine ticks on every GUI frame but only re-evaluates sounds when game state changes meaningfully:

```rust
pub struct AmbientEngine {
    catalog: SoundCatalog,
    current_location: LocationId,
    current_time: TimeOfDay,
    current_weather: Weather,
    current_season: Season,
    active_sounds: Vec<ActiveSound>,
    last_event_time: Instant,
    event_cooldown: Duration,  // Minimum time between one-shot events
}

impl AmbientEngine {
    /// Called every frame. Only re-evaluates if state changed.
    pub fn tick(&mut self, state: &GameState, audio: &AudioManager) {
        let state_changed = self.detect_state_change(state);
        if state_changed {
            self.rebuild_sound_set(state, audio);
        }
        self.maybe_trigger_event(state, audio);
    }
}
```

### State Change Detection

Re-evaluate the full sound set when any of these change:
- Player location
- Time of day (not every minute — only when `TimeOfDay` enum variant changes)
- Weather
- Season

### Event Scheduling

One-shot sounds (rooster, bell, dog bark) are triggered probabilistically with cooldowns:
- Minimum 30 seconds between events on the same channel
- Each event has a probability per tick (e.g., rooster at dawn: 5% per second)
- Church bells follow the actual game clock (Angelus at 6:00, 12:00, 18:00)

## Integration with GUI

```rust
// In src/gui/mod.rs

pub struct GuiApp {
    // ... existing fields ...
    #[cfg(feature = "audio")]
    audio_manager: Option<AudioManager>,
    #[cfg(feature = "audio")]
    ambient_engine: Option<AmbientEngine>,
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ... existing update logic ...

        // Ambient sound tick
        #[cfg(feature = "audio")]
        if let (Some(engine), Some(audio)) =
            (&mut self.ambient_engine, &self.audio_manager)
        {
            let state = GameState::from_world(&self.world);
            engine.tick(&state, audio);
        }
    }
}
```

### Feature Flag

In `Cargo.toml`:

```toml
[features]
default = ["audio"]
audio = ["rodio"]

[dependencies]
rodio = { version = "0.19", optional = true, default-features = false, features = ["vorbis"] }
```

Building without audio: `cargo build --no-default-features`

## Cargo Feature Considerations

- CI should test both `--features audio` and `--no-default-features` to ensure the feature gate compiles cleanly.
- All `src/audio/` code is wrapped in `#[cfg(feature = "audio")]`.
- The `lib.rs` module declaration: `#[cfg(feature = "audio")] pub mod audio;`

## Testing Strategy

### Unit Tests

- `catalog.rs`: Test that all entries have valid filters, no duplicate paths.
- `propagation.rs`: Test BFS propagation with a small test graph. Verify volume attenuation math.
- `ambient.rs`: Test sound selection for various (location, time, season, weather) combos.

### Integration Tests

- Use `GameTestHarness` with `--script` mode. Add a `check_ambient` query that returns currently-playing sounds as JSON.
- Test scenario: walk to Darcy's Pub at night → verify fiddle music is selected.
- Test scenario: stand at Crossroads at dawn → verify rooster from Murphy's Farm propagates.
- Test scenario: stand at Fairy Fort at midnight in Autumn → verify eerie silence + Samhain atmosphere.

### Audio tests run only with feature flag

```rust
#[cfg(test)]
#[cfg(feature = "audio")]
mod audio_tests {
    // Tests that require rodio
}
```

## Future Considerations

- **Web/Mobile (Phase 7)**: Web Audio API replaces rodio. The `AmbientEngine` and `SoundCatalog` are platform-agnostic; only `AudioManager` needs a web backend.
- **Dynamic events**: Wakes, pattern days, hurling matches could trigger special soundscapes.
- **NPC-driven sounds**: A travelling piper NPC arriving could trigger pipe music at their location.
- **Player volume control**: Add a volume slider to the GUI settings panel.
- **Procedural wind**: Use rodio's noise generator + filters to create wind that varies continuously rather than looping a clip.
