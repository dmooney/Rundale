# Weather System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

Weather is a simulation driver, not just visual dressing. Weather state is part of world state and affects NPC context prompts, location descriptions, and color palettes.

## Weather Enum

The `Weather` enum in `src/world/mod.rs` defines five conditions:

| Variant      | Description                               |
|--------------|-------------------------------------------|
| `Clear`      | Default — no palette modification         |
| `Overcast`   | Slightly darker and desaturated           |
| `Rain`       | Darker with a blue-gray tint              |
| `Fog`        | Washed out, low contrast                  |
| `Storm`      | Much darker and heavily desaturated       |

## Effects on Simulation

| Weather Condition | Effect                                                        |
|-------------------|---------------------------------------------------------------|
| **Rain**          | Keeps people indoors, changes encounter patterns              |
| **Harsh winters** | Strain resources, shift NPC conversations                     |
| **Beautiful evenings** | Bring people outdoors                                    |
| **Fog**           | Affects atmosphere and NPC behavior                           |
| **Overcast**      | Muted mood, reduced outdoor activity                          |
| **Storms**        | Disruptive, affects travel and NPC schedules                  |

## Palette Tinting

Weather modifies the base time-of-day color palette via multiplicative tinting in `src/world/palette.rs`. The tinting system applies three layers:

1. **Time-of-day interpolation** — smooth linear interpolation between 7 keyframe palettes based on exact hour and minute
2. **Season tinting** — subtle color shifts (Winter: cooler/bluer, Summer: warmer, Autumn: amber, Spring: greener)
3. **Weather tinting** — brightness, desaturation, and color temperature adjustments

### Weather Tint Parameters

| Weather   | RGB Multiplier         | Desaturation | Brightness | Contrast Reduction |
|-----------|------------------------|-------------|------------|-------------------|
| Clear     | (1.0, 1.0, 1.0)       | 0%          | 100%       | 0%                |
| Overcast  | (0.95, 0.95, 0.97)    | 15%         | 92%        | 0%                |
| Rain      | (0.88, 0.90, 0.95)    | 20%         | 85%        | 0%                |
| Fog       | (0.97, 0.97, 0.98)    | 35%         | 95%        | 15%               |
| Storm     | (0.80, 0.82, 0.85)    | 30%         | 75%        | 0%                |

### Season Tint Parameters

| Season  | RGB Multiplier         | Desaturation |
|---------|------------------------|-------------|
| Spring  | (0.98, 1.02, 0.98)    | 0%          |
| Summer  | (1.03, 1.01, 0.97)    | 0%          |
| Autumn  | (1.06, 1.00, 0.92)    | 0%          |
| Winter  | (0.94, 0.96, 1.04)    | 8%          |

The GUI consumes `RawPalette` from the engine.

## Related

- [GUI Design](gui-design.md) — GUI color theming
- [Time System](time-system.md) — Seasons drive weather patterns
- [NPC System](npc-system.md) — Weather affects NPC schedules, behavior, and dialogue

## Source Modules

- [`src/world/mod.rs`](../../src/world/mod.rs) — `Weather` enum definition
- [`src/world/palette.rs`](../../src/world/palette.rs) — Smooth interpolation engine, season/weather tinting
- [`src/world/palette.rs`](../../src/world/palette.rs) — Palette engine, season/weather tinting
- [`src/npc/`](../../src/npc/) — Weather-aware NPC behavior
