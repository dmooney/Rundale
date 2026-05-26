//! Map-related commands: `/unexplored` (reveal/hide map locations) and `/map` (tile sources).

use super::{CommandEffect, CommandResult};
use crate::ipc::config::GameConfig;

/// Handles the `/unexplored` command (reveal/hide all unexplored map locations).
///
/// Gated by the `reveal-unexplored` feature flag (default-enabled per
/// CLAUDE.md rule #6). Uses `is_disabled` semantics so the feature ships
/// on without needing to seed the flags file.
pub(super) fn handle_unexplored_command(
    config: &mut GameConfig,
    arg: Option<bool>,
) -> CommandResult {
    if config.flags.is_disabled("reveal-unexplored") {
        config.reveal_unexplored_locations = false;
        return CommandResult::text(
            "The /unexplored command is disabled. Re-enable with /flag enable reveal-unexplored.",
        );
    }

    match arg {
        Some(true) => {
            config.reveal_unexplored_locations = true;
            CommandResult::text(
                "All unexplored locations are now revealed on the map (still marked unvisited).",
            )
        }
        Some(false) => {
            config.reveal_unexplored_locations = false;
            CommandResult::text("Unexplored locations are hidden again (fog-of-war frontier only).")
        }
        None => {
            let status = if config.reveal_unexplored_locations {
                "revealed"
            } else {
                "hidden"
            };
            CommandResult::text(format!(
                "Unexplored locations are currently {}.\nUsage: /unexplored reveal|hide",
                status
            ))
        }
    }
}

/// Handles the `/map` command (list / switch map tile sources).
///
/// Gated by the `period-map-tiles` feature flag (default-enabled per
/// CLAUDE.md rule #6). Uses `is_disabled` semantics so the feature ships
/// on without needing to seed the flags file.
pub(super) fn handle_map_command(config: &mut GameConfig, arg: Option<String>) -> CommandResult {
    if config.flags.is_disabled("period-map-tiles") {
        return CommandResult::text(
            "Period map tiles are disabled. Re-enable with /flag enable period-map-tiles.",
        );
    }

    let arg = arg.as_deref().map(str::trim).filter(|s| !s.is_empty());

    // Compare case-insensitively: TOML keys are canonical lowercase, but
    // the parser preserves case from the user input (`/map OSM`).
    let lookup_id = |needle: &str| -> Option<(String, String)> {
        let needle_lower = needle.to_lowercase();
        config
            .tile_sources
            .iter()
            .find(|(id, _)| id.to_lowercase() == needle_lower)
            .cloned()
    };

    match arg {
        None => {
            if config.tile_sources.is_empty() {
                return CommandResult::text("No tile sources configured.");
            }
            let mut lines = vec!["Available tile sources:".to_string()];
            for (id, label) in &config.tile_sources {
                let marker = if id == &config.active_tile_source {
                    "*"
                } else {
                    " "
                };
                let active_tag = if id == &config.active_tile_source {
                    " (active)"
                } else {
                    ""
                };
                lines.push(format!("  {} {}{} — {}", marker, id, active_tag, label));
            }
            lines.push("Usage: /map <id>".to_string());
            CommandResult::text(lines.join("\n"))
        }
        Some(needle) => match lookup_id(needle) {
            Some((id, label)) => {
                config.active_tile_source = id.clone();
                CommandResult::with_effect(
                    format!("Switched map tiles to {}.", label),
                    CommandEffect::ApplyTiles(id),
                )
            }
            None => {
                let available: Vec<&str> = config
                    .tile_sources
                    .iter()
                    .map(|(id, _)| id.as_str())
                    .collect();
                let list = if available.is_empty() {
                    "(none configured)".to_string()
                } else {
                    available.join(", ")
                };
                CommandResult::text(format!(
                    "Unknown tile source '{}'. Available: {}",
                    needle, list
                ))
            }
        },
    }
}
