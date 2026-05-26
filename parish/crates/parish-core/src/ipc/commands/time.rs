//! Time-control commands: pause, resume, status, speed, wait, tick.

use chrono::Timelike;

use crate::input::Command;
use crate::npc::manager::NpcManager;
use crate::world::WorldState;

use super::CommandResult;

/// Handle time-control commands: pause/resume clock, status, speed, wait, tick.
pub(super) fn handle_time_control_command(
    cmd: Command,
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
) -> CommandResult {
    match cmd {
        Command::Pause => {
            world.clock.pause();
            CommandResult::text("The clocks of the parish stand still.")
        }
        Command::Resume => {
            world.clock.resume();
            CommandResult::text("Time stirs again in the parish.")
        }
        Command::Status => {
            let tod = world.clock.time_of_day();
            let season = world.clock.season();
            let loc = world.current_location().name.clone();
            let paused = if world.clock.is_paused() {
                " (paused)"
            } else {
                ""
            };
            CommandResult::text(format!(
                "Location: {} | {} | {}{}",
                loc, tod, season, paused
            ))
        }
        Command::ShowSpeed => {
            let s = world
                .clock
                .current_speed()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Custom ({}x)", world.clock.speed_factor()));
            CommandResult::text(format!("Speed: {}", s))
        }
        Command::SetSpeed(speed) => {
            world.clock.set_speed(speed);
            CommandResult::text(speed.activation_message())
        }
        Command::InvalidSpeed(name) => CommandResult::text(format!(
            "Unknown speed '{}'. Try: slow, normal, fast, fastest, ludicrous.",
            name
        )),
        Command::Wait(minutes) => {
            world.clock.advance(minutes as i64);
            npc_manager.assign_tiers(world, &[]);
            let _events = npc_manager.tick_schedules(
                &world.clock,
                &world.graph,
                world.weather,
                &world.event_bus,
            );
            let now = world.clock.now();
            let tod = world.clock.time_of_day();
            CommandResult::text(format!(
                "You wait for {} minutes...\nIt is now {:02}:{:02} {}.",
                minutes,
                now.hour(),
                now.minute(),
                tod
            ))
        }
        Command::Tick => {
            npc_manager.assign_tiers(world, &[]);
            let events = npc_manager.tick_schedules(
                &world.clock,
                &world.graph,
                world.weather,
                &world.event_bus,
            );
            let count = events.len();
            if count == 0 {
                CommandResult::text("No NPC activity.")
            } else {
                CommandResult::text(format!("{} schedule event(s) processed.", count))
            }
        }
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}
