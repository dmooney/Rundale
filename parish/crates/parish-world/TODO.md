# parish-world — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Duplication | P2 | `src/graph.rs:382-476` | `shortest_path` and `shortest_path_filtered` share ~90% of their BFS implementation (visited set, queue, predecessors, path reconstruction). The only difference is edge filtering. The unfiltered variant should delegate to the filtered one with an always-true closure, or both should call a shared helper. |
| TD-002 | Duplication | P2 | `src/lib.rs:87-238` | `WorldState::new()`, `from_parish_file()`, and `from_mod_params()` repeat identical struct field initializations for `text_log`, `event_bus`, `visited_locations`, `edge_traversals`, `gossip_network`, `conversation_log`, `player_name`, and `tick_generation`. The `from_mod_params` also duplicates the graph→legacy-locations loop from `from_parish_file` verbatim. |
| TD-003 | Duplication | P2 | `src/encounter.rs:55-63` `src/encounter.rs:89-97` | `check_encounter_with_config` and `check_encounter_with_table` contain identical 7-arm `match` blocks mapping `TimeOfDay` to `EncounterConfig` thresholds (dawn/morning/midday/afternoon/dusk/night/midnight). |
| TD-004 | Duplication | P2 | `src/movement.rs:123-131` `src/movement.rs:170-177` | `resolve_movement` and `resolve_movement_with_weather` duplicate the first ~14 lines: `find_by_name` call, `AlreadyHere` check, and the identity-comparison guard. |
| TD-005 | Complexity | P2 | `src/graph.rs:300-373` | `find_by_name_with_config` uses an unrolled 7-level priority chain (exact name → alias → substring → article-strip → fuzzy) with magic sentinel `u8::MAX`. The priority levels 1–7 are not enumerated — adding a matching tier requires editing numbered branches in three places. |
| TD-006 | Complexity | P2 | `src/movement.rs:163-258` | `resolve_movement_with_weather` is ~95 lines spanning filtered pathfinding, weather effect evaluation per edge, per-edge time computation with slowdown, narration building, and a fallback path for unreachable destinations. The "shouldn't happen" fallback at line 210–221 is particularly hard to follow. |
| TD-007 | Weak Tests | P1 | `src/graph.rs:382-431` | `shortest_path_filtered` has no direct unit tests in graph.rs. It is only exercised indirectly through `resolve_movement_with_weather` tests in movement.rs (hazard_graph, storm reroute, blocked-when-no-alternative). Missing coverage on edge cases: empty filter, cycle-without-target, filter that passes only the target node, filter that oscillates between reject/accept. |
| TD-008 | Dead Code | P3 | `src/graph.rs:85-87` | `Connection.traversal_minutes` is annotated `#[serde(skip_serializing)]` and the doc explicitly marks it "Legacy field — ignored at runtime." The field is never read or written by any function body. It exists only because old test and fixture JSON files may still include it. |
| TD-009 | Config/Cargo | P3 | `Cargo.toml:14-15` | `anyhow` and `thiserror` are declared as dependencies but are not directly used in any source file. Error handling in this crate exclusively uses `parish_types::ParishError`. |
| TD-010 | Config/Cargo | P2 | `Cargo.toml:21` `src/transport.rs:109` | `toml` is listed as a regular dependency but is only called from `#[cfg(test)]` code in `transport.rs` tests (`toml::from_str`). The `TransportConfig` struct only derives serde traits; production deserialization is the caller's responsibility. `toml` should be a `[dev-dependencies]` entry. |
| TD-011 | Stale Docs/Comments | P3 | `README.md:14` | README lists `palette` as a module under "Key modules," but `parish-world` has no `palette` module — `parish-palette` is a separate leaf crate. The module list is also missing `session`, `wayfarers`, and `weather_travel`. |

## In Progress

*(none)*

## Done

*(none)*
