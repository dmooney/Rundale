//! Time-control commands: pause, resume, status, speed, wait, tick.

use chrono::Timelike;
use parish_types::minute_word;

use crate::input::Command;
use crate::ipc::config::GameConfig;
use crate::npc::manager::NpcManager;
use crate::world::WorldState;

use super::CommandResult;

/// Handle time-control commands: pause/resume clock, status, speed, wait, tick.
///
/// `config` is used to check the `focus-auto-pause` feature flag for
/// [`Command::PauseSilent`] and [`Command::ResumeSilent`].
pub(super) fn handle_time_control_command(
    cmd: Command,
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    config: &GameConfig,
) -> CommandResult {
    match cmd {
        Command::Pause => {
            // Only announce on the running->paused edge. The frontend
            // auto-pause tracker dispatches /pause repeatedly on user-idle
            // edges; without this gate a redundant /pause re-emits the system
            // line, producing the duplicate messages the demo audit caught
            // (TODO #6 / #31). An empty response is not emitted.
            if world.clock.is_paused() {
                CommandResult::text("")
            } else {
                world.clock.pause();
                CommandResult::text("The clocks of the parish stand still.")
            }
        }
        Command::Resume => {
            // Symmetric edge-gating: only announce on the paused->running edge.
            if world.clock.is_paused() {
                world.clock.resume();
                CommandResult::text("Time stirs again in the parish.")
            } else {
                CommandResult::text("")
            }
        }
        Command::PauseSilent => {
            // Focus/visibility-driven pause: clock freezes but no message is
            // emitted.  The edge-gate still applies — a redundant silent pause
            // while already paused is a no-op with empty response.
            //
            // When the `focus-auto-pause` flag is explicitly disabled (e.g. by
            // the QA harness), this command is a no-op so that focus events
            // cannot silently toggle game time (#1357).
            if !config.flags.is_disabled("focus-auto-pause") && !world.clock.is_paused() {
                world.clock.pause();
            }
            CommandResult::text("")
        }
        Command::ResumeSilent => {
            // Focus/visibility-driven resume: clock restarts but no message is
            // emitted.  The edge-gate still applies.
            //
            // Skipped when `focus-auto-pause` is explicitly disabled (#1357).
            if !config.flags.is_disabled("focus-auto-pause") && world.clock.is_paused() {
                world.clock.resume();
            }
            CommandResult::text("")
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
                "You wait for {} {}...\nIt is now {:02}:{:02} {}.",
                minutes,
                minute_word(minutes),
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
