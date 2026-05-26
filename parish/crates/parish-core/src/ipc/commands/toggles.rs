//! Sidebar and improv toggle commands.

use crate::input::Command;

use crate::ipc::config::GameConfig;
use super::CommandResult;

/// Sidebar and improv toggles.
pub(super) fn handle_sidebar_improv_command(cmd: Command, config: &mut GameConfig) -> CommandResult {
    match cmd {
        Command::ToggleSidebar => {
            CommandResult::text("The Irish words panel is managed by the sidebar.")
        }
        Command::ToggleImprov => {
            config.improv_enabled = !config.improv_enabled;
            if config.improv_enabled {
                CommandResult::text("The characters loosen up — improv craft engaged.")
            } else {
                CommandResult::text("The characters settle back to their usual selves.")
            }
        }
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}
