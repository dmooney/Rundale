# ADR 011: Geo-Tool OSM Pipeline for Automated World Generation

## Status

Accepted

## Context

The game world is built on real Irish geography with 14 hand-authored locations in `data/parish.json`. To scale to thousands or millions of locations across Ireland, we need an automated pipeline to download and convert geographic data.

## Decision

Build a separate binary (`geo-tool`) that:

1. **Downloads** geographic data from the Overpass API (OpenStreetMap)
2. **Extracts** game-relevant features (pubs, churches, farms, ring forts, crossroads, etc.)
3. **Generates** connections from the road network with real walking distances
4. **Creates** 1820s-style description templates from OSM tags
5. **Merges** with existing hand-authored data (curated locations preserved)
6. **Outputs** validated `parish.json` files loadable by the game engine

Key design decisions:

- **Separate binary, shared types**: The geo-tool is a `[[bin]]` in the same crate, reusing `LocationData`, `Connection`, and `WorldGraph` types directly. No data model drift.
- **Overpass API over PBF files**: Targeted queries are more practical than downloading all of Ireland. Caching eliminates redundant downloads.
- **Three-tier descriptions**: `curated` (human-authored, never overwritten), `template` (rule-generated), `llm_pending` (for future LLM enrichment). Metadata sidecar tracks provenance.
- **Administrative-level targeting**: Queries at townland, parish, barony, county, or province level using OSM's `admin_level` hierarchy.
- **LOD filtering**: Full detail (every feature), notable (POIs only), or sparse (major landmarks) — composable with distance-based LOD.

## Consequences

- World can scale from 14 to thousands of locations automatically
- Hand-authored locations are always preserved during merge
- Description quality varies by tier (template < LLM < curated)
- Requires internet access for initial download (cached thereafter)
- Generated data must pass the same `WorldGraph` validation as hand-authored data
- Future LLM enrichment pass can selectively upgrade `template` descriptions
