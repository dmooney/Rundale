//! Snapshot serialization — captures and restores dynamic game state.
//!
//! The [`GameSnapshot`] struct is a fully serializable representation of
//! all dynamic game state. Static data (world graph, location map) is
//! excluded because it's loaded from data files on startup.
//!
//! ## Module layout
//!
//! - [`types`] — serializable data structs (`ClockSnapshot`, `NpcSnapshot`,
//!   `GameSnapshot`) and the `edge_traversals_serde` helper.
//! - [`convert`] — `NpcSnapshot::from_npc` / `NpcSnapshot::into_npc`
//!   (capture/restore of individual NPCs).
//! - [`restore`] — `GameSnapshot::capture`, `GameSnapshot::restore`, and
//!   the private sub-restore helpers.

mod convert;
mod restore;
pub mod types;

#[cfg(test)]
mod tests;

pub use types::{ClockSnapshot, GameSnapshot, NpcSnapshot};
