//! Provider/model/key commands — base, cloud, per-category, and presets.

use crate::config::{InferenceCategory, Provider};
use crate::input::Command;
use crate::ipc::handlers::mask_key;

use super::{CommandEffect, CommandResult};
use crate::ipc::config::GameConfig;

/// Base provider/model/key commands.
pub(super) fn handle_provider_command(cmd: Command, config: &mut GameConfig) -> CommandResult {
    match cmd {
        Command::ShowProvider => CommandResult::text(format!("Provider: {}", config.provider_name)),
        Command::SetProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                config.base_url = provider.default_base_url().to_string();
                config.provider_name = provider.id().to_string();
                config.fill_missing_models_from_presets();
                CommandResult::with_effect(
                    format!("Provider changed to {}.", config.provider_name),
                    CommandEffect::RebuildInference,
                )
            }
            Err(e) => CommandResult::text(format!("{}", e)),
        },
        Command::ShowModel => {
            if config.model_name.is_empty() {
                CommandResult::text("Model: (auto-detect)")
            } else {
                CommandResult::text(format!("Model: {}", config.model_name))
            }
        }
        Command::SetModel(name) => {
            config.model_name = name.clone();
            CommandResult::text(format!("Model changed to {}.", name))
        }
        Command::ShowKey => match &config.api_key {
            Some(key) => CommandResult::text(format!("API key: {}", mask_key(key))),
            None => CommandResult::text("API key: (not set)"),
        },
        Command::SetKey(value) => {
            config.api_key = Some(value);
            CommandResult::with_effect("API key updated.", CommandEffect::RebuildInference)
        }
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}

/// Cloud provider/model/key commands.
pub(super) fn handle_cloud_provider_command(
    cmd: Command,
    config: &mut GameConfig,
) -> CommandResult {
    match cmd {
        Command::ShowCloud => {
            if let Some(ref provider) = config.cloud_provider_name {
                let model = config.cloud_model_name.as_deref().unwrap_or("(none)");
                CommandResult::text(format!("Cloud: {} | Model: {}", provider, model))
            } else {
                CommandResult::text("No cloud provider configured.")
            }
        }
        Command::SetCloudProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let base_url = provider.default_base_url().to_string();
                let provider_name = provider.id().to_string();
                config.cloud_provider_name = Some(provider_name.clone());
                config.cloud_base_url = Some(base_url);
                CommandResult::with_effect(
                    format!("Cloud provider changed to {}.", provider_name),
                    CommandEffect::RebuildCloudClient,
                )
            }
            Err(e) => CommandResult::text(format!("{}", e)),
        },
        Command::ShowCloudModel => match &config.cloud_model_name {
            Some(model) => CommandResult::text(format!("Cloud model: {}", model)),
            None => CommandResult::text("Cloud model: (not set)"),
        },
        Command::SetCloudModel(name) => {
            config.cloud_model_name = Some(name.clone());
            CommandResult::text(format!("Cloud model changed to {}.", name))
        }
        Command::ShowCloudKey => match &config.cloud_api_key {
            Some(key) => CommandResult::text(format!("Cloud API key: {}", mask_key(key))),
            None => CommandResult::text("Cloud API key: (not set)"),
        },
        Command::SetCloudKey(value) => {
            config.cloud_api_key = Some(value);
            CommandResult::with_effect("Cloud API key updated.", CommandEffect::RebuildCloudClient)
        }
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
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
        Command::SetCategoryProvider(cat, name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let provider_name = provider.id().to_string();
                config.category_provider.insert(cat, provider_name.clone());
                config
                    .category_base_url
                    .insert(cat, provider.default_base_url().to_string());
                config.fill_missing_models_from_presets();
                CommandResult::with_effect(
                    format!("{} provider changed to {}.", cat.name(), provider_name),
                    CommandEffect::RebuildInference,
                )
            }
            Err(e) => CommandResult::text(format!("{}", e)),
        },
        Command::ShowCategoryModel(cat) => match config.category_model.get(&cat) {
            Some(m) => CommandResult::text(format!("{} model: {}", cat.name(), m)),
            None => CommandResult::text(format!(
                "{} model: (inherits base: {})",
                cat.name(),
                config.model_name
            )),
        },
        Command::SetCategoryModel(cat, name) => {
            config.category_model.insert(cat, name.clone());
            CommandResult::text(format!("{} model changed to {}.", cat.name(), name))
        }
        Command::ShowCategoryKey(cat) => match config.category_api_key.get(&cat) {
            Some(key) => CommandResult::text(format!("{} API key: {}", cat.name(), mask_key(key))),
            None => CommandResult::text(format!("{} API key: (not set)", cat.name())),
        },
        Command::SetCategoryKey(cat, value) => {
            let cat_name = cat.name().to_string();
            config.category_api_key.insert(cat, value);
            CommandResult::with_effect(
                format!("{} API key updated.", cat_name),
                CommandEffect::RebuildInference,
            )
        }
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}

/// Provider preset commands.
pub(super) fn handle_preset_command(cmd: Command, config: &mut GameConfig) -> CommandResult {
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
        Command::ApplyPreset(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                if !provider.has_preset() {
                    CommandResult::text(format!(
                        "No preset available for '{}'. Configure models manually with /model.<category>.",
                        name
                    ))
                } else {
                    let provider_name = provider.id().to_string();
                    let default_url = provider.default_base_url().to_string();

                    // Provider/url writes are identical for both branches.
                    config.provider_name = provider_name.clone();
                    config.base_url = default_url.clone();
                    for cat in InferenceCategory::ALL {
                        config.category_provider.insert(cat, provider_name.clone());
                        config.category_base_url.insert(cat, default_url.clone());
                    }

                    // For Ollama with a recorded auto-setup model: re-pin
                    // that model across every slot instead of writing the
                    // static qwen3 preset list. Auto-setup pulled exactly
                    // one model matched to the user's hardware; the static
                    // preset would route every category to qwen3 tags the
                    // user has not downloaded.
                    if provider.id() == "ollama"
                        && let Some(auto) = config.auto_setup_model.clone()
                    {
                        config.pin_setup_model(auto);
                    } else {
                        // Base model: use Dialogue's pick so any code path
                        // that still falls through to `model_name` gets a
                        // sensible value.
                        if let Some(m) = provider.preset_model(InferenceCategory::Dialogue) {
                            config.model_name = m.to_string();
                        }

                        // Per-category models: always overwrite (applying
                        // a preset is an explicit user action). API keys
                        // are intentionally left alone — see hint below.
                        for cat in InferenceCategory::ALL {
                            if let Some(m) = provider.preset_model(cat).map(str::to_string) {
                                config.category_model.insert(cat, m);
                            } else {
                                config.category_model.remove(&cat);
                            }
                        }
                    }

                    let hint = if provider.requires_api_key() && config.api_key.is_none() {
                        format!(
                            " Set your API key with `/key <value>` — {} requires one.",
                            provider_name
                        )
                    } else {
                        String::new()
                    };

                    CommandResult::with_effect(
                        format!(
                            "Applied {} preset (Dialogue/Simulation/Intent/Reaction).{}",
                            provider_name, hint
                        ),
                        CommandEffect::RebuildInference,
                    )
                }
            }
            Err(e) => CommandResult::text(format!("{}", e)),
        },
        _ => unreachable!("dispatched only by handle_command for matching variants"),
    }
}
