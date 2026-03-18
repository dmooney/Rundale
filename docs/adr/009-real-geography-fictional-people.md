# ADR-009: Real Geography, Fictional People

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-18)

## Context

Parish is set in rural Ireland, centered on a parish near Roscommon, County Roscommon. The world-building approach must balance authenticity (a genuine sense of Irish place) with legal and ethical constraints (no real people or businesses should be depicted).

Ireland has exceptionally rich geographic data available:

- **OpenStreetMap** (via Geofabrik Ireland extract): Roads, buildings, waterways, railways, places, land use. ODbL licensed.
- **Townlands.ie**: Parish and townland boundaries in GeoJSON/Shapefile/CSV. Roscommon alone has 7 baronies, 62 civil parishes, 110 electoral divisions, and 2,082 townlands.
- **Tailte Eireann** (formerly Ordnance Survey Ireland): Official civil parish, townland, county, and barony boundaries. CC-BY licensed.
- **Historical maps**: GeoHive historical OS maps and Down Survey maps for world-building reference.

The townland is the fundamental unit of Irish rural land division, with pre-Norman origins. Using real townland names and boundaries creates an authentic sense of place that procedural generation cannot match.

## Decision

Use **real Irish geography with entirely fictional characters and businesses**.

**Geography (real):**

- Location nodes are derived from real places in OpenStreetMap data
- Townland, parish, and barony boundaries from Townlands.ie and Tailte Eireann
- Real road networks, waterways, and landmarks inform the location graph and traversal times
- Starting area: a parish near Roscommon (exact parish to be selected)
- Resolution decreases with distance: ~30-50 nodes for the starting parish, ~5 nodes for distant cities

**People and businesses (fictional):**

- All NPCs are fictional characters with no intentional resemblance to real people
- All businesses, organizations, and institutions are fictional
- A disclaimer is required: "Any resemblance to real persons, living or dead, or actual businesses is purely coincidental. All characters and commercial establishments in this game are fictional."

**Data licensing compliance:**

- OpenStreetMap data: ODbL license (attribution required, share-alike for derivative databases)
- Tailte Eireann data: CC-BY license (attribution required)
- Attribution must be displayed in-game or in documentation

## Consequences

**Positive:**

- Authentic sense of place: real townland names, real roads, real landmarks create a world that feels genuinely Irish
- Educational value: players learn real Irish geography, townland names, and spatial relationships
- Rich source data: thousands of real locations, roads, and features to draw from
- The townland system provides natural location granularity for the game's node graph
- Historical depth: centuries of real history inform the world's texture and atmosphere

**Negative:**

- ODbL license compliance requires attribution and share-alike for derivative databases
- CC-BY license requires attribution for Tailte Eireann data
- Must carefully ensure no real people or businesses are inadvertently depicted
- Real geography constrains world design: cannot move a pub to where it would be more convenient for gameplay
- OSM data processing requires filtering, cleaning, and transformation into the game's node graph format
- Players familiar with the real locations may notice inaccuracies or missing details

## Alternatives Considered

- **Fully fictional world**: Create an entirely imagined Irish-flavored setting. Avoids all licensing and accuracy concerns but loses the authentic sense of place. "A generic Irish village" does not feel the same as "a specific townland in County Roscommon." The connection to real geography is a core design goal.
- **Procedural generation**: Generate terrain and settlements algorithmically. Produces a generic world that lacks the specificity and character of real places. Irish geography has distinctive features (townland boundaries, boreen networks, specific landmark types) that procedural generation would not capture.
- **Real people and businesses**: Depict actual residents and businesses of the area. Completely unacceptable due to privacy and legal risks. Even with good intentions, fictional scenarios involving real people could cause harm.
- **Anonymized real data**: Use real geographic patterns but change all names. Loses the authentic connection to place while still requiring the same data processing effort. The townland names themselves carry cultural and historical significance.

## Related

- [docs/design/world-geography.md](../design/world-geography.md)
- [ADR-001: Graph-Based World Representation](001-graph-based-world.md)
