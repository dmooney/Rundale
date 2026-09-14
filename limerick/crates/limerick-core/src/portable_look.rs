//! Portable location rendering used by embedded and desktop runtimes.
//!
//! This module contains no IPC or inference state. The desktop command wrapper
//! re-exports the same function so every runtime renders `/look` identically.

use crate::npc::manager::NpcManager;
use crate::world::WorldState;
use crate::world::description::{format_exits, render_description};

/// Renders the current location description with NPC names and optional exits.
///
/// The caller owns locking and transport/event emission. Keeping this function
/// pure makes the local `/look` path available to portable clients without
/// constructing the desktop [`GameLoopContext`](crate::game_loop::GameLoopContext).
pub fn render_look_text(
    world: &WorldState,
    npc_manager: &NpcManager,
    speed_m_per_s: f64,
    transport_label: &str,
    include_exits: bool,
) -> String {
    let desc = if let Some(loc_data) = world.current_location_data() {
        let tod = world.clock.time_of_day();
        let weather = world.weather.to_string();
        let npc_display: Vec<String> = npc_manager
            .npcs_at(world.player_location)
            .iter()
            .map(|n| npc_manager.display_name(n).to_string())
            .collect();
        let npc_names: Vec<&str> = npc_display.iter().map(|s| s.as_str()).collect();
        render_description(loc_data, tod, &weather, &npc_names)
    } else {
        world.current_location().description.clone()
    };

    if include_exits {
        let exits = format_exits(
            world.player_location,
            &world.graph,
            speed_m_per_s,
            transport_label,
        );
        format!("{desc}\n{exits}")
    } else {
        desc
    }
}
