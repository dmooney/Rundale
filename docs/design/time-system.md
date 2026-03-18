# Time System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

## Day/Night Cycle

- **20 real-world minutes = 1 in-game day** (matches Minecraft pacing)
- Night portion: ~7-8 real minutes

## Seasons & Years

- **Target: 2-3 real-world hours = 1 in-game year**
- ~6-9 in-game days per season
- Each season lasts ~30-45 real-world minutes
- A full year is experienced in a single play session
- Multiple years of play show parish evolution: relationships deepen, people age, things change

## Irish Calendar Festivals

The four traditional Irish seasonal festivals map to the game's seasons:

| Festival      | Season Start   | Approximate Date |
|---------------|----------------|------------------|
| **Imbolc**    | Start of spring | ~February 1     |
| **Bealtaine** | Start of summer | ~May 1          |
| **Lughnasa**  | Start of autumn | ~August 1       |
| **Samhain**   | Start of winter | ~November 1     |

These are potential moments where the mythological layer surfaces. Not scripted yet — but the temporal hooks should exist in the time system.

## Related

- [Weather System](weather-system.md) — Weather varies by season and affects simulation
- [Mythology Hooks](mythology-hooks.md) — Festival dates as mythological event triggers
- [World & Geography](world-geography.md) — Traversal time advances the clock
- [ADR 007: 20 Real Minutes = 1 Game Day](../adr/007-time-scale-20min-day.md)

## Source Modules

- [`src/world/`](../../src/world/) — World state, time system
