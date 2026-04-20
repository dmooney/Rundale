# Mythology Layer (Future Hooks)

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

Irish mythology should have structural hooks in the prototype even if no content exists yet. **No mythological content or events for v1. Just ensure the data model doesn't preclude it.**

**Update (2026-04):** A first pass at surfacing the mythology layer lives in
[`crates/parish-world/src/liminal.rs`](../../crates/parish-world/src/liminal.rs).
The `/omen` command and the `liminal-moments` feature flag expose atmospheric
readings at mythologically significant locations, scored from a site's
classification × time-of-day × weather × festival. See the playtest fixture
[`testing/fixtures/play_liminal.txt`](../../testing/fixtures/play_liminal.txt).

## Festival Date Hooks

The time system tracks the four traditional Irish seasonal festivals:

| Festival      | Date           | Significance                                    |
|---------------|----------------|-------------------------------------------------|
| **Samhain**   | ~November 1    | Start of winter; boundary between worlds thins  |
| **Imbolc**    | ~February 1    | Start of spring; renewal and purification       |
| **Bealtaine** | ~May 1         | Start of summer; fertility and fire             |
| **Lughnasa**  | ~August 1      | Start of autumn; harvest and assembly           |

These are potential moments where the mythological layer surfaces.

## Location Mythological Significance

Location nodes can have a `mythological_significance` property. Types include:

- Fairy forts (ring forts / raths)
- Holy wells
- Crossroads
- Bogs
- Ancient burial sites
- Standing stones

## Day/Night Atmospheric Space

The day/night cycle creates space for mythological content:

- **Daytime** = social simulation, the human world
- **Nighttime** = potential for "something else"

## NPC Beliefs & Superstitions

The NPC knowledge system can accommodate:

- Beliefs and superstitions
- Half-remembered stories
- Local folklore and traditions
- Fear or reverence for specific locations

These would be stored as part of the NPC knowledge model and could surface in dialogue.

## Related

- [Time System](time-system.md) — Festival dates and day/night cycle
- [NPC System](npc-system.md) — NPC knowledge model for beliefs/superstitions
- [World & Geography](world-geography.md) — Location mythological_significance property

## Source Modules

- [`src/world/`](../../src/world/) — Location properties, time system
- [`src/npc/`](../../src/npc/) — NPC knowledge model
