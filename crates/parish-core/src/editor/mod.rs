//! Parish Designer — editor support module.
//!
//! Backend for the GUI editor utility that lets game designers browse mods,
//! edit NPC and location data, validate cross-references, and inspect save
//! files. The editor operates on a **fresh in-memory copy loaded from disk**
//! and never touches the live gameplay [`GameMod`](crate::game_mod::GameMod)
//! or [`AppState`]; see `docs/design/designer-editor.md` for the full design.
//!
//! Unlike [`GameMod::load`](crate::game_mod::GameMod::load), which is
//! all-or-nothing, the editor loads each mod file independently via
//! [`mod_io::load_mod_snapshot`] so a broken `festivals.json` doesn't hide a
//! working `npcs.json`. Post-save revalidation uses
//! [`validate::validate_snapshot`], not `GameMod::load`, for the same reason.

pub mod format;
pub mod live_reload;
#[cfg(test)]
mod maintenance_tool;
pub mod mod_io;
pub mod persist;
pub mod save_inspect;
pub mod types;
pub mod validate;

pub use format::write_json_deterministic;
pub use live_reload::reload_world_graph_preserving_runtime;
pub use mod_io::{list_mods, load_mod_snapshot};
pub use persist::{
    SaveResult, save_anachronisms, save_encounters, save_festivals, save_mod, save_npcs, save_world,
};
pub use save_inspect::{
    BranchSummary, SaveFileSummary, SnapshotDetail, SnapshotSummary, list_branches, list_saves,
    list_snapshots, read_latest_snapshot,
};
pub use types::{
    EditorModSnapshot, ModSummary, ValidationIssue, ValidationReport, ValidationSeverity,
};
pub use validate::validate_snapshot;
