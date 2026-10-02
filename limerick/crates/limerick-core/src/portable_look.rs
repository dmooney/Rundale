//! Portable location rendering used by embedded and desktop runtimes.
//!
//! This module contains no IPC or inference state. The desktop command wrapper
//! re-exports the same function so every runtime renders `/look` identically.

use crate::npc::manager::NpcManager;
use crate::world::WorldState;
use crate::world::description::{format_exits, render_description, render_setting};

/// The player's current scene as a client with a status header shows it: the
/// location, its standing description, and who is there. Time of day,
/// weather, and exits are left to the header and to `/exits`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneText {
    /// Canonical location id.
    pub location_id: u32,
    /// Location name (the scene's title).
    pub name: String,
    /// Description, then a presence line when anyone is there.
    pub text: String,
}

/// Renders the player's current scene (see [`SceneText`]).
pub fn render_scene(world: &WorldState, npc_manager: &NpcManager) -> SceneText {
    let location = world.current_location();
    // In id order, so a journaled scene reads the same on every run.
    let mut present = npc_manager.npcs_at(world.player_location);
    present.sort_by_key(|npc| npc.id.0);
    let names: Vec<String> = present
        .iter()
        .map(|npc| npc_manager.display_name(npc).to_string())
        .collect();
    let setting = match world.current_location_data() {
        Some(data) => {
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            render_setting(data, &refs)
        }
        None => location.description.clone(),
    };
    let text = match presence_line(&names) {
        Some(presence) => format!("{setting}\n\n{presence}"),
        None => setting,
    };
    SceneText {
        location_id: world.player_location.0,
        name: location.name.clone(),
        text,
    }
}

/// "Peig is here.", "Mícheál and Róisín are here.", "A, B and C are here.",
/// or nothing when no one is.
fn presence_line(names: &[String]) -> Option<String> {
    let (last, rest) = names.split_last()?;
    let line = if rest.is_empty() {
        format!("{last} is here.")
    } else {
        format!("{} and {last} are here.", rest.join(", "))
    };
    Some(crate::ipc::capitalize_first(&line))
}

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
