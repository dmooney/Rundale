# ADR-001: Graph-Based World Representation

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-18)

## Context

Parish is a text adventure set in rural Ireland, built on real geography. The game world needs to represent the island of Ireland at varying levels of detail, from a dense starting parish near Roscommon (~30-50 locations) down to sparse representations of distant cities (~5 nodes).

The representation must support:

- Natural language movement ("go to the pub", "walk to the church", "head down the boreen toward Lough Ree")
- Variable spatial resolution based on distance from the starting area
- Traversal times derived from real-world distances in OpenStreetMap data
- Dynamic descriptions enriched by LLM based on time, weather, season, and events
- Properties per location: indoor/outdoor, public/private, associated NPCs, mythological significance

Three primary options were considered: a continuous coordinate grid, a graph of named nodes, and procedural generation.

## Decision

The world is represented as a **graph of named location nodes** connected by weighted edges. Each edge carries a traversal time in game-minutes, derived from real distances in OSM data.

Each location node contains:

- Name (real Irish place name)
- Description template (dynamically enriched by LLM)
- Connections to other locations with traversal times
- Properties: indoor/outdoor, public/private
- Associated NPCs (home, workplace)
- Optional `mythological_significance` field for future use

Movement is expressed in natural language and resolved to graph traversal. While the player moves between nodes, game time advances by the edge's traversal time, and the simulation ticks forward accordingly. Encounters may occur en route.

The map is a **static authored data file** (JSON or SQLite). Geography never changes; only the people and events within it are dynamic.

## Consequences

**Positive:**

- Natural language movement maps cleanly to named destinations ("go to the pub" resolves to a node)
- Variable resolution is straightforward: dense node graphs for nearby areas, sparse for distant ones
- No spatial collision system, physics, or coordinate math needed
- Traversal times create natural pacing and simulation windows during movement
- Each node is a self-contained context for NPC interactions and descriptions

**Negative:**

- Cannot do continuous pathfinding or free-roaming exploration between nodes
- Limited spatial relationships: no "the church is to the left of the pub" without explicit encoding
- Adding new locations requires authoring new nodes and edges rather than placing points on a map
- En-route encounters need special handling since there is no intermediate space between nodes

## Alternatives Considered

- **Tile or hex grid**: Standard for graphical games but adds overhead with no benefit for a text-based game. Spatial precision is wasted when all interaction is through language.
- **Continuous 2D coordinate space**: Unnecessary complexity for a text adventure. Would require pathfinding, coordinate-based queries, and spatial indexing with no player-facing benefit.
- **Procedural generation**: Would lose the authenticity of real Irish geography, which is a core design goal. Real townlands, roads, and landmarks ground the world in a specific sense of place.

## Related

- [docs/design/world-geography.md](../design/world-geography.md)
- [ADR-009: Real Geography, Fictional People](009-real-geography-fictional-people.md)
