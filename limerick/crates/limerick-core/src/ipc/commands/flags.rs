//! Feature flag commands.

use crate::input::{Command, FlagSubcommand};

use super::{CommandEffect, CommandResult};
use crate::ipc::config::GameConfig;

/// Feature flag commands.
pub(super) fn handle_flag_command(cmd: Command, config: &mut GameConfig) -> CommandResult {
    match cmd {
        Command::Flags | Command::Flag(FlagSubcommand::List) => {
            let list = config.flags.list();
            if list.is_empty() {
                CommandResult::text(
                    "No feature flags have been set. Use /flag enable <name> to enable one.",
                )
            } else {
                let mut lines = vec!["Feature flags:".to_string()];
                for (name, enabled) in &list {
                    let status = if *enabled { "on " } else { "off" };
                    lines.push(format!("  [{}] {}", status, name));
                }
                CommandResult::text(lines.join("\n"))
            }
        }
        Command::Flag(FlagSubcommand::Enable(name)) => {
            config.flags.enable(&name);
            CommandResult::with_effect(
                format!("Feature '{}' enabled.", name),
                CommandEffect::SaveFlags,
            )
        }
        Command::Flag(FlagSubcommand::Disable(name)) => {
            config.flags.disable(&name);
            if name == "reveal-unexplored" {
                config.reveal_unexplored_locations = false;
            }
            CommandResult::with_effect(
                format!("Feature '{}' disabled.", name),
                CommandEffect::SaveFlags,
            )
        }
        Command::InvalidFlagName(msg) => CommandResult::text(msg),
        Command::InvalidBranchName(msg) => CommandResult::text(msg),
        Command::InvalidSystemCommand(input) => CommandResult::text(format!(
            "Unknown system command: {input}. Use /help to list available commands."
        )),
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}
