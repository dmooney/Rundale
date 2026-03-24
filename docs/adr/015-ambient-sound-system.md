# ADR-015: Ambient Sound System

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-24)

## Context

Parish already invests heavily in visual atmosphere — smooth color palette interpolation by time of day, season, and weather; atmospheric idle messages; en-route encounter prose. But the soundscape is absent. The game world of 1820s Kilteevan would have been rich with sound: fiddle music from the pub, church bells carrying across the parish, farm animals at dawn, curlews over the bog, crossroads dances on summer evenings. These sounds are documented in `docs/research/music-entertainment.md`.

Sound is inherently spatial and temporal. Church bells carry for miles in flat midlands terrain. Pub music spills into the street and nearby locations. Roosters crow at dawn but not at midnight. Crossroads dances happen on summer evenings. The game already has the data to model this: location graph with traversal times (proxy for distance), `TimeOfDay`, `Season`, `Weather`, and `LocationKind` (in the geo-tool, not yet in the main game).

The question is how to implement ambient sound: as text descriptions (extending the existing idle message system) or as actual audio playback through speakers.

## Decision

Implement ambient sound as **actual audio playback** through speakers, using the **rodio** crate for audio output. Audio plays in **GUI mode only** (egui). TUI and headless modes remain silent.

### Why rodio

- Most popular Rust audio playback library (5.3M+ downloads)
- Simple API: `Sink` for volume control, mixing, pause/resume
- Built on `cpal` (cross-platform audio I/O)
- Supports OGG Vorbis, WAV, MP3, FLAC decoding
- No restriction on simultaneous sounds — mixes automatically
- "noise" feature for procedural white/pink noise (useful for wind)
- Can build without playback support for CI/headless environments
- Already proven with ratatui-based games

### Why not kira

Kira is a more sophisticated game audio library with spatial audio, tweening, and clock-based sequencing. It's powerful but over-engineered for our needs — we don't need 3D spatial audio or sub-tick timing. Our "spatial" model is discrete (graph hops, not coordinates), and volume attenuation by distance is simple enough to do with rodio's `Sink::set_volume()`.

### Why not text-only

Text descriptions of sound ("You hear a fiddle from Darcy's Pub") add atmosphere but don't create immersion the way actual audio does. A faint church bell tolling while you read about the bog road is qualitatively different from a text message saying "A bell tolls in the distance." The two approaches are complementary — ambient text already exists in idle messages and can coexist with audio — but audio is the primary goal.

### Architecture Summary

```
src/audio/
├── mod.rs           # AudioManager: owns rodio OutputStream, manages Sinks
├── catalog.rs       # SoundCatalog: maps (LocationKind, TimeOfDay, Season) → asset paths
├── ambient.rs       # AmbientEngine: selects and schedules sounds based on game state
└── propagation.rs   # Propagation logic: BFS distance, volume attenuation, weather dampening

assets/audio/        # OGG/WAV files, organized by category
├── pub/             # Fiddle reels, crowd murmur, glasses clinking
├── church/          # Bell tolls (close, distant), hymns
├── farm/            # Rooster, cattle, sheep, dogs, donkey
├── water/           # Lake lapping, reeds, waterfowl
├── bog/             # Wind, curlew, silence
├── weather/         # Rain, wind, thunder
├── village/         # Children, doors, footsteps
└── nature/          # Generic birds, wind, insects
```

### Key Design Decisions

1. **LocationKind in main game**: Add `LocationKind` enum to `LocationData` (extending `parish.json`) so the audio system knows what kind of location the player is at and what's nearby.

2. **Graph-based propagation**: Use `WorldGraph` BFS with `traversal_minutes` as distance. Each sound has a propagation range. Volume attenuates linearly with distance. Church bells propagate parish-wide; pub music reaches 1–2 hops; farm sounds reach adjacent locations.

3. **Time/season/weather filtering**: Sound catalog entries are tagged with when they're valid. The ambient engine filters by current game state before selecting sounds.

4. **Layered mixing**: Multiple ambient sounds play simultaneously on separate `Sink`s with independent volume. A base ambient layer (wind, nature) always plays; event sounds (bells, music) layer on top.

5. **GUI-only**: Audio is gated behind GUI mode. The `AudioManager` is `Option<AudioManager>` in the app, `None` in TUI/headless. The `rodio` dependency uses a cargo feature flag (`audio`) so builds without audio support remain possible.

6. **Royalty-free assets**: All audio files must be CC0 (public domain) or CC-BY (attribution) licensed. See `docs/research/ambient-sound-sources.md` for sourcing research.

## Consequences

- **New dependency**: `rodio` (and transitively `cpal`) added to `Cargo.toml` behind a feature flag.
- **Asset management**: Binary size increases with bundled audio files. Assets should be OGG Vorbis (good compression, patent-free). Total target: <20 MB for the full sound catalog.
- **CI compatibility**: CI environments may lack audio hardware. The `audio` feature flag allows building/testing without rodio. Audio-specific tests use mocks or are `#[cfg(feature = "audio")]`.
- **`LocationKind` in parish.json**: Adds a required field to location data, breaking old JSON files without it. Use `#[serde(default)]` for backward compat during transition.

## Alternatives Considered

| Alternative | Reason Rejected |
|---|---|
| Text-only ambient sounds | Doesn't meet the user's goal of actual audio playback |
| kira game audio library | Over-engineered for our discrete spatial model |
| cpal directly | Too low-level; rodio provides the mixing/decoding layer we need |
| Bevy audio | Would require adopting the Bevy ECS; massive dependency for one feature |
| Web Audio API (Phase 7) | Future consideration for web/mobile; rodio handles native desktop now |
