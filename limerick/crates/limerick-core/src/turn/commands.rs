//! Local slash commands a host lets players run as turns (#2120).
//!
//! The desktop runs `/`-commands on its own system-command path, outside the
//! turn API. A host that has no such path (the phone) enables these on its
//! [`TurnEngine`](super::TurnEngine) instead: each runs as an ordinary
//! request, answered locally against the attempt's candidate state with no
//! inference call, and commits like any turn. The four product spec §5.4
//! commands are advertised by `/help` and the host's command list; the time
//! and inspection commands work but are not advertised.
//!
//! [`COMMANDS`] is the registry a host completes from (#2146): every command,
//! and the words that may follow it, so completion and the parser agree.

use crate::debug_view::{self, DebugView};
use crate::game_loop::GameLoopContext;
use crate::input::{Command, parse_system_command};
use crate::ipc::{handle_command, text_log};
use crate::world::description::format_exits;
use crate::world::time::minute_word;
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

/// A command a host offers, as completion shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    /// The command, with its `/`.
    pub name: &'static str,
    /// What it does.
    pub summary: &'static str,
    /// Whether `/help` and the host's short command list name it.
    pub advertised: bool,
    /// What may follow it.
    pub argument: CommandArgument,
}

/// What may follow a command's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandArgument {
    /// Nothing.
    None,
    /// A number of minutes; these are suggestions, any number works.
    Minutes(&'static [u32]),
    /// A `/debug` view ([`debug_view::SUBCOMMANDS`]), some followed by an
    /// NPC's name.
    DebugView,
}

/// Every command the turn engine runs, in completion order. Aliases
/// (`/npcs`) are accepted but not listed.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "/look",
        summary: "Look around",
        advertised: true,
        argument: CommandArgument::None,
    },
    CommandSpec {
        name: "/people",
        summary: "Who is here",
        advertised: true,
        argument: CommandArgument::None,
    },
    CommandSpec {
        name: "/exits",
        summary: "Where you can go",
        advertised: true,
        argument: CommandArgument::None,
    },
    CommandSpec {
        name: "/help",
        summary: "These commands",
        advertised: true,
        argument: CommandArgument::None,
    },
    CommandSpec {
        name: "/wait",
        summary: "Let time pass",
        advertised: false,
        argument: CommandArgument::Minutes(&[15, 30, 60]),
    },
    CommandSpec {
        name: "/pause",
        summary: "Hold the clock",
        advertised: false,
        argument: CommandArgument::None,
    },
    CommandSpec {
        name: "/resume",
        summary: "Let the clock run",
        advertised: false,
        argument: CommandArgument::None,
    },
    CommandSpec {
        name: "/debug",
        summary: "Inspect engine state",
        advertised: false,
        argument: CommandArgument::DebugView,
    },
    CommandSpec {
        name: "/flags",
        summary: "Feature flags",
        advertised: false,
        argument: CommandArgument::None,
    },
];

/// The commands `/help` and a host's command list offer.
pub fn advertised() -> impl Iterator<Item = &'static CommandSpec> {
    COMMANDS.iter().filter(|command| command.advertised)
}

/// A word completion offers, and what may follow it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionWord {
    /// The word, as inserted.
    pub word: String,
    /// What it does.
    pub summary: String,
    /// The words that may follow it.
    pub next: Vec<CompletionWord>,
    /// Whether an NPC's name follows instead (any NPC in the world).
    pub takes_npc: bool,
}

impl CompletionWord {
    fn leaf(word: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            word: word.into(),
            summary: summary.into(),
            next: Vec::new(),
            takes_npc: false,
        }
    }
}

impl CommandSpec {
    /// This command and what may follow it, as a completion tree.
    pub fn completion(&self) -> CompletionWord {
        let next = match self.argument {
            CommandArgument::None => Vec::new(),
            CommandArgument::Minutes(minutes) => minutes
                .iter()
                .map(|minutes| {
                    CompletionWord::leaf(
                        minutes.to_string(),
                        format!("{minutes} {}", minute_word(*minutes)),
                    )
                })
                .collect(),
            CommandArgument::DebugView => debug_view::SUBCOMMANDS
                .iter()
                .map(|sub| CompletionWord {
                    takes_npc: sub.takes_npc,
                    ..CompletionWord::leaf(sub.name, sub.summary)
                })
                .collect(),
        };
        CompletionWord {
            next,
            ..CompletionWord::leaf(self.name, self.summary)
        }
    }
}

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
    advertised()
        .map(|command| format!("{}: {}", command.name, command.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{COMMANDS, LocalCommand, advertised};

    #[test]
    fn every_completion_parses_as_the_command_it_names() {
        let names: Vec<_> = advertised().map(|command| command.name).collect();
        assert_eq!(names, ["/look", "/people", "/exits", "/help"]);
        for command in COMMANDS {
            let tree = command.completion();
            assert!(LocalCommand::parse(&tree.word).is_some(), "{}", tree.word);
            for next in &tree.next {
                let text = format!("{} {}", tree.word, next.word);
                let parsed = LocalCommand::parse(&text);
                assert!(parsed.is_some(), "{text}");
                if next.takes_npc {
                    let named = format!("{text} micheal");
                    assert_eq!(
                        LocalCommand::parse(&named),
                        Some(LocalCommand::Debug(Some(format!("{} micheal", next.word)))),
                        "{named}"
                    );
                }
            }
        }
        let debug = COMMANDS
            .iter()
            .find(|c| c.name == "/debug")
            .unwrap()
            .completion();
        let views: Vec<_> = debug.next.iter().map(|next| next.word.as_str()).collect();
        assert_eq!(
            views,
            [
                "memory",
                "schedule",
                "relationships",
                "gossip",
                "clock",
                "here",
                "npcs",
                "tiers",
                "language",
                "reactions",
                "help"
            ]
        );
        let wait = COMMANDS
            .iter()
            .find(|c| c.name == "/wait")
            .unwrap()
            .completion();
        assert_eq!(
            LocalCommand::parse(&format!("/wait {}", wait.next[0].word)),
            Some(LocalCommand::Wait(15))
        );
    }

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
