# Time System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADR: [007](../adr/007-time-scale-20min-day.md)

## Day/Night Cycle

- **40 real-world minutes = 1 in-game day** at Normal speed (factor 36.0)
- Night portion: ~14-16 real minutes

## Game Speed Presets

Speed is adjustable at runtime via the `/speed` command, inspired by SimCity:

| Command | Preset | Factor | Real time per game day |
|---------|--------|--------|----------------------|
| `/speed slow` | Slow | 18.0 | 80 minutes |
| `/speed normal` | Normal (default) | 36.0 | 40 minutes |
| `/speed fast` | Fast | 72.0 | 20 minutes |
| `/speed fastest` | Fastest | 144.0 | 10 minutes |
| `/speed ludicrous` | Ludicrous | 864.0 | 100 seconds |

`/speed` alone shows the current pace. Speed changes recalibrate the clock
seamlessly — current game time is preserved, only the rate of passage changes.

## Seasons & Years

- **Target: 4-6 real-world hours = 1 in-game year** at Normal speed
- ~6-9 in-game days per season
- Each season lasts ~60-90 real-world minutes at Normal speed
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
