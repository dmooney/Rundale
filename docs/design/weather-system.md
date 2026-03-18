# Weather System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

Weather is a simulation driver, not just visual dressing. Weather state is part of world state and affects NPC context prompts.

## Effects on Simulation

| Weather Condition | Effect                                                        |
|-------------------|---------------------------------------------------------------|
| **Rain**          | Keeps people indoors, changes encounter patterns              |
| **Harsh winters** | Strain resources, shift NPC conversations                     |
| **Beautiful evenings** | Bring people outdoors                                    |
| **Fog**           | Affects atmosphere and NPC behavior                           |
| **Overcast**      | Muted mood, reduced outdoor activity                          |
| **Storms**        | Disruptive, affects travel and NPC schedules                  |

## TUI Color Palette Modifiers

Weather modifies the base time-of-day color palette in the TUI:

| Weather   | Palette Effect              |
|-----------|-----------------------------|
| Overcast  | Muted/desaturated           |
| Rain      | Cooler tones, grey cast     |
| Fog       | Heavily desaturated         |
| Clear     | Full saturation             |

See [TUI Design](tui-design.md) for the full color system.

## Related

- [TUI Design](tui-design.md) — Weather palette modifiers and visual atmosphere
- [Time System](time-system.md) — Seasons drive weather patterns
- [NPC System](npc-system.md) — Weather affects NPC schedules, behavior, and dialogue

## Source Modules

- [`src/world/`](../../src/world/) — World state, weather state
- [`src/tui/`](../../src/tui/) — Weather-driven palette modifiers
- [`src/npc/`](../../src/npc/) — Weather-aware NPC behavior
