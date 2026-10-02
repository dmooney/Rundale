//! Local slash commands a host lets players run as turns (#2120).
//!
//! The desktop runs `/`-commands on its own system-command path, outside the
//! turn API. A host that has no such path (the phone) enables these on its
//! [`TurnEngine`](super::TurnEngine) instead: each runs as an ordinary
//! request, answered locally against the attempt's candidate state with no
//! inference call, and commits like any turn. The four product spec §5.4
//! commands are advertised by `/help` and the host's command list; the time
//! and inspection commands work but are not advertised.

use crate::debug_view::{self, DebugView};
use crate::game_loop::GameLoopContext;
use crate::input::{Command, parse_system_command};
use crate::ipc::{handle_command, text_log};
use crate::world::description::format_exits;
use crate::world::transport::TransportMode;

/// A slash command the turn engine answers locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCommand {
    /// `/look`: the current place, as plain `look` describes it.
    Look,
    /// `/people` (or `/npcs`): who is here.
    People,
    /// `/exits`: where the player can go from here.
    Exits,
    /// `/help`: the advertised commands.
    Help,
    /// `/wait [minutes]`: let time pass.
    Wait(u32),
    /// `/pause`: hold the clock.
    Pause,
    /// `/resume`: let the clock run again.
    Resume,
    /// `/debug [view]`: inspect engine state.
    Debug(Option<String>),
    /// `/flags`: list feature flags.
    Flags,
}

/// The commands `/help` and a host's command list offer, with what each does.
pub const ADVERTISED: &[(&str, &str)] = &[
    ("/look", "Look around"),
    ("/people", "Who is here"),
    ("/exits", "Where you can go"),
    ("/help", "These commands"),
];

impl LocalCommand {
    /// The command `text` names, or `None` when it is not one of these
    /// (ordinary input, or a `/`-command the host does not offer).
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        match trimmed.to_lowercase().as_str() {
            "/look" => return Some(Self::Look),
            "/people" | "/npcs" => return Some(Self::People),
            "/exits" => return Some(Self::Exits),
            "/help" => return Some(Self::Help),
            _ => {}
        }
        match parse_system_command(trimmed)? {
            Command::Wait(minutes) => Some(Self::Wait(minutes)),
            Command::Pause => Some(Self::Pause),
            Command::Resume => Some(Self::Resume),
            Command::Debug(view) => Some(Self::Debug(view)),
            Command::Flags => Some(Self::Flags),
            _ => None,
        }
    }

    /// Runs the command against the attempt's state and emits its answer as
    /// a narration line. `/look` is answered by the ordinary `look` turn, so
    /// it is not run here.
    pub(super) async fn run(&self, ctx: &GameLoopContext<'_>, transport: &TransportMode) {
        let text = {
            let mut world = ctx.world.lock().await;
            let mut npc_manager = ctx.npc_manager.lock().await;
            let mut config = ctx.config.lock().await;
            let mut engine_command =
                |command| handle_command(command, &mut world, &mut npc_manager, &mut config);
            match self {
                Self::Look => return,
                Self::People => engine_command(Command::NpcsHere).response,
                Self::Wait(minutes) => engine_command(Command::Wait(*minutes)).response,
                Self::Pause => engine_command(Command::Pause).response,
                Self::Resume => engine_command(Command::Resume).response,
                Self::Flags => engine_command(Command::Flags).response,
                Self::Help => help_text(),
                Self::Exits => format_exits(
                    world.player_location,
                    &world.graph,
                    transport.speed_m_per_s,
                    &transport.label,
                ),
                Self::Debug(view) => debug_view::handle_debug(
                    view.as_deref(),
                    &DebugView {
                        world: &world,
                        npc_manager: &npc_manager,
                        language: &ctx.language,
                    },
                )
                .join("\n"),
            }
        };
        if text.trim().is_empty() {
            return;
        }
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log("system", text)).unwrap_or(serde_json::Value::Null),
        );
    }
}

/// `/help`: one line per advertised command.
fn help_text() -> String {
    ADVERTISED
        .iter()
        .map(|(command, what)| format!("{command}: {what}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::LocalCommand;

    #[test]
    fn parses_the_advertised_and_unadvertised_commands_only() {
        assert_eq!(LocalCommand::parse("/look"), Some(LocalCommand::Look));
        assert_eq!(LocalCommand::parse(" /PEOPLE "), Some(LocalCommand::People));
        assert_eq!(LocalCommand::parse("/npcs"), Some(LocalCommand::People));
        assert_eq!(LocalCommand::parse("/exits"), Some(LocalCommand::Exits));
        assert_eq!(LocalCommand::parse("/help"), Some(LocalCommand::Help));
        assert_eq!(
            LocalCommand::parse("/wait 30"),
            Some(LocalCommand::Wait(30))
        );
        assert_eq!(LocalCommand::parse("/wait"), Some(LocalCommand::Wait(15)));
        assert_eq!(LocalCommand::parse("/pause"), Some(LocalCommand::Pause));
        assert_eq!(LocalCommand::parse("/resume"), Some(LocalCommand::Resume));
        assert_eq!(
            LocalCommand::parse("/debug"),
            Some(LocalCommand::Debug(None))
        );
        assert_eq!(
            LocalCommand::parse("/debug clock"),
            Some(LocalCommand::Debug(Some("clock".to_string())))
        );
        assert_eq!(LocalCommand::parse("/flags"), Some(LocalCommand::Flags));
        for refused in [
            "/save",
            "/quit",
            "/provider",
            "/map",
            "/nonsense",
            "look",
            "help me",
        ] {
            assert_eq!(LocalCommand::parse(refused), None, "{refused}");
        }
    }
}
