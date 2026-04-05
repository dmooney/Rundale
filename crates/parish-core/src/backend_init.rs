//! Shared backend bootstrapping helpers.
//!
//! Centralizes world/NPC/mod loading for all runtime backends
//! (headless CLI, test harness, Tauri, and web server).

use std::path::Path;

use crate::game_mod::{GameMod, find_default_mod};
use crate::npc::{Npc, manager::NpcManager};
use crate::world::{LocationId, WorldState};

/// Fallback behavior to use if NPC data cannot be loaded from disk.
#[derive(Debug, Clone, Copy)]
pub enum NpcFallback {
    /// Return an empty [`NpcManager`].
    Empty,
    /// Seed manager with one deterministic test NPC.
    TestNpc,
}

/// Result of backend bootstrap: optional mod plus initialized world + NPCs.
pub struct BackendBootstrap {
    pub game_mod: Option<GameMod>,
    pub world: WorldState,
    pub npc_manager: NpcManager,
}

/// Loads the default game mod if available, then initializes world + NPCs.
///
/// If mod loading fails, falls back to legacy `data/` JSON files.
/// If those fail too, falls back to default empty world and NPC fallback policy.
pub fn bootstrap_default_mod_or_data(
    data_dir: &Path,
    start_location: LocationId,
    npc_fallback: NpcFallback,
) -> BackendBootstrap {
    let game_mod = find_default_mod().and_then(|dir| GameMod::load(&dir).ok());
    let (world, npc_manager) =
        load_world_and_npcs(game_mod.as_ref(), data_dir, start_location, npc_fallback);

    BackendBootstrap {
        game_mod,
        world,
        npc_manager,
    }
}

/// Loads world + NPCs from either a game mod or legacy `data/` files.
pub fn load_world_and_npcs(
    game_mod: Option<&GameMod>,
    data_dir: &Path,
    start_location: LocationId,
    npc_fallback: NpcFallback,
) -> (WorldState, NpcManager) {
    let world = game_mod
        .and_then(|gm| WorldState::from_mod(gm).ok())
        .or_else(|| {
            WorldState::from_parish_file(&data_dir.join("parish.json"), start_location).ok()
        })
        .unwrap_or_else(WorldState::new);

    let npcs_path = game_mod
        .map(|gm| gm.npcs_path())
        .unwrap_or_else(|| data_dir.join("npcs.json"));

    let mut npc_manager = NpcManager::load_from_file(&npcs_path).unwrap_or_else(|_| {
        let mut manager = NpcManager::new();
        if matches!(npc_fallback, NpcFallback::TestNpc) {
            manager.add_npc(Npc::new_test_npc());
        }
        manager
    });

    npc_manager.assign_tiers(&world, &[]);

    (world, npc_manager)
}
