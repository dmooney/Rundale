//! Provider/model/key commands — base, cloud, per-category, and presets.

use crate::input::Command;
use crate::ipc::handlers::mask_key;

use super::CommandResult;
use crate::ipc::config::GameConfig;

/// Base provider/model/key commands.
pub(super) fn handle_provider_command(cmd: Command, config: &mut GameConfig) -> CommandResult {
    match cmd {
        Command::ShowProvider => CommandResult::text(format!("Provider: {}", config.provider_name)),
        Command::SetProvider(_) => removed_routing_command(),
        Command::ShowModel => {
            if config.model_name.is_empty() {
                CommandResult::text("Model: (auto-detect)")
            } else {
                CommandResult::text(format!("Model: {}", config.model_name))
            }
        }
        Command::SetModel(_) => removed_routing_command(),
        Command::ShowKey => match &config.api_key {
            Some(key) => CommandResult::text(format!("API key: {}", mask_key(key))),
            None => CommandResult::text("API key: (not set)"),
        },
        Command::SetKey(_) => removed_routing_command(),
        Command::ShowBaseUrl => CommandResult::text(format!("Base URL: {}", config.base_url)),
        Command::SetBaseUrl(_) => removed_routing_command(),
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}

/// Cloud provider/model/key commands.
pub(super) fn handle_cloud_provider_command(
    _cmd: Command,
    _config: &mut GameConfig,
) -> CommandResult {
    CommandResult::text(
        "Legacy /cloud commands were removed by configuration schema v2; use a named loadout and per-category route instead.",
    )
}

/// Per-category provider/model/key commands.
pub(super) fn handle_category_provider_command(
    cmd: Command,
    config: &mut GameConfig,
) -> CommandResult {
    match cmd {
        Command::ShowCategoryProvider(cat) => match config.category_provider.get(&cat) {
            Some(p) => CommandResult::text(format!("{} provider: {}", cat.name(), p)),
            None => CommandResult::text(format!(
                "{} provider: (inherits base: {})",
                cat.name(),
                config.provider_name
            )),
        },
        Command::SetCategoryProvider(_, _) => removed_routing_command(),
        Command::ShowCategoryModel(cat) => match config.category_model.get(&cat) {
            Some(m) => CommandResult::text(format!("{} model: {}", cat.name(), m)),
            None => CommandResult::text(format!(
                "{} model: (inherits base: {})",
                cat.name(),
                config.model_name
            )),
        },
        Command::SetCategoryModel(_, _) => removed_routing_command(),
        Command::ShowCategoryKey(cat) => match config.category_api_key.get(&cat) {
            Some(key) => CommandResult::text(format!("{} API key: {}", cat.name(), mask_key(key))),
            None => CommandResult::text(format!("{} API key: (not set)", cat.name())),
        },
        Command::SetCategoryKey(_, _) => removed_routing_command(),
        Command::ShowCategoryBaseUrl(cat) => match config.category_base_url.get(&cat) {
            Some(u) => CommandResult::text(format!("{} base URL: {}", cat.name(), u)),
            None => CommandResult::text(format!(
                "{} base URL: (inherits base: {})",
                cat.name(),
                config.base_url
            )),
        },
        Command::SetCategoryBaseUrl(_, _) => removed_routing_command(),
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}

/// Provider preset commands.
pub(super) fn handle_preset_command(cmd: Command, _config: &mut GameConfig) -> CommandResult {
    match cmd {
        Command::ShowPreset => {
            use parish_config::registry;
            let mut ids: Vec<String> = registry()
                .all()
                .into_iter()
                .filter(|p| p.has_preset())
                .map(|p| p.id().to_string())
                .collect();
            ids.sort();
            CommandResult::text(format!(
                "Usage: /preset <provider>. Providers with presets: {}",
                ids.join(", ")
            ))
        }
        Command::ApplyPreset(_) => removed_routing_command(),
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}

fn removed_routing_command() -> CommandResult {
    CommandResult::text(
        "Runtime provider/model/key changes were removed by configuration schema v2; use setup or edit a named loadout, then reload configuration.",
    )
}
