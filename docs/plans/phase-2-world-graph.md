# Plan: Phase 2 — World Graph

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Complete** (OSM extraction deferred as stretch goal)

## Goal

Replace the single test location with a graph of real-world-inspired locations for a parish near Roscommon, implement movement between nodes with traversal time, and advance the game clock during travel.

## Prerequisites

- Phase 1 complete: working TUI, GameClock, input parsing, Ollama integration
- Parish location selected (see [Open Questions](./open-questions.md) #1)

## Tasks

1. **Extend `Location` struct in `src/world/mod.rs`**
   - Add fields: `description_template: String` (contains `{time}`, `{weather}`, `{npcs_present}` placeholders), `connections: Vec<Connection>`, `associated_npcs: Vec<NpcId>`, `mythological_significance: Option<String>`
   - `Connection` struct: `target: LocationId`, `traversal_minutes: u16`, `path_description: String` (e.g., "a narrow boreen lined with hawthorn")

2. **Implement `WorldGraph` in `src/world/mod.rs`**
   - `WorldGraph` struct: `locations: HashMap<LocationId, Location>`, helper methods
   - `fn get(&self, id: LocationId) -> Option<&Location>`
   - `fn neighbors(&self, id: LocationId) -> Vec<(LocationId, &Connection)>`
   - `fn find_by_name(&self, name: &str) -> Option<LocationId>` — case-insensitive fuzzy match (contains)
   - `fn shortest_path(&self, from: LocationId, to: LocationId) -> Option<Vec<LocationId>>` — BFS on unweighted graph for basic pathfinding

3. **Create hand-authored parish data file: `data/parish.json`**
   - 12-15 locations: The Crossroads (hub), Darcy's Pub, St. Brigid's Church, Post Office, National School, GAA Pitch, Lough Shore, Bridge over the Hind River, Murphy's Farm, O'Brien's Farm, The Fairy Fort, Bog Road, Connolly's Shop, The Creamery
   - Each with 2-4 connections to neighbors, realistic traversal times (2-15 minutes)
   - Description templates reflecting rural Roscommon character

4. **Implement JSON deserialization for `WorldGraph`**
   - `impl WorldGraph { fn load_from_file(path: &Path) -> Result<Self> }` — read and deserialize `data/parish.json`
   - Derive `Serialize, Deserialize` on `Location`, `Connection`, `WorldGraph`
   - Validate on load: all connection targets exist, no orphan nodes

5. **Create OSM extraction tool (stretch goal): `src/bin/osm_extract.rs`**
   - Reads Geofabrik `.pbf` file for Ireland, filters to bounding box around target parish
   - Extracts: named places, roads, buildings, waterways
   - Outputs `parish.json` in the WorldGraph format
   - Depends on `osmpbf` crate (add to `[dev-dependencies]` or as optional feature)
   - This is a tooling task, not required for the game to run

6. **Implement movement command handling in `src/input/mod.rs`**
   - Extend `IntentKind` with `Move` variant
   - `fn resolve_movement(intent: &PlayerIntent, graph: &WorldGraph, current: LocationId) -> Result<MovementResult>`
   - `MovementResult` enum: `Arrived(LocationId, u16)`, `NotFound(String)`, `AlreadyHere`
   - Fuzzy matching: if `intent.target` partially matches a connected location name, accept it

7. **Implement traversal with time advancement in game loop**
   - When `MovementResult::Arrived(dest, minutes)` is returned, call `clock.advance(minutes)`
   - Print travel narration: "You walk {path_description}. ({minutes} minutes pass.)"
   - Update `world.player_location` to destination
   - Render new location description on arrival

8. **Implement en-route encounter system in `src/world/mod.rs`**
   - `fn check_encounter(from: LocationId, to: LocationId, world: &WorldState) -> Option<EncounterEvent>`
   - `EncounterEvent` struct: `npc_id: Option<NpcId>`, `description: String`
   - Probability-based: 20% chance per traversal, weighted by time of day and weather
   - For now, encounters just add flavor text; full NPC interaction deferred to Phase 3

9. **Implement dynamic location descriptions**
   - `fn render_description(location: &Location, world: &WorldState) -> String` — interpolate template placeholders with current time, weather, present NPCs
   - For Tier 1 quality: optionally send template + context to Ollama for enrichment (`async fn enrich_description(...)`)
   - Cache enriched descriptions for 10 game-minutes to avoid redundant inference calls

10. **Write tests**
    - `test_world_graph_load`: load `data/parish.json`, assert location count, verify all connections are bidirectional
    - `test_find_by_name`: fuzzy match "pub" -> "Darcy's Pub", "church" -> "St. Brigid's Church"
    - `test_shortest_path`: verify BFS finds path between non-adjacent nodes
    - `test_movement_time_advancement`: move between nodes, assert clock advanced by correct minutes
    - `test_encounter_probability`: run 1000 encounter checks, assert ~20% hit rate within tolerance

## Design References

- [World & Geography](../design/world-geography.md)
- [Time System](../design/time-system.md)

## Key Decisions

- [ADR-001: Graph-Based World](../adr/001-graph-based-world.md)
- [ADR-007: Time Scale 20min Day](../adr/007-time-scale-20min-day.md)

## Acceptance Criteria

- `data/parish.json` contains 12-15 locations with valid connections
- Player can type "go to the pub" and arrive at Darcy's Pub after the correct traversal time
- Game clock advances during travel; time-of-day and palette shift accordingly
- Location descriptions display on arrival with time/weather context
- `cargo test` passes all world graph and movement tests
- All connections in the graph are bidirectional (validated on load)

## Resolved Issues

- **Parish selection**: Resolved as **Kiltoom** (see [open-questions.md](./open-questions.md) #1). Location data in `data/parish.json` uses Kiltoom townlands and geography (Lough Ree, Shannon, Hodson Bay).
- **Pathfinding algorithm**: Use **simple BFS** on the unweighted graph. The parish is small enough (~15-25 nodes) that weighted pathfinding provides negligible benefit. BFS finds shortest-hop paths, and traversal time is summed from edge weights along the path for clock advancement. Revisit only if the graph exceeds 50 nodes.
- **Travel narration verbosity**: Use a **single line** for movement narration in Phase 2 (e.g., "You walk along the narrow boreen to the crossroads. (8 minutes)"). LLM-enriched multi-paragraph narration is deferred to Phase 6 polish — it requires inference calls for non-NPC text, which competes with NPC cognition for Ollama throughput.
