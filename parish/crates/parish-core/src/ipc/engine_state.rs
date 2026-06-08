//! Canonical, deterministic engine-state snapshot — backend-agnostic.
//!
//! The MCP automated-QA loop (#1331) needs a programmatic way to read the
//! *authoritative* Parish engine state so an agent can assert that the UI
//! layer actually resolved a state transition. The world snapshot and map
//! data are framing-specific DTOs tuned for the frontend; this module folds
//! the load-bearing engine facts into one flat, deterministic JSON object —
//! the analogue of the issue's illustrative `active_scene` /
//! `village_grapevine_status`, mapped to the real schema.
//!
//! It is built from the same live `WorldState` + `NpcManager` references the
//! other read handlers use, so it can never drift from what the player sees
//! (rule #12). Every entry point (`parish-server`, `parish-tauri` bridge)
//! exposes it through the existing `tauri_invoke` / HTTP command seam, and the
//! MCP `parish_engine_state` tool maps onto it.
//!
//! Determinism: given identical world + NPC state the output is byte-identical.
//! Fields derived from wall-clock time (real-elapsed seconds, etc.) are
//! deliberately excluded — this is the *game* state, not a timing probe.

use serde::{Deserialize, Serialize};

use crate::ipc::handlers::weekday_name;
use crate::npc::manager::NpcManager;
use crate::world::WorldState;
use chrono::{Datelike, Timelike};

/// The player's current scene — location identity the engine resolved.
///
/// Named `active_scene` to match the issue's illustrative schema while
/// carrying the real location id + name the world graph holds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveScene {
    /// Player's current location id.
    pub location_id: u32,
    /// Player's current location name.
    pub location_name: String,
    /// Whether the location is indoors.
    pub indoor: bool,
}

/// Deterministic game-clock facts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineClock {
    /// Game hour (0–23).
    pub hour: u8,
    /// Game minute (0–59).
    pub minute: u8,
    /// Time-of-day label (e.g. "Morning").
    pub time_label: String,
    /// Full day-of-week name (e.g. "Monday").
    pub day_of_week: String,
    /// Schedule day-type label (e.g. "Weekday", "Market Day").
    pub day_type: String,
    /// Season label.
    pub season: String,
    /// Festival name if today is a festival, else null.
    pub festival: Option<String>,
    /// Whether the clock is player-paused.
    pub paused: bool,
    /// Whether the clock is frozen waiting on inference.
    pub inference_paused: bool,
}

/// Player-centric engine facts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerState {
    /// Player's current location id.
    pub location_id: u32,
    /// Number of locations the player has visited.
    pub visited_count: usize,
    /// Player's name, once introduced (else null).
    pub name: Option<String>,
}

/// A single co-located NPC, identity-resolved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NpcStatus {
    /// Stable NPC id.
    pub id: u32,
    /// Canonical real name (stable key).
    pub real_name: String,
    /// Display name (brief description until introduced).
    pub display_name: String,
    /// Current mood label.
    pub mood: String,
    /// Whether the player has been introduced.
    pub introduced: bool,
}

/// Aggregate NPC status for the current scene + roster totals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NpcStateSummary {
    /// NPCs co-located with the player right now.
    pub here: Vec<NpcStatus>,
    /// Total NPCs in the roster.
    pub total: usize,
    /// Number of NPCs the player has been introduced to.
    pub introduced: usize,
}

/// Gossip-network status — the real analogue of the issue's
/// `village_grapevine_status`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrapevineStatus {
    /// Total number of gossip items circulating.
    pub item_count: usize,
    /// Highest distortion level across all items (0 = none distorted).
    pub max_distortion: u8,
    /// Number of items that have been distorted at least once.
    pub distorted_item_count: usize,
}

/// The canonical, deterministic Parish engine state for QA validation.
///
/// Flat by design: an agent compares each field against what the UI rendered.
/// Serialised snake_case to match every other Parish IPC type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineState {
    /// Schema version, so consumers can detect shape changes.
    pub schema_version: u32,
    /// Player's current scene (location identity).
    pub active_scene: ActiveScene,
    /// Game-clock facts.
    pub clock: EngineClock,
    /// Current weather label.
    pub weather: String,
    /// Player-centric state.
    pub player: PlayerState,
    /// NPC roster + co-located status.
    pub npcs: NpcStateSummary,
    /// Gossip-network ("village grapevine") status.
    pub grapevine: GrapevineStatus,
}

/// Current schema version of [`EngineState`]. Bump on any breaking change.
pub const ENGINE_STATE_SCHEMA_VERSION: u32 = 1;

/// Builds the canonical engine state from live world + NPC references.
///
/// Pure given its inputs (no I/O, no wall-clock reads beyond the game clock),
/// so it is trivially unit-testable and deterministic.
pub fn build_engine_state(world: &WorldState, npc_manager: &NpcManager) -> EngineState {
    let now = world.clock.now();
    let loc = world.current_location();
    let indoor = world
        .current_location_data()
        .map(|d| d.indoor)
        .unwrap_or(loc.indoor);

    let active_scene = ActiveScene {
        location_id: world.player_location.0,
        location_name: loc.name.clone(),
        indoor,
    };

    let clock = EngineClock {
        hour: now.hour() as u8,
        minute: now.minute() as u8,
        time_label: world.clock.time_of_day().to_string(),
        day_of_week: weekday_name(now.weekday()).to_string(),
        day_type: world.clock.day_type().to_string(),
        season: world.clock.season().to_string(),
        festival: world.clock.check_festival().map(|f| f.to_string()),
        paused: world.clock.is_paused(),
        inference_paused: world.clock.is_inference_paused(),
    };

    let player = PlayerState {
        location_id: world.player_location.0,
        visited_count: world.visited_locations.len(),
        name: world.player_name.clone(),
    };

    let here: Vec<NpcStatus> = npc_manager
        .npcs_at(world.player_location)
        .into_iter()
        .map(|npc| NpcStatus {
            id: npc.id.0,
            real_name: npc.name.clone(),
            display_name: npc_manager.display_name(npc).to_string(),
            mood: npc.mood.clone(),
            introduced: npc_manager.is_introduced(npc.id),
        })
        .collect();

    let npcs = NpcStateSummary {
        here,
        total: npc_manager.npc_count(),
        introduced: npc_manager.introduced_count(),
    };

    let items = world.gossip_network.all_items();
    let max_distortion = items.iter().map(|i| i.distortion_level).max().unwrap_or(0);
    let distorted_item_count = items.iter().filter(|i| i.distortion_level > 0).count();
    let grapevine = GrapevineStatus {
        item_count: world.gossip_network.len(),
        max_distortion,
        distorted_item_count,
    };

    EngineState {
        schema_version: ENGINE_STATE_SCHEMA_VERSION,
        active_scene,
        clock,
        weather: world.weather.to_string(),
        player,
        npcs,
        grapevine,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::manager::NpcManager;
    use crate::world::WorldState;

    #[test]
    fn engine_state_has_canonical_shape() {
        let world = WorldState::new();
        let npc_manager = NpcManager::new();
        let state = build_engine_state(&world, &npc_manager);

        // active_scene maps the real location.
        assert_eq!(state.active_scene.location_id, world.player_location.0);
        assert_eq!(state.active_scene.location_name, "The Crossroads");
        assert!(!state.active_scene.indoor);

        // Clock fields are populated.
        assert_eq!(state.clock.hour, 8);
        assert_eq!(state.clock.minute, 0);
        assert!(!state.clock.time_label.is_empty());
        assert!(!state.clock.day_of_week.is_empty());
        assert!(!state.clock.season.is_empty());

        // Player state.
        assert_eq!(state.player.location_id, world.player_location.0);
        assert!(state.player.name.is_none());

        // NPC roster is empty in a bare world.
        assert!(state.npcs.here.is_empty());
        assert_eq!(state.npcs.total, 0);

        // Grapevine starts empty.
        assert_eq!(state.grapevine.item_count, 0);
        assert_eq!(state.grapevine.max_distortion, 0);

        assert_eq!(state.schema_version, ENGINE_STATE_SCHEMA_VERSION);
    }

    #[test]
    fn engine_state_is_deterministic() {
        let world = WorldState::new();
        let npc_manager = NpcManager::new();
        let a = build_engine_state(&world, &npc_manager);
        let b = build_engine_state(&world, &npc_manager);
        assert_eq!(a, b, "identical state must produce identical snapshots");
    }

    #[test]
    fn engine_state_serializes_snake_case_with_expected_keys() {
        let world = WorldState::new();
        let npc_manager = NpcManager::new();
        let state = build_engine_state(&world, &npc_manager);
        let v = serde_json::to_value(&state).unwrap();
        // Top-level keys an MCP QA agent will assert against.
        for key in [
            "schema_version",
            "active_scene",
            "clock",
            "weather",
            "player",
            "npcs",
            "grapevine",
        ] {
            assert!(v.get(key).is_some(), "missing top-level key: {key}");
        }
        assert!(v["active_scene"].get("location_name").is_some());
        assert!(v["clock"].get("day_of_week").is_some());
        assert!(v["grapevine"].get("item_count").is_some());
    }
}
