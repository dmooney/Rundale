# Geo-Tool — Geographic Data Conversion

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADR: [011](../adr/011-geo-tool-osm-pipeline.md)

A development tool that downloads real geographic data from OpenStreetMap and converts it into the `parish.json` world graph format used by the Parish game engine.

## Purpose

The game world is built on real Irish geography (see [World Geography](world-geography.md)). The geo-tool automates the process of:

1. Downloading geographic features from the Overpass API
2. Classifying them into game-relevant location types
3. Generating connections from the road network
4. Creating description templates in 1820s style
5. Outputting validated `parish.json` files

This enables scaling from the current 15 hand-authored locations to thousands or millions of locations across Ireland.

## Architecture

The geo-tool is a separate binary (`src/bin/geo_tool/`) that shares types with the main game crate.

```
src/bin/geo_tool/
├── main.rs          # CLI entry point (clap)
├── pipeline.rs      # Orchestrates the full conversion workflow
├── overpass.rs      # Overpass API client with caching & retries
├── osm_model.rs     # OSM data model, classification types, distance math
├── extract.rs       # Feature extraction & classification from OSM tags
├── connections.rs   # Connection generation from road network
├── descriptions.rs  # Description template generation (3-tier)
├── lod.rs           # Level-of-detail filtering
├── merge.rs         # Merge logic for curated + generated data
├── cache.rs         # File-based response cache
└── output.rs        # parish.json + metadata output & validation
```

## Usage

```sh
# Generate parish data for a named area at a given admin level
cargo run --bin geo-tool -- --area "Kiltoom" --level parish

# Generate for a full county
cargo run --bin geo-tool -- --area "Roscommon" --level county

# Use a bounding box
cargo run --bin geo-tool -- --bbox 53.45,-8.05,53.55,-7.95

# Merge with existing hand-authored data
cargo run --bin geo-tool -- --area "Kiltoom" --merge data/parish.json

# Dry run — show Overpass queries without executing
cargo run --bin geo-tool -- --area "Kiltoom" --dry-run

# Control detail level
cargo run --bin geo-tool -- --area "Kiltoom" --detail full    # Every feature
cargo run --bin geo-tool -- --area "Kiltoom" --detail notable # POIs only
cargo run --bin geo-tool -- --area "Kiltoom" --detail sparse  # Major landmarks
```

### Administrative Levels

| Level      | OSM admin_level | Description                           |
|------------|-----------------|---------------------------------------|
| `townland` | 10              | Single townland (~50-200 acres)       |
| `parish`   | 8               | Civil parish (group of townlands)     |
| `barony`   | 7               | Barony (group of parishes)            |
| `county`   | 6               | County                                |
| `province` | 5               | Province (Connacht, Leinster, etc.)   |

## Pipeline Stages

1. **Download** — Queries Overpass API for POIs and road network within the area
2. **Extract** — Classifies OSM elements into 25 game location types
3. **LOD Filter** — Applies detail level (full/notable/sparse)
4. **Connect** — Generates connections from road network with traversal times
5. **Build** — Creates `LocationData` with descriptions and mythological hooks
6. **Merge** — Combines with existing curated data (if `--merge`)
7. **Output** — Writes validated `parish.json` + metadata sidecar
8. **Validate** — Verifies output against `WorldGraph` validation rules

## Description Tiers

Each location tracks how its description was generated:

| Tier        | Source                              | Overwritten on re-run? |
|-------------|-------------------------------------|------------------------|
| `curated`   | Hand-authored by human              | Never                  |
| `template`  | Auto-generated from OSM tags        | Yes                    |
| `llm_pending` | Placeholder for future LLM pass  | Yes                    |

The metadata sidecar (`parish-generated.meta.json`) records the tier for each location, enabling selective LLM enrichment later.

## Location Types

The extractor classifies OSM features into these game-relevant types:

| Type           | Indoor? | Public? | Example OSM tags                    |
|----------------|---------|---------|--------------------------------------|
| Pub            | Yes     | Yes     | `amenity=pub`                        |
| Church         | Yes     | Yes     | `amenity=place_of_worship`           |
| Shop           | Yes     | Yes     | `shop=*`                             |
| School         | Yes     | Yes     | `amenity=school`                     |
| PostOffice     | Yes     | Yes     | `amenity=post_office`                |
| Farm           | No      | No      | `building=farmhouse`                 |
| Crossroads     | No      | Yes     | Road junction (3+ ways meeting)      |
| Bridge         | No      | Yes     | `man_made=bridge`                    |
| Well           | No      | Yes     | `historic=holy_well`                 |
| Waterside      | No      | Yes     | `natural=water`                      |
| Bog            | No      | Yes     | `natural=wetland`                    |
| Woodland       | No      | Yes     | `natural=wood`                       |
| RingFort       | No      | Yes     | `historic=ring_fort`                 |
| StandingStone  | No      | Yes     | `historic=standing_stone`            |
| Graveyard      | No      | Yes     | `landuse=cemetery`                   |
| Mill           | Yes     | Yes     | `man_made=watermill`                 |
| Forge          | Yes     | Yes     | `craft=blacksmith`                   |
| LimeKiln       | No      | Yes     | `man_made=kiln`                      |
| Harbour        | No      | Yes     | `leisure=harbour`                    |
| Hill           | No      | Yes     | `natural=peak`                       |
| Ruin           | No      | Yes     | `historic=castle`, `historic=ruins`  |
| NamedPlace     | No      | Yes     | `place=townland|village|hamlet`      |

## Connection Generation

Connections are generated using:

1. **Road proximity** — Features within 100m of the same road are connected, with distance calculated along the road geometry
2. **Direct distance** — Features within 2km but not on a shared road get direct "across the fields" connections
3. **Connectivity enforcement** — BFS finds disconnected components and adds bridge edges

Traversal times assume walking at ~4.5 km/h (75 m/min), minimum 1 minute, maximum 120 minutes.

## Caching

Overpass API responses are cached in `data/cache/geo/` to avoid redundant downloads during iterative development. Use `--no-cache` to force re-download.

## Output Files

- `data/parish-generated.json` — Game-loadable parish data (standard format, includes lat/lon coordinates)
- `data/parish-generated.meta.json` — Provenance metadata (description tiers, OSM IDs)

All generated locations include WGS 84 `lat`/`lon` coordinates in the main parish.json, sourced from OSM geometry. The GUI map panel uses these coordinates for geographic positioning.

## Scale Considerations

- **Chunked processing**: Run at parish level first, then scale to county
- **LOD tiers**: Dense detail for starting area, sparse for distant regions
- **Caching**: Avoids re-downloading on iterative runs
- **Max locations**: `--max-locations N` caps output for testing
- **Validation**: Output is validated against `WorldGraph` rules before writing

## Related

- [World Geography](world-geography.md) — Map source data and world structure
- [ADR 001: Graph-Based World](../adr/001-graph-based-world.md) — Why we use a graph, not a grid
- [ADR 009: Real Geography, Fictional People](../adr/009-real-geography-fictional-people.md) — Design philosophy
