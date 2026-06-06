//! World graph — location graph with connections, pathfinding, and data loading.
//!
//! The world is a graph of named location nodes connected by edges
//! with traversal times. This module provides the `WorldGraph` container,
//! `Connection` edges, and BFS pathfinding.
//!
//! # Submodule layout
//!
//! | Submodule        | Contents                                                            |
//! |------------------|---------------------------------------------------------------------|
//! | `schema`         | Type definitions: `GeoKind`, `RelativeRef`, `Hazard`, `Connection`,|
//! |                  | `LocationData`, `WorldGraph` (struct + `new`/`Default`),            |
//! |                  | `WorldGraphFile` (JSON wrapper)                                     |
//! | `loader`         | `load_from_file`, `load_from_str`, `validate`                       |
//! | `lookup`         | `get`, `neighbors`, `find_by_name`, `find_by_name_with_config`,     |
//! |                  | `connection_between`, `location_count`, `location_ids`              |
//! | `pathfinding`    | `shortest_path`, `shortest_path_filtered`, `edge_travel_minutes`,   |
//! |                  | `path_travel_time`, `travel_times_from`, `hop_distances`            |

mod loader;
mod lookup;
mod pathfinding;
mod schema;

// Re-export the full public API so that all call sites outside this module
// continue to use the same paths (e.g. `parish_world::graph::WorldGraph`).
pub use schema::{Connection, GeoKind, Hazard, LocationData, RelativeRef, WorldGraph};
