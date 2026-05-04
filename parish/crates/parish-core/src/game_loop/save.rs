//! Shared save-game and new-game helpers (#696).
//!
//! Extracts the pure "load fresh world + NPCs" computation that was duplicated
//! across `parish-server/src/routes.rs` (`do_new_game_inner`) and
//! `parish-tauri/src/commands.rs` (`do_new_game`).
//!
//! The persistence side effects (opening the DB, saving a snapshot, updating
//! `save_path` / `branch_id` / `branch_name` on AppState) remain per-runtime
//! because:
//! - Both runtimes use different `AppState` concrete types.
//! - Both use `spawn_blocking + Database::open` via `parish-persistence` — the
//!   shared `SessionStore` trait exists (#614) but is not yet wired to the
//!   command-handler paths (deferred; would require threading `Arc<dyn SessionStore>`
//!   through each AppState variant).
//!
//! Headless CLI continues to use its own inline `handle_headless_new_game`
//! (App struct not yet on `Arc<Mutex<T>>`; see module-level comment in
//! `game_loop/mod.rs`).
//!
//! # Architecture gate
//!
//! This module is backend-agnostic — it imports only `parish-core` types.
//! It must not import `axum`, `tauri`, or any crate in
//! `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use std::path::Path;

use crate::game_mod::GameMod;
use crate::npc::manager::NpcManager;
use crate::world::{DEFAULT_START_LOCATION, WorldState};

/// Loads a fresh [`WorldState`] and [`NpcManager`] for a new game.
///
/// Prefers the active game mod when `game_mod` is `Some`. Falls back to
/// legacy data files under `data_dir` when no mod is active.
///
/// This is a pure, synchronous operation — it reads from disk but does not
/// acquire any async locks or interact with any AppState.  Callers are
/// responsible for swapping the results into their live state under locks.
///
/// # Errors
///
/// Returns `Err(String)` if the world data cannot be loaded.  NPC load
/// failures are treated as soft errors (a warning is logged and an empty
/// `NpcManager` is returned).
///
/// # Parameters
///
/// - `game_mod`: the active game mod, if any.
/// - `data_dir`: legacy fallback data directory (used only when `game_mod` is
///   `None`).
pub fn load_fresh_world_and_npcs(
    game_mod: Option<&GameMod>,
    data_dir: &Path,
) -> Result<(WorldState, NpcManager), String> {
    // Prefer the game mod; fall back to legacy parish.json / world.json.
    let (world, npcs_path) = if let Some(gm) = game_mod {
        let world = crate::game_mod::world_state_from_mod(gm)
            .map_err(|e| format!("Failed to load world from mod: {}", e))?;
        (world, gm.npcs_path())
    } else {
        let parish = data_dir.join("parish.json");
        let world_path = if parish.exists() {
            parish
        } else {
            data_dir.join("world.json")
        };
        let world = WorldState::from_parish_file(&world_path, DEFAULT_START_LOCATION)
            .map_err(|e| format!("Failed to load world data from {:?}: {}", world_path, e))?;
        (world, data_dir.join("npcs.json"))
    };

    let npc_manager = NpcManager::load_from_file(&npcs_path).unwrap_or_else(|e| {
        tracing::warn!(
            path = %npcs_path.display(),
            error = %e,
            "load_fresh_world_and_npcs: failed to load NPCs; starting with empty manager",
        );
        NpcManager::new()
    });

    Ok((world, npc_manager))
}
