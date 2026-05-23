# World & Geography

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADRs: [001](../adr/001-graph-based-world.md), [009](../adr/009-real-geography-fictional-people.md)

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

Used for period accuracy, atmosphere, and verifying that generated location data matches the 1820s landscape. Not suitable as a primary import pipeline but valuable for cross-checking and for the tile/visual layer.

#### Raw GIS data — free downloads

**GSI Goldmine / geologicalmaps.net (best available)**
- The Geological Survey of Ireland's 1-inch historical sheets drawn on the same OS base survey as the 6-inch maps, covering all of Ireland
- Free TIFF downloads per sheet; CC Attribution 4.0 license; ~450,000-page archive
- The geological overlay is secondary — the base layer shows roads, settlements, drainage, townlands, and parish names, making these genuine 19th-century topographic sources
- Sheet index and download: https://www.geologicalmaps.net/irishhistmaps/mapindex.cfm
- Full archive (reports + maps): https://secure.decc.gov.ie/goldmine/index.html

**irelandmapped.ie — OS200 Project (launched June 2024)**
- Open-access digital archive from University of Limerick + Queen's University Belfast, funded by the Irish Research Council / AHRC
- Connects First Edition 6-inch maps (surveyed 1829–1842) with OS Memoirs, Letters, and Name Books
- Shapefile download confirmed; ArcGIS and OpenLayers integration; IIIF image links
- https://www.irelandmapped.ie

**David Rumsey Map Collection**
- Has 19th-century Ireland coverage including the Railway Commissioners' map
- Built-in georeferencing tool; export any map as GeoTIFF directly from the website
- Thinner Irish coverage than UK but free and accessible
- https://www.davidrumsey.com (search "Ireland")

**OpenHistoricalMap**
- OSM-format vector data with date-tagged historical features; daily planet dump on Amazon S3
- Overpass API available for spatial queries by bounding box + date range
- Ireland 1800s coverage is sparse (volunteer-traced); worth querying before investing
- https://www.openhistoricalmap.org

**British Library Mechanical Curator → Wikimedia Commons**
- The BL digitized ~1 million images from 19th-century books; a georeferencing project has worked through the Ireland-relevant subset
- Individual high-res scans available; coverage is map-by-map rather than systematic
- Category: https://commons.wikimedia.org/wiki/Category:19th-century_maps_of_Ireland
- Georeferenced subset: https://commons.wikimedia.org/wiki/Commons:British_Library/Mechanical_Curator_collection/georeferencing_done

#### Reference / viewer only

- GeoHive historical OS maps (6-inch and 25-inch series, viewer): https://webapps.geohive.ie/mapviewer/index.html
- Irish Townland and Historical Map Viewer (OSi ArcGIS): https://osi.maps.arcgis.com/apps/webappviewer/index.html?id=bc56a1cf08844a2aa2609aa92e89497e
- Down Survey maps (17th-century, TCD GIS project): https://downsurvey.tcd.ie/down-survey-maps.php
- NLS 6-inch Ireland 1829–1969 (viewable free, JPEG purchase): https://maps.nls.uk/os/6inch-ireland/
- UCD Digital Collections — ~1,370 OS town plans 1847–1896: https://digital.ucd.ie/view/ucdlib:40377

#### Survey of the field

- "Open Historical Maps of Ireland" (academic overview): https://www.academia.edu/43288155/Open_Historical_Maps_of_Ireland

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
- **Geolocation** (WGS 84 latitude/longitude for real-world map placement)
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
