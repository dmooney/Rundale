# Plan: Phase 5F — World Graph Expansion

> Parent: [Phase 5](phase-5-full-lod-scale.md) | [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Planned**
>
> **Depends on:** Phase 5D (Tier 3/4 operational — distant locations need batch/rules simulation)
> **Depended on by:** None (can be done in parallel with 5E if tier assignment is ready)

## Goal

Expand the world graph beyond the starting parish to include Roscommon town, Athlone, and Dublin, with decreasing detail at greater distances. Add NPCs for new locations and validate the full LOD system at scale.

## Tasks

### 1. New location data files

Create or extend data files with new location nodes:

#### Roscommon Town (`data/roscommon.json`) — ~10 nodes

| ID | Name | Indoor | Description |
|----|------|--------|-------------|
| 101 | Main Street | no | The main thoroughfare of Roscommon town, cobblestoned and busy |
| 102 | Market Square | no | Open square where weekly markets are held |
| 103 | County Hospital | yes | The county infirmary, a solid stone building |
| 104 | Train Station | yes | A small station on the Midland Great Western line |
| 105 | Roscommon Castle | no | The ruined Norman castle on the edge of town |
| 106 | Abbey Hotel | yes | A coaching inn and hotel near the town centre |
| 107 | GAA Grounds | no | The local Gaelic games pitch |
| 108 | Industrial Estate | no | Workshops, a tannery, and small businesses |
| 109 | Library | yes | The lending library maintained by the parish |
| 110 | Shopping Centre | yes | A row of shops along Market Square |

#### Athlone (`data/athlone.json`) — ~5 nodes

| ID | Name | Indoor | Description |
|----|------|--------|-------------|
| 201 | Town Centre | no | The bustling centre of Athlone, a garrison town |
| 202 | Athlone Castle | no | The imposing castle guarding the Shannon crossing |
| 203 | Shannon Bridge | no | The stone bridge spanning the River Shannon |
| 204 | Luan Gallery | yes | A small exhibition hall near the river |
| 205 | Athlone Station | yes | Railway station connecting east and west |

#### Dublin (`data/dublin.json`) — ~5 nodes

| ID | Name | Indoor | Description |
|----|------|--------|-------------|
| 301 | O'Connell Street | no | The wide main boulevard of the capital |
| 302 | Trinity College | yes | The ancient university, seat of learning |
| 303 | Heuston Station | yes | The western railway terminus |
| 304 | Phoenix Park | no | The vast royal park on the city's edge |
| 305 | Temple Bar | yes | A warren of narrow streets with pubs and lodgings |

#### Lat/Lon coordinates

Each location needs `lat` and `lon` fields for the SVG map:

- Roscommon: ~53.63°N, -8.19°W
- Athlone: ~53.42°N, -7.94°W
- Dublin: ~53.35°N, -6.26°W

### 2. Inter-region connections

| From | To | Travel time (game minutes) |
|------|----|----|
| Kiltoom Crossroads (1) | Roscommon Main Street (101) | 30 |
| Roscommon Train Station (104) | Athlone Station (205) | 40 |
| Athlone Station (205) | Heuston Station (303) | 120 |

Within each region, all nodes are interconnected with 5-15 minute travel times.

### 3. Multi-file world graph loading

Extend `WorldGraph::load_from_file` or add a new method:

```rust
impl WorldGraph {
    /// Loads and merges multiple region data files into one graph.
    pub fn load_from_files(paths: &[&Path]) -> Result<Self, ParishError>;
}
```

Alternatively, create a single `data/world.json` that includes all regions. The multi-file approach is preferred for maintainability.

### 4. New NPCs for expanded locations

Add 5-10 NPCs for Roscommon (shopkeepers, a doctor, a solicitor, a priest) and 2-3 for Athlone. Dublin NPCs are optional (Tier 4 only).

These NPCs will primarily operate at Tier 3/4 and only inflate to Tier 1/2 if the player travels there.

### 5. Validate LOD at scale

With ~25+ total NPCs across 4 regions:

- Verify tier assignment distributes correctly (most NPCs at Tier 3/4 when player is in the parish).
- Verify Tier 3 batch inference handles the volume.
- Verify tier transitions work when player travels to Roscommon (NPCs inflate).
- Profile memory usage and inference latency.

### 6. Update description templates

New locations need description templates with `{time}`, `{weather}`, `{npcs_present}` placeholders, consistent with existing parish locations.

### 7. Travel narration for long journeys

Extend `movement.rs` to generate multi-segment travel narration for journeys > 30 minutes:

- "You set off along the road toward Roscommon..."
- Intermediate narration at the halfway point.
- "After a long walk, you arrive at Roscommon Main Street."

## Tests

| Test | What it verifies |
|------|------------------|
| `test_load_roscommon_data` | Roscommon JSON parses correctly, all 10 nodes present |
| `test_load_athlone_data` | Athlone JSON parses correctly |
| `test_load_dublin_data` | Dublin JSON parses correctly |
| `test_multi_file_graph_merge` | All regions merge into one graph with correct connections |
| `test_cross_region_pathfinding` | BFS finds path from parish to Dublin via Roscommon + Athlone |
| `test_cross_region_travel_time` | Parish to Dublin total travel time = 30+40+120 = 190 minutes |
| `test_tier_assignment_cross_region` | NPCs in Dublin are Tier 4 when player is in parish |
| `test_tier_inflation_on_travel` | Traveling to Roscommon inflates Roscommon NPCs to Tier 1/2 |
| `test_long_journey_narration` | Travel > 30 min produces multi-segment narration |
| `test_location_ids_unique` | No duplicate location IDs across all data files |

## Acceptance Criteria

- World graph includes 4 regions with ~35 total locations
- Cross-region pathfinding works correctly
- Travel times are realistic (30 min to Roscommon, 3+ hours to Dublin)
- New NPCs operate at appropriate cognitive tiers
- Tier transitions work seamlessly when player travels between regions
- All location data includes lat/lon for SVG map rendering
- All tests passing
