//! Shared system command handler for all Parish backends.
//!
//! [`handle_command`] processes [`Command`] variants against mutable game state
//! and returns a [`CommandResult`] containing the response text and any side
//! effects. Each backend (Tauri, web server, headless CLI, test harness) calls
//! this function after acquiring its own locks, then dispatches the result
//! through its own event/output mechanism.
//!
//! Mode-specific commands (quit, save, load, map, debug, etc.) are returned as
//! [`CommandEffect`] variants so each backend can handle them appropriately.

use chrono::Timelike;

use crate::config::Provider;
use crate::input::{Command, FlagSubcommand};
use crate::npc::manager::NpcManager;
use crate::world::WorldState;

use super::config::GameConfig;
use super::handlers::mask_key;

/// Side effects that the calling backend must handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEffect {
    /// The player wants to quit.
    Quit,
    /// The inference pipeline needs to be rebuilt (provider/key changed).
    RebuildInference,
    /// Toggle the full map overlay (GUI) or show text map (CLI).
    ToggleMap,
    /// Save the game.
    SaveGame,
    /// Fork a new timeline branch with the given name.
    ForkBranch(String),
    /// Load a named branch.
    LoadBranch(String),
    /// List all save branches.
    ListBranches,
    /// Show snapshot history for the current branch.
    ShowLog,
    /// Run a debug sub-command.
    Debug(Option<String>),
    /// Show the loading spinner for the given number of seconds.
    ShowSpinner(u64),
    /// Start a fresh new game.
    NewGame,
    /// Rebuild the cloud/dialogue client specifically.
    RebuildCloudClient,
    /// Persist the current feature flag state to disk.
    SaveFlags,
    /// Apply a user-selected UI theme; frontend resolves the actual palette colors.
    /// Carries (theme_name, mode) where mode is "light", "dark", "auto", or "".
    ApplyTheme(String, String),
}

/// The result of processing a system command.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// The text response to display to the player. Empty string means no
    /// text should be emitted (e.g. for map toggle).
    pub response: String,
    /// Side effects the backend must handle after emitting the response.
    pub effects: Vec<CommandEffect>,
}

impl CommandResult {
    fn text(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            effects: vec![],
        }
    }

    fn with_effect(response: impl Into<String>, effect: CommandEffect) -> Self {
        Self {
            response: response.into(),
            effects: vec![effect],
        }
    }

    fn effect_only(effect: CommandEffect) -> Self {
        Self {
            response: String::new(),
            effects: vec![effect],
        }
    }
}

/// Processes a system command, mutating world/NPC/config state and returning
/// the response text plus any side effects.
///
/// The caller must acquire whatever locks are necessary before calling this
/// function and handle the returned [`CommandEffect`]s afterwards.
pub fn handle_command(
    cmd: Command,
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    config: &mut GameConfig,
) -> CommandResult {
    match cmd {
        // ── Time control ────────────────────────────────────────────────
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
        Command::InvalidBranchName(msg) => CommandResult::text(msg),

        // ── Info commands ───────────────────────────────────────────────
        Command::About => CommandResult::text(
            [
                "Parish — A text adventure set in 1820s rural Ireland.",
                "Explore a living village powered by AI-driven NPCs.",
                "",
                "Type /help for available commands.",
            ]
            .join("\n"),
        ),
        Command::NpcsHere => {
            let npcs = npc_manager.npcs_at(world.player_location);
            if npcs.is_empty() {
                CommandResult::text("No one else is here.")
            } else {
                let mut lines = vec!["NPCs here:".to_string()];
                for npc in &npcs {
                    let display = npc_manager.display_name(npc);
                    let intro = if npc_manager.is_introduced(npc.id) {
                        " [introduced]"
                    } else {
                        ""
                    };
                    lines.push(format!(
                        "  {} — {} ({}){}",
                        display, npc.occupation, npc.mood, intro
                    ));
                }
                CommandResult::text(lines.join("\n"))
            }
        }
        Command::Time => {
            let now = world.clock.now();
            let tod = world.clock.time_of_day();
            let season = world.clock.season();
            let festival = world
                .clock
                .check_festival()
                .map(|f| f.to_string())
                .unwrap_or_else(|| "none".to_string());
            let paused = if world.clock.is_paused() {
                " (PAUSED)"
            } else {
                ""
            };
            CommandResult::text(format!(
                "{:02}:{:02} {} — {}{}\nWeather: {}\nSpeed: {}x\nFestival: {}",
                now.hour(),
                now.minute(),
                tod,
                season,
                paused,
                world.weather,
                world.clock.speed_factor(),
                festival
            ))
        }
        Command::Wait(minutes) => {
            world.clock.advance(minutes as i64);
            npc_manager.assign_tiers(world, &[]);
            let _events = npc_manager.tick_schedules(&world.clock, &world.graph, world.weather);
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
            let events = npc_manager.tick_schedules(&world.clock, &world.graph, world.weather);
            let count = events.len();
            if count == 0 {
                CommandResult::text("No NPC activity.")
            } else {
                CommandResult::text(format!("{} schedule event(s) processed.", count))
            }
        }

        // ── Sidebar & Improv ────────────────────────────────────────────
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

        // ── Base provider/model/key ───────���─────────────────────────────
        Command::ShowProvider => CommandResult::text(format!("Provider: {}", config.provider_name)),
        Command::SetProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                config.base_url = provider.default_base_url().to_string();
                config.provider_name = format!("{:?}", provider).to_lowercase();
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

        // ── Cloud provider ──────────���───────────────────────────────────
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
                let provider_name = format!("{:?}", provider).to_lowercase();
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

        // ── Per-category provider/model/key ──��──────────────────────────
        Command::ShowCategoryProvider(cat) => {
            let idx = GameConfig::cat_idx(cat);
            match &config.category_provider[idx] {
                Some(p) => CommandResult::text(format!("{} provider: {}", cat.name(), p)),
                None => CommandResult::text(format!(
                    "{} provider: (inherits base: {})",
                    cat.name(),
                    config.provider_name
                )),
            }
        }
        Command::SetCategoryProvider(cat, name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let idx = GameConfig::cat_idx(cat);
                let provider_name = format!("{:?}", provider).to_lowercase();
                config.category_provider[idx] = Some(provider_name.clone());
                config.category_base_url[idx] = Some(provider.default_base_url().to_string());
                CommandResult::with_effect(
                    format!("{} provider changed to {}.", cat.name(), provider_name),
                    CommandEffect::RebuildInference,
                )
            }
            Err(e) => CommandResult::text(format!("{}", e)),
        },
        Command::ShowCategoryModel(cat) => {
            let idx = GameConfig::cat_idx(cat);
            match &config.category_model[idx] {
                Some(m) => CommandResult::text(format!("{} model: {}", cat.name(), m)),
                None => CommandResult::text(format!(
                    "{} model: (inherits base: {})",
                    cat.name(),
                    config.model_name
                )),
            }
        }
        Command::SetCategoryModel(cat, name) => {
            let idx = GameConfig::cat_idx(cat);
            config.category_model[idx] = Some(name.clone());
            CommandResult::text(format!("{} model changed to {}.", cat.name(), name))
        }
        Command::ShowCategoryKey(cat) => {
            let idx = GameConfig::cat_idx(cat);
            match &config.category_api_key[idx] {
                Some(key) => {
                    CommandResult::text(format!("{} API key: {}", cat.name(), mask_key(key)))
                }
                None => CommandResult::text(format!("{} API key: (not set)", cat.name())),
            }
        }
        Command::SetCategoryKey(cat, value) => {
            let cat_name = cat.name().to_string();
            let idx = GameConfig::cat_idx(cat);
            config.category_api_key[idx] = Some(value);
            CommandResult::with_effect(
                format!("{} API key updated.", cat_name),
                CommandEffect::RebuildInference,
            )
        }

        // ── Feature flags ───────────────────────────────────────────────
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
            CommandResult::with_effect(
                format!("Feature '{}' disabled.", name),
                CommandEffect::SaveFlags,
            )
        }
        Command::InvalidFlagName(msg) => CommandResult::text(msg),

        // ── Mode-specific commands (delegated to backend) ───────────────
        Command::Quit => CommandResult::effect_only(CommandEffect::Quit),
        Command::Help => CommandResult::text(
            [
                "A few things ye might say:",
                "  /help              — Show this help",
                "  /about             — About this game",
                "  /status            — Where am I?",
                "  /time              — Time, weather, and season details",
                "  /npcs              — Who is nearby?",
                "  /wait [minutes]    — Wait in place (default: 15 min)",
                "  /pause             — Hold time still",
                "  /resume            — Let time flow again",
                "  /speed [slow|normal|fast|fastest|ludicrous]  — Show or change game speed",
                "  /irish             — Toggle Irish pronunciation sidebar",
                "  /improv            — Toggle improv craft mode",
                "  /map               — Toggle the full map",
                "  /flag list                  — List all feature flags",
                "  /flag enable <name>         — Enable a feature flag",
                "  /flag disable <name>        — Disable a feature flag",
                "  /save              — Save the game",
                "  /fork <name>       — Fork a new branch from here",
                "  /load <name>       — Load a named branch",
                "  /branches          — List save branches",
                "  /log               — Show branch history",
                "  /new-game          — Start a fresh game",
            ]
            .join("\n"),
        ),
        Command::Save => CommandResult::effect_only(CommandEffect::SaveGame),
        Command::Fork(name) => CommandResult::effect_only(CommandEffect::ForkBranch(name)),
        Command::Load(name) => CommandResult::effect_only(CommandEffect::LoadBranch(name)),
        Command::Branches => CommandResult::effect_only(CommandEffect::ListBranches),
        Command::Log => CommandResult::effect_only(CommandEffect::ShowLog),
        Command::Map => CommandResult::effect_only(CommandEffect::ToggleMap),
        Command::Debug(sub) => CommandResult::effect_only(CommandEffect::Debug(sub)),
        Command::Spinner(secs) => CommandResult::effect_only(CommandEffect::ShowSpinner(secs)),
        Command::NewGame => CommandResult::effect_only(CommandEffect::NewGame),
        Command::Theme(arg) => match arg.as_deref().map(str::trim) {
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
                            "auto" => "Solarized auto — follows the real sun.",
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
        },
    }
}

/// Renders the current location description with NPC names and exits.
///
/// Returns the combined text that all backends display for a "look" command.
pub fn render_look_text(
    world: &WorldState,
    npc_manager: &NpcManager,
    speed_m_per_s: f64,
    transport_label: &str,
    include_exits: bool,
) -> String {
    use crate::world::description::{format_exits, render_description};

    let desc = if let Some(loc_data) = world.current_location_data() {
        let tod = world.clock.time_of_day();
        let weather = world.weather.to_string();
        let npc_display: Vec<String> = npc_manager
            .npcs_at(world.player_location)
            .iter()
            .map(|n| npc_manager.display_name(n).to_string())
            .collect();
        let npc_names: Vec<&str> = npc_display.iter().map(|s| s.as_str()).collect();
        render_description(loc_data, tod, &weather, &npc_names)
    } else {
        world.current_location().description.clone()
    };

    if include_exits {
        let exits = format_exits(
            world.player_location,
            &world.graph,
            speed_m_per_s,
            transport_label,
        );
        format!("{}\n{}", desc, exits)
    } else {
        desc
    }
}

// ── Tests ────────���──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InferenceCategory;

    fn default_state() -> (WorldState, NpcManager, GameConfig) {
        (WorldState::new(), NpcManager::new(), GameConfig::default())
    }

    #[test]
    fn pause_command() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Pause, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("stand still"));
        assert!(world.clock.is_paused());
    }

    #[test]
    fn resume_command() {
        let (mut world, mut npc, mut config) = default_state();
        world.clock.pause();
        let result = handle_command(Command::Resume, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("stirs again"));
        assert!(!world.clock.is_paused());
    }

    #[test]
    fn status_command() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Status, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("Location:"));
    }

    #[test]
    fn toggle_improv() {
        let (mut world, mut npc, mut config) = default_state();
        assert!(!config.improv_enabled);
        let result = handle_command(Command::ToggleImprov, &mut world, &mut npc, &mut config);
        assert!(config.improv_enabled);
        assert!(result.response.contains("improv"));
    }

    #[test]
    fn set_provider_triggers_rebuild() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetProvider("openrouter".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("openrouter"));
        assert!(result.effects.contains(&CommandEffect::RebuildInference));
    }

    #[test]
    fn set_key_triggers_rebuild() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetKey("sk-test12345678".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(result.response, "API key updated.");
        assert!(result.effects.contains(&CommandEffect::RebuildInference));
    }

    #[test]
    fn show_model_auto_detect() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::ShowModel, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("auto-detect"));
    }

    #[test]
    fn quit_returns_effect() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Quit, &mut world, &mut npc, &mut config);
        assert!(result.response.is_empty());
        assert!(result.effects.contains(&CommandEffect::Quit));
    }

    #[test]
    fn npcs_here_empty() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::NpcsHere, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("No one"));
    }

    #[test]
    fn time_command() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Time, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("Weather:"));
        assert!(result.response.contains("Speed:"));
    }

    #[test]
    fn category_provider_inherits_base() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::ShowCategoryProvider(InferenceCategory::Dialogue),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("inherits base"));
    }

    #[test]
    fn render_look_text_basic() {
        let world = WorldState::new();
        let npc = NpcManager::new();
        let text = render_look_text(&world, &npc, 1.25, "on foot", true);
        assert!(!text.is_empty());
    }
}
