//! Look command — renders the current location description with NPC names and exits.

use crate::npc::manager::NpcManager;
use crate::world::description::{format_exits, render_description};
use crate::world::WorldState;

/// Renders the current location description with NPC names and exits.
///
/// Returns the combined text that all backends display for a "look" command.
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
        format!("{}\n{}", desc, exits)
    } else {
        desc
    }
}
