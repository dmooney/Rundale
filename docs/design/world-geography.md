# World & Geography

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md)

The world is built on real Irish geography. All places are real. All people and businesses are fictional.

## Map Source Data

### OpenStreetMap (Primary)

- Source: Geofabrik Ireland extract
- Data: Roads, buildings, waterways, railways, places, land use
- Filter to County Roscommon / target parish for starting area
- License: ODbL (attribution required)
- Download: https://download.geofabrik.de/europe/ireland-and-northern-ireland.html

### Townlands.ie (Parish/Townland Boundaries)

- Format: GeoJSON/Shapefile/CSV downloads
- Roscommon: 7 baronies, 62 civil parishes, 110 electoral divisions, 2,082 townlands
- Townland = fundamental unit of Irish rural land division (pre-Norman origin)
- Download: https://www.townlands.ie/page/download/

### Tailte Eireann (Official Boundaries)

- Formerly Ordnance Survey Ireland
- Data: Civil parishes, townlands, counties, baronies
- License: CC-BY
- Formats: CSV, KML, Shapefile, GeoJSON (ITM projection)
- Portal: https://data-osi.opendata.arcgis.com/

### Historical Reference (World-Building)

Not for direct data import — used for world-building context and atmosphere:

- GeoHive historical OS maps (6-inch and 25-inch series): https://webapps.geohive.ie/mapviewer/index.html
- Down Survey maps (17th century): http://downsurvey.tcd.ie/down-survey-maps.php

## World Structure

The world is a **graph of location nodes**, not a continuous coordinate grid.

- **Nodes**: Named locations — the pub, the church, farms, crossroads, landmarks, the fairy fort
- **Edges**: Paths between nodes with traversal times in game-minutes (derived from real distances in OSM data)
- **Movement**: Natural language ("go to the pub", "walk to the church", "head down the boreen toward Lough Ree")
- **Traversal**: The world ticks forward while the player moves. A 10-minute walk means 10 game-minutes of simulation. Encounters may happen en route.

### Resolution by Distance

| Area             | Detail Level                          |
|------------------|---------------------------------------|
| Starting parish  | ~30-50 location nodes (dense, intimate) |
| Roscommon town   | ~10 nodes (visitor-level detail)      |
| Galway/Athlone   | Sparse                                |
| Dublin/Cork      | ~5 nodes (you're a stranger here)     |

### Location Properties

Each location has:

- **Name** (real place name)
- **Description template** (dynamically enriched by LLM based on time, weather, season, current events)
- **Connections** to other locations with traversal times
- **Properties**: indoor/outdoor, public/private
- **Associated NPCs** (home, workplace)
- **Mythological significance** (fairy forts, holy wells, crossroads, bogs — future hook)

The map is a **static authored data file** (JSON or SQLite). Geography never changes. Only the people and events within it are dynamic.

## Disclaimer

> Any resemblance to real persons, living or dead, or actual businesses is purely coincidental. All characters and commercial establishments in this game are fictional.

## Related

- [ADR 001: Graph-Based World](../adr/001-graph-based-world.md)
- [ADR 009: Real Geography, Fictional People](../adr/009-real-geography-fictional-people.md)
- [Time System](time-system.md) — Traversal advances game time
- [NPC System](npc-system.md) — NPCs are bound to location nodes

## Source Modules

- [`src/world/`](../../src/world/) — World state, location graph, time system
