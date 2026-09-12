//! Theme selection command.

use super::{CommandEffect, CommandResult};

/// Theme selection command.
pub(super) fn handle_theme_command(arg: Option<String>) -> CommandResult {
    match arg.as_deref().map(str::trim) {
        None | Some("") => CommandResult::text(
            "Available themes: default, solarized\n\
             Usage: /theme <name> [light|dark|auto]\n\
             Solarized auto switches with real-world sunrise and sunset.",
        ),
        Some("default") => CommandResult::with_effect(
            "Reverting to the parish's natural colours.",
            CommandEffect::ApplyTheme("default".to_string(), String::new()),
        ),
        Some(rest) => {
            let mut parts = rest.splitn(2, ' ');
            let name = parts.next().unwrap_or("").to_lowercase();
            let mode = parts.next().map(str::trim).unwrap_or("").to_lowercase();
            match name.as_str() {
                "solarized" => {
                    let mode = if mode.is_empty() {
                        "auto".to_string()
                    } else {
                        mode
                    };
                    let msg = match mode.as_str() {
                        "light" => "Solarized light applied.",
                        "dark" => "Solarized dark applied.",
                        "auto" => "Solarized auto — follows the game's time of day.",
                        other => {
                            return CommandResult::text(format!(
                                "Unknown mode '{}'. Try: light, dark, auto",
                                other
                            ));
                        }
                    };
                    CommandResult::with_effect(
                        msg,
                        CommandEffect::ApplyTheme("solarized".to_string(), mode),
                    )
                }
                other => CommandResult::text(format!(
                    "Unknown theme '{}'. Available: default, solarized",
                    other
                )),
            }
        }
    }
}
