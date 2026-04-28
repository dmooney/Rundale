# Weather System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

Weather is a simulation driver. Weather state is part of world state and affects NPC context prompts, location descriptions, and encounter patterns. It does **not** affect the UI palette — the time-of-day palette is independent of weather.

## Weather Enum

The `Weather` enum in `crates/parish-types/src/ids.rs` defines seven conditions:

| Variant         | Description           |
|-----------------|-----------------------|
| `Clear`         | Sunny / no cover      |
| `PartlyCloudy`  | Mixed sun and cloud   |
| `Overcast`      | Heavy cloud cover     |
| `LightRain`     | Drizzle / light rain  |
| `HeavyRain`     | Sustained rain        |
| `Fog`           | Low visibility        |
| `Storm`         | Wind, thunder, gale   |

## Effects on Simulation

| Weather Condition | Effect                                                        |
|-------------------|---------------------------------------------------------------|
| **Rain**          | Keeps people indoors, changes encounter patterns              |
| **Harsh winters** | Strain resources, shift NPC conversations                     |
| **Beautiful evenings** | Bring people outdoors                                    |
| **Fog**           | Affects atmosphere and NPC behavior                           |
| **Overcast**      | Muted mood, reduced outdoor activity                          |
| **Storms**        | Disruptive, affects travel and NPC schedules                  |

## Related

- [GUI Design](gui-design.md) — GUI color theming
- [Time System](time-system.md) — Seasons drive weather patterns
- [NPC System](npc-system.md) — Weather affects NPC schedules, behavior, and dialogue

## Source Modules

- [`crates/parish-types/src/ids.rs`](../../crates/parish-types/src/ids.rs) — `Weather` enum definition
- [`crates/parish-palette/src/lib.rs`](../../crates/parish-palette/src/lib.rs) — Time-of-day palette interpolation (no weather input)
- [`crates/parish-npc/`](../../crates/parish-npc/) — Weather-aware NPC behavior
