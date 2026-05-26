//! Informational commands: about, NPCs here, time/weather details.

use chrono::Timelike;

use crate::input::Command;
use crate::npc::manager::NpcManager;
use crate::world::WorldState;

use super::CommandResult;

/// Handle informational commands: about, NPCs here, time/weather details.
pub(super) fn handle_info_command(
    cmd: Command,
    world: &WorldState,
    npc_manager: &NpcManager,
) -> CommandResult {
    match cmd {
        Command::About => CommandResult::text(
            [
                &format!(
                    "Parish v{} — a living-world text-adventure engine",
                    env!("CARGO_PKG_VERSION")
                ),
                "Set and story content come from the active base mod (see /help).",
                "",
                "Created by Dave Mooney © 2026",
                "Licensed under GNU General Public License v3.0.",
                "",
                "Type /help for available commands.",
            ]
            .join("\n"),
        ),
        Command::NpcsHere => {
            let npcs = npc_manager.npcs_at(world.player_location);
            if npcs.is_empty() {
                CommandResult::text("No one else is here.")
            } else {
                let mut lines = vec!["NPCs here:".to_string()];
                for npc in &npcs {
                    let display = npc_manager.display_name(npc);
                    let intro = if npc_manager.is_introduced(npc.id) {
                        " [introduced]"
                    } else {
                        ""
                    };
                    lines.push(format!(
                        "  {} — {} ({}){}",
                        display, npc.occupation, npc.mood, intro
                    ));
                }
                CommandResult::text(lines.join("\n"))
            }
        }
        Command::Time => {
            let now = world.clock.now();
            let tod = world.clock.time_of_day();
            let season = world.clock.season();
            let festival = world
                .clock
                .check_festival()
                .map(|f| f.to_string())
                .unwrap_or_else(|| "none".to_string());
            let paused = if world.clock.is_paused() {
                " (PAUSED)"
            } else {
                ""
            };
            CommandResult::text(format!(
                "{:02}:{:02} {} — {}{}\nWeather: {}\nSpeed: {}x\nFestival: {}",
                now.hour(),
                now.minute(),
                tod,
                season,
                paused,
                world.weather,
                world.clock.speed_factor(),
                festival
            ))
        }
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}
