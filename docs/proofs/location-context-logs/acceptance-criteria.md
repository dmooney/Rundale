# Acceptance Criteria — location-context-logs

Parallel feature to `character-context-logs` (#1010). Every location in
`WorldState::graph` gets a markdown file on disk under
`<user-data-dir>/<app>/logs/branch-<id>/` named `loc-NNN-slug.md`. File
opens with a stable PROFILE section and grows an append-only JOURNAL of
events that happened at that location.

## Criteria

- **C1.** Every location in the loaded mod gets one `loc-NNN-<slug>.md`
  file on session start. File count equals
  `WorldGraph::location_count()`.
- **C2.** Each file contains a PROFILE section, bounded by
  `<!-- PROFILE_START -->` / `<!-- PROFILE_END -->`, that includes:
  - Name as H1 (`# <name> — Location Log`)
  - Indoor/outdoor and public/private flags
  - Description template (verbatim from the world graph)
  - Geography line with coordinates, geo-kind, aliases, source
  - Mythological-significance section iff the field is non-empty
  - Connections list with path description + hazard tag when present
  - Residents list iff `associated_npcs` is non-empty
- **C3.** `PlayerMoved { to: loc_id }` appends a `### <timestamp> —
  Player arrived` heading + `*Arrived from <prev>*` body to the
  destination's file. Duplicate consecutive arrivals at the same
  location are suppressed.
- **C4.** `NpcArrived { npc_id, location }` appends a `### <timestamp>
  — <name> arrived` heading to that location's file. `NpcDeparted`
  appends a parallel `<name> departed` heading.
- **C5.** Log directory is branch-scoped (`logs/branch-<id>/`) so the
  same world on a different save branch writes to its own folder.
- **C6.** Feature is gated by the `location-logs` flag (default on).
  Disabling the flag at startup must produce no files and no errors.

## Verification fixture

`parish/testing/fixtures/play_location-context-logs.txt` — drives a
short walk that fires `PlayerMoved` between two locations.
