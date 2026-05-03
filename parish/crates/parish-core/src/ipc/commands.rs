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

use crate::config::{InferenceCategory, Provider};
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
    /// Open the Parish Designer mod editor (GUI only).
    OpenDesigner,
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
    /// Switch the full-map base tile source. Carries the source id
    /// (e.g. "osm", "historic") — frontend looks up URL etc.
    /// from the tile registry it received via `UiConfigSnapshot`.
    ApplyTiles(String),
}

/// How a command's response text should be presented by the frontend.
///
/// Most command output is prose rendered in the chat panel's proportional
/// serif font. Tabular output (e.g. the `/help` two-column list) needs a
/// monospace font so that column-aligned padding actually lines up.
/// Frontends translate this into a `subtype` on the text-log payload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextPresentation {
    /// Default — render with the normal chat font.
    #[default]
    Prose,
    /// Render with a monospace font so column alignment is preserved.
    Tabular,
}

/// The result of processing a system command.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// The text response to display to the player. Empty string means no
    /// text should be emitted (e.g. for map toggle).
    pub response: String,
    /// Side effects the backend must handle after emitting the response.
    pub effects: Vec<CommandEffect>,
    /// How the frontend should render [`Self::response`].
    pub presentation: TextPresentation,
}

impl CommandResult {
    fn text(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            effects: vec![],
            presentation: TextPresentation::Prose,
        }
    }

    fn text_tabular(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            effects: vec![],
            presentation: TextPresentation::Tabular,
        }
    }

    fn with_effect(response: impl Into<String>, effect: CommandEffect) -> Self {
        Self {
            response: response.into(),
            effects: vec![effect],
            presentation: TextPresentation::Prose,
        }
    }

    fn effect_only(effect: CommandEffect) -> Self {
        Self {
            response: String::new(),
            effects: vec![effect],
            presentation: TextPresentation::Prose,
        }
    }
}

/// Canonical list of user-facing system commands shown by `/help`.
///
/// Kept in alphabetical order by command name so the rendered output is
/// stable and easy to scan. Descriptions are short so the list fits
/// comfortably in the chat panel.
const HELP_ENTRIES: &[(&str, &str)] = &[
    ("/about", "About this game"),
    ("/branches", "List save branches"),
    ("/designer", "Open the Parish Designer"),
    ("/flag disable <name>", "Disable a feature flag"),
    ("/flag enable <name>", "Enable a feature flag"),
    ("/flag list", "List all feature flags"),
    ("/fork <name>", "Fork a new branch from here"),
    ("/help", "Show this help"),
    ("/improv", "Toggle improv craft mode"),
    ("/irish", "Toggle Irish pronunciation sidebar"),
    ("/load <name>", "Load a named branch"),
    ("/log", "Show branch history"),
    ("/map [id]", "List or switch map tile sources"),
    ("/new-game", "Start a fresh game"),
    ("/npcs", "Who is nearby?"),
    ("/pause", "Hold time still"),
    ("/resume", "Let time flow again"),
    ("/save", "Save the game"),
    (
        "/speed [slow|normal|fast|fastest|ludicrous]",
        "Show or change game speed",
    ),
    ("/status", "Where am I?"),
    ("/time", "Time, weather, and season details"),
    (
        "/unexplored [reveal|hide]",
        "Reveal or hide all unexplored locations",
    ),
    ("/wait [minutes]", "Wait in place (default: 15 min)"),
];

/// Renders the `/help` body as a monospace-aligned two-column list.
///
/// Command names are left-padded to the widest entry so the em-dash
/// separator lines up in a fixed-width font. Frontends tag this response
/// with [`TextPresentation::Tabular`] so the chat UI picks a monospace
/// font — see [`CommandResult::text_tabular`].
fn render_help_text() -> String {
    let max_cmd_width = HELP_ENTRIES
        .iter()
        .map(|(cmd, _)| cmd.chars().count())
        .max()
        .unwrap_or(0);

    let mut out = String::from("Available commands:");
    for (cmd, desc) in HELP_ENTRIES {
        out.push('\n');
        out.push_str(&format!("  {cmd:<max_cmd_width$} — {desc}"));
    }
    out
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
                &format!(
                    "Parish v{} — An Irish Living World Text Adventure",
                    env!("CARGO_PKG_VERSION")
                ),
                "Set in 1820 rural Ireland, powered by the custom Parish engine.",
                "",
                "Created by Dave Mooney © 2026",
                "Licensed under GNU General Public License v3.0.",
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
        Command::Weather(Some(kind)) => match kind.parse::<parish_types::Weather>() {
            Ok(w) => {
                world.weather = w;
                CommandResult::text(format!("Weather forced to {}.", w))
            }
            Err(_) => CommandResult::text(
                "Unknown weather. Try: Clear, Partly Cloudy, Overcast, \
                 Light Rain, Heavy Rain, Fog, Storm.",
            ),
        },
        Command::Weather(None) => {
            CommandResult::text(format!("Current weather: {}.", world.weather))
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
                // Auto-fill any unset model fields with the provider's preset
                // (base + per-role) so users who only set the provider get
                // sensible defaults.
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
                // Auto-fill the model for this category if unset, using the
                // new provider's preset for this role.
                config.fill_missing_models_from_presets();
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

        // ── Provider presets ────────────────────────────────────────────
        Command::ShowPreset => CommandResult::text(
            "Usage: /preset <provider>. Providers with presets: anthropic, openai, google, \
             groq, xai, mistral, deepseek, together, openrouter, ollama, lmstudio, vllm",
        ),
        Command::ApplyPreset(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                if !provider.has_preset() {
                    CommandResult::text(format!(
                        "No preset available for '{}'. Configure models manually with /model.<category>.",
                        name
                    ))
                } else {
                    let presets = provider.preset_models();
                    let provider_name = format!("{:?}", provider).to_lowercase();
                    let default_url = provider.default_base_url().to_string();

                    // Base provider/url/model: use Dialogue's pick as the base model
                    // so any code path that still falls through to `model_name` gets
                    // a sensible value.
                    config.provider_name = provider_name.clone();
                    config.base_url = default_url.clone();
                    if let Some(m) = presets[InferenceCategory::Dialogue.idx()] {
                        config.model_name = m.to_string();
                    }

                    // Per-category: always overwrite (applying a preset is an
                    // explicit user action). API keys are intentionally left
                    // alone — see hint below.
                    for cat in InferenceCategory::ALL {
                        let idx = cat.idx();
                        config.category_provider[idx] = Some(provider_name.clone());
                        config.category_base_url[idx] = Some(default_url.clone());
                        config.category_model[idx] = presets[idx].map(str::to_string);
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
            // When disabling a flag that has associated cached state, clear
            // that state immediately so the next render sees the correct value
            // without requiring the player to run another command first.
            if name == "reveal-unexplored" {
                config.reveal_unexplored_locations = false;
            }
            CommandResult::with_effect(
                format!("Feature '{}' disabled.", name),
                CommandEffect::SaveFlags,
            )
        }
        Command::InvalidFlagName(msg) => CommandResult::text(msg),

        // ── Mode-specific commands (delegated to backend) ───────────────
        Command::Quit => CommandResult::effect_only(CommandEffect::Quit),
        Command::Help => CommandResult::text_tabular(render_help_text()),
        Command::Save => CommandResult::effect_only(CommandEffect::SaveGame),
        Command::Fork(name) => CommandResult::effect_only(CommandEffect::ForkBranch(name)),
        Command::Load(name) => CommandResult::effect_only(CommandEffect::LoadBranch(name)),
        Command::Branches => CommandResult::effect_only(CommandEffect::ListBranches),
        Command::Log => CommandResult::effect_only(CommandEffect::ShowLog),
        Command::Map(arg) => handle_map_command(config, arg),
        Command::Unexplored(arg) => handle_unexplored_command(config, arg),
        Command::Designer => CommandResult::effect_only(CommandEffect::OpenDesigner),
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
        },
    }
}

/// Handles the `/weather` command.
///
/// Handles the `/unexplored` command (reveal/hide all unexplored map locations).
///
/// Gated by the `reveal-unexplored` feature flag (default-enabled per
/// CLAUDE.md rule #6). Uses `is_disabled` semantics so the feature ships
/// on without needing to seed the flags file.
fn handle_unexplored_command(config: &mut GameConfig, arg: Option<bool>) -> CommandResult {
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
fn handle_map_command(config: &mut GameConfig, arg: Option<String>) -> CommandResult {
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

    // ── Additional coverage for previously untested Command variants ─────────

    #[test]
    fn about_command_returns_game_blurb() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::About, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("Parish"));
        assert!(result.response.contains("/help"));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn help_command_lists_commands() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Help, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("/help"));
        assert!(result.response.contains("/save"));
        assert!(result.response.contains("/pause"));
        let about_pos = result
            .response
            .find("/about")
            .expect("help text should include /about");
        let help_pos = result
            .response
            .find("/help")
            .expect("help text should include /help");
        let time_pos = result
            .response
            .find("/time")
            .expect("help text should include /time");
        assert!(about_pos < help_pos);
        assert!(help_pos < time_pos);
        assert!(result.effects.is_empty());
    }

    #[test]
    fn wait_command_advances_clock() {
        let (mut world, mut npc, mut config) = default_state();
        let start = world.clock.now();
        let result = handle_command(Command::Wait(30), &mut world, &mut npc, &mut config);
        let end = world.clock.now();
        let delta = (end - start).num_minutes();
        assert_eq!(delta, 30);
        assert!(result.response.contains("30 minutes"));
    }

    #[test]
    fn tick_command_with_empty_roster() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Tick, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("No NPC activity"));
    }

    #[test]
    fn show_speed_reports_current_speed() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::ShowSpeed, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("Speed:"));
    }

    #[test]
    fn set_speed_updates_clock() {
        use parish_types::time::GameSpeed;
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetSpeed(GameSpeed::Fast),
            &mut world,
            &mut npc,
            &mut config,
        );
        // Activation message should be non-empty; speed should be Fast.
        assert!(!result.response.is_empty());
        assert_eq!(world.clock.current_speed(), Some(GameSpeed::Fast));
    }

    #[test]
    fn invalid_speed_reports_hint() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::InvalidSpeed("warp".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("warp"));
        assert!(result.response.contains("slow"));
    }

    #[test]
    fn invalid_branch_name_returns_msg() {
        let (mut world, mut npc, mut config) = default_state();
        let msg = "Branch name too long.".to_string();
        let result = handle_command(
            Command::InvalidBranchName(msg.clone()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(result.response, msg);
    }

    #[test]
    fn invalid_flag_name_returns_msg() {
        let (mut world, mut npc, mut config) = default_state();
        let msg = "Flag name cannot be empty.".to_string();
        let result = handle_command(
            Command::InvalidFlagName(msg.clone()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(result.response, msg);
    }

    #[test]
    fn toggle_sidebar_returns_message() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::ToggleSidebar, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("sidebar"));
    }

    #[test]
    fn set_model_updates_config() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetModel("qwen3:14b".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(config.model_name, "qwen3:14b");
        assert!(result.response.contains("qwen3:14b"));
    }

    #[test]
    fn show_key_not_set() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::ShowKey, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("not set"));
    }

    #[test]
    fn show_key_masks_when_set() {
        let (mut world, mut npc, mut config) = default_state();
        config.api_key = Some("sk-abcdefghijklmnop".to_string());
        let result = handle_command(Command::ShowKey, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("API key"));
        // Full key must not leak.
        assert!(!result.response.contains("abcdefghijklmnop"));
    }

    #[test]
    fn show_provider_reflects_config() {
        let (mut world, mut npc, mut config) = default_state();
        config.provider_name = "lmstudio".to_string();
        let result = handle_command(Command::ShowProvider, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("lmstudio"));
    }

    #[test]
    fn set_provider_invalid_returns_error() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetProvider("bogus".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        // Invalid provider should not trigger a rebuild.
        assert!(!result.effects.contains(&CommandEffect::RebuildInference));
    }

    // ── Cloud provider commands ──────────────────────────────────────────────

    #[test]
    fn show_cloud_not_configured() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::ShowCloud, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("No cloud provider"));
    }

    #[test]
    fn show_cloud_configured() {
        let (mut world, mut npc, mut config) = default_state();
        config.cloud_provider_name = Some("openrouter".to_string());
        config.cloud_model_name = Some("anthropic/claude-3-haiku".to_string());
        let result = handle_command(Command::ShowCloud, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("openrouter"));
        assert!(result.response.contains("claude-3-haiku"));
    }

    #[test]
    fn set_cloud_provider_triggers_rebuild() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetCloudProvider("openrouter".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("openrouter"));
        assert!(result.effects.contains(&CommandEffect::RebuildCloudClient));
        assert_eq!(config.cloud_provider_name.as_deref(), Some("openrouter"));
    }

    #[test]
    fn set_cloud_model_updates_config() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetCloudModel("gpt-4o".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(config.cloud_model_name.as_deref(), Some("gpt-4o"));
        assert!(result.response.contains("gpt-4o"));
    }

    #[test]
    fn set_cloud_key_triggers_cloud_rebuild() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetCloudKey("sk-cloud-secret".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.contains(&CommandEffect::RebuildCloudClient));
        assert_eq!(config.cloud_api_key.as_deref(), Some("sk-cloud-secret"));
    }

    #[test]
    fn show_cloud_model_not_set() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::ShowCloudModel, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("not set"));
    }

    #[test]
    fn show_cloud_key_masks_when_set() {
        let (mut world, mut npc, mut config) = default_state();
        config.cloud_api_key = Some("sk-cloudabcd1234".to_string());
        let result = handle_command(Command::ShowCloudKey, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("Cloud API key"));
        assert!(!result.response.contains("cloudabcd1234"));
    }

    // ── Category-specific commands ───────────────────────────────────────────

    #[test]
    fn set_category_provider_stores_and_triggers_rebuild() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetCategoryProvider(InferenceCategory::Dialogue, "openrouter".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.contains(&CommandEffect::RebuildInference));
        let idx = GameConfig::cat_idx(InferenceCategory::Dialogue);
        assert_eq!(config.category_provider[idx].as_deref(), Some("openrouter"));
    }

    #[test]
    fn set_category_model_stores_override() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetCategoryModel(InferenceCategory::Simulation, "mini-model".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        let idx = GameConfig::cat_idx(InferenceCategory::Simulation);
        assert_eq!(config.category_model[idx].as_deref(), Some("mini-model"));
        assert!(result.response.contains("mini-model"));
    }

    #[test]
    fn show_category_model_inherits_base() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::ShowCategoryModel(InferenceCategory::Intent),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("inherits base"));
    }

    #[test]
    fn show_category_key_not_set() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::ShowCategoryKey(InferenceCategory::Reaction),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("not set"));
    }

    #[test]
    fn set_category_key_triggers_rebuild() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetCategoryKey(InferenceCategory::Dialogue, "sk-cat-key".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.contains(&CommandEffect::RebuildInference));
        let idx = GameConfig::cat_idx(InferenceCategory::Dialogue);
        assert_eq!(config.category_api_key[idx].as_deref(), Some("sk-cat-key"));
    }

    // ── Provider presets ────────────────────────────────────────────────────

    #[test]
    fn apply_preset_anthropic_populates_all_four_slots() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::ApplyPreset("anthropic".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.contains(&CommandEffect::RebuildInference));
        assert_eq!(config.provider_name, "anthropic");
        assert_eq!(config.base_url, "https://api.anthropic.com");
        assert_eq!(config.model_name, "claude-opus-4-7");

        let idx_d = InferenceCategory::Dialogue.idx();
        let idx_s = InferenceCategory::Simulation.idx();
        let idx_i = InferenceCategory::Intent.idx();
        let idx_r = InferenceCategory::Reaction.idx();
        assert_eq!(
            config.category_model[idx_d].as_deref(),
            Some("claude-opus-4-7")
        );
        assert_eq!(
            config.category_model[idx_s].as_deref(),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            config.category_model[idx_i].as_deref(),
            Some("claude-haiku-4-5")
        );
        assert_eq!(
            config.category_model[idx_r].as_deref(),
            Some("claude-sonnet-4-6")
        );
        for cat in InferenceCategory::ALL {
            let i = cat.idx();
            assert_eq!(config.category_provider[i].as_deref(), Some("anthropic"));
            assert_eq!(
                config.category_base_url[i].as_deref(),
                Some("https://api.anthropic.com")
            );
        }
    }

    #[test]
    fn apply_preset_overwrites_existing_category_models() {
        let (mut world, mut npc, mut config) = default_state();
        let idx = InferenceCategory::Dialogue.idx();
        config.category_model[idx] = Some("old-dialogue-model".to_string());

        handle_command(
            Command::ApplyPreset("ollama".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(config.category_model[idx].as_deref(), Some("qwen3:32b"));
    }

    #[test]
    fn apply_preset_does_not_touch_api_keys() {
        let (mut world, mut npc, mut config) = default_state();
        let idx = InferenceCategory::Dialogue.idx();
        config.api_key = Some("sk-existing".to_string());
        config.category_api_key[idx] = Some("sk-cat".to_string());

        handle_command(
            Command::ApplyPreset("anthropic".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(config.api_key.as_deref(), Some("sk-existing"));
        assert_eq!(config.category_api_key[idx].as_deref(), Some("sk-cat"));
    }

    #[test]
    fn apply_preset_hints_when_api_key_missing() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::ApplyPreset("openai".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("API key"));
        assert!(result.effects.contains(&CommandEffect::RebuildInference));
    }

    #[test]
    fn apply_preset_no_hint_for_keyless_provider() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::ApplyPreset("ollama".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(!result.response.contains("API key"));
    }

    #[test]
    fn apply_preset_unknown_provider_returns_error() {
        let (mut world, mut npc, mut config) = default_state();
        let prior_provider = config.provider_name.clone();
        let result = handle_command(
            Command::ApplyPreset("not-a-provider".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(!result.effects.contains(&CommandEffect::RebuildInference));
        // Config should not have been mutated on error.
        assert_eq!(config.provider_name, prior_provider);
    }

    #[test]
    fn apply_preset_custom_returns_no_preset_message() {
        let (mut world, mut npc, mut config) = default_state();
        let prior_provider = config.provider_name.clone();
        let result = handle_command(
            Command::ApplyPreset("custom".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("No preset"));
        assert!(!result.effects.contains(&CommandEffect::RebuildInference));
        assert_eq!(config.provider_name, prior_provider);
    }

    #[test]
    fn set_provider_fills_missing_models_from_preset() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::SetProvider("anthropic".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.contains(&CommandEffect::RebuildInference));
        assert_eq!(config.provider_name, "anthropic");
        assert_eq!(config.model_name, "claude-opus-4-7");
        // All four per-category slots should be filled from the Anthropic preset.
        assert_eq!(
            config.category_model[InferenceCategory::Intent.idx()].as_deref(),
            Some("claude-haiku-4-5"),
        );
        assert_eq!(
            config.category_model[InferenceCategory::Simulation.idx()].as_deref(),
            Some("claude-sonnet-4-6"),
        );
    }

    #[test]
    fn set_provider_does_not_overwrite_existing_model() {
        let mut config = GameConfig {
            model_name: "preferred-model".to_string(),
            ..GameConfig::default()
        };
        let mut world = WorldState::new();
        let mut npc = NpcManager::new();
        handle_command(
            Command::SetProvider("anthropic".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(config.model_name, "preferred-model");
    }

    #[test]
    fn set_category_provider_fills_missing_model_from_preset() {
        let (mut world, mut npc, mut config) = default_state();
        handle_command(
            Command::SetCategoryProvider(InferenceCategory::Intent, "anthropic".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(
            config.category_model[InferenceCategory::Intent.idx()].as_deref(),
            Some("claude-haiku-4-5"),
        );
    }

    #[test]
    fn show_preset_lists_providers() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::ShowPreset, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("anthropic"));
        assert!(result.response.contains("ollama"));
    }

    // ── Feature flags ────────────────────────────────────────────────────────

    #[test]
    fn flag_list_empty() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Flag(FlagSubcommand::List),
            &mut world,
            &mut npc,
            &mut config,
        );
        // Either empty-state message or flag header — depends on default flags.
        assert!(
            result.response.contains("No feature flags")
                || result.response.contains("Feature flags")
        );
    }

    #[test]
    fn flag_enable_triggers_save() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Flag(FlagSubcommand::Enable("my-feature".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.contains(&CommandEffect::SaveFlags));
        assert!(result.response.contains("my-feature"));
        assert!(result.response.contains("enabled"));
    }

    #[test]
    fn flag_disable_triggers_save() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Flag(FlagSubcommand::Disable("my-feature".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.contains(&CommandEffect::SaveFlags));
        assert!(result.response.contains("disabled"));
    }

    #[test]
    fn flag_disable_reveal_unexplored_clears_active_reveal_state() {
        let (mut world, mut npc, mut config) = default_state();
        // Simulate: reveal mode is active (e.g. player ran `/unexplored reveal`).
        config.reveal_unexplored_locations = true;
        // Operator runs `/flag disable reveal-unexplored` — this must immediately
        // clear the cached reveal state, not wait for the next `/unexplored` call.
        let result = handle_command(
            Command::Flag(FlagSubcommand::Disable("reveal-unexplored".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.contains(&CommandEffect::SaveFlags));
        assert!(result.response.contains("disabled"));
        assert!(
            !config.reveal_unexplored_locations,
            "reveal_unexplored_locations must be cleared immediately when the flag is disabled"
        );
    }

    #[test]
    fn flags_alias_matches_list() {
        let (mut world, mut npc, mut config) = default_state();
        let flags_result = handle_command(Command::Flags, &mut world, &mut npc, &mut config);
        let list_result = handle_command(
            Command::Flag(FlagSubcommand::List),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(flags_result.response, list_result.response);
    }

    // ── Effect-only commands ─────────────────────────────────────────────────

    #[test]
    fn save_returns_save_effect() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Save, &mut world, &mut npc, &mut config);
        assert!(result.response.is_empty());
        assert!(result.effects.contains(&CommandEffect::SaveGame));
    }

    #[test]
    fn fork_returns_fork_effect() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Fork("experiment".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(
            result
                .effects
                .contains(&CommandEffect::ForkBranch("experiment".to_string()))
        );
    }

    #[test]
    fn load_returns_load_effect() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Load("main".to_string()),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(
            result
                .effects
                .contains(&CommandEffect::LoadBranch("main".to_string()))
        );
    }

    #[test]
    fn branches_returns_list_effect() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Branches, &mut world, &mut npc, &mut config);
        assert!(result.effects.contains(&CommandEffect::ListBranches));
    }

    #[test]
    fn log_returns_show_log_effect() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Log, &mut world, &mut npc, &mut config);
        assert!(result.effects.contains(&CommandEffect::ShowLog));
    }

    #[test]
    fn new_game_returns_effect() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::NewGame, &mut world, &mut npc, &mut config);
        assert!(result.effects.contains(&CommandEffect::NewGame));
    }

    #[test]
    fn spinner_returns_effect_with_seconds() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Spinner(5), &mut world, &mut npc, &mut config);
        assert!(result.effects.contains(&CommandEffect::ShowSpinner(5)));
    }

    #[test]
    fn debug_returns_effect_with_subcommand() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Debug(Some("schedule".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(
            result
                .effects
                .contains(&CommandEffect::Debug(Some("schedule".to_string())))
        );
    }

    #[test]
    fn debug_no_subcommand() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Debug(None), &mut world, &mut npc, &mut config);
        assert!(result.effects.contains(&CommandEffect::Debug(None)));
    }

    // ── Theme ────────────────────────────────────────────────────────────────

    #[test]
    fn theme_no_arg_lists_available() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Theme(None), &mut world, &mut npc, &mut config);
        assert!(result.response.contains("default"));
        assert!(result.response.contains("solarized"));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn theme_default_applies_default() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Theme(Some("default".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.iter().any(|e| matches!(
            e,
            CommandEffect::ApplyTheme(name, _) if name == "default"
        )));
    }

    #[test]
    fn theme_solarized_defaults_to_auto() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Theme(Some("solarized".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.iter().any(|e| matches!(
            e,
            CommandEffect::ApplyTheme(name, mode) if name == "solarized" && mode == "auto"
        )));
    }

    #[test]
    fn theme_solarized_with_explicit_mode() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Theme(Some("solarized dark".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.effects.iter().any(|e| matches!(
            e,
            CommandEffect::ApplyTheme(name, mode) if name == "solarized" && mode == "dark"
        )));
    }

    #[test]
    fn theme_unknown_name_returns_error() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Theme(Some("neon".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("neon"));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn theme_solarized_invalid_mode() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Theme(Some("solarized taupe".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("taupe"));
        assert!(result.effects.is_empty());
    }

    // ── NpcsHere with population ─────────────────────────────────────────────

    #[test]
    fn npcs_here_lists_present_npcs() {
        // Use the full GameTestHarness via the default state + direct roster inspection.
        // We can't cheaply populate an NpcManager from scratch here, so we only assert
        // the branch is reachable via the empty path; the populated path is covered by
        // integration tests in crates/parish-cli/tests/.
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::NpcsHere, &mut world, &mut npc, &mut config);
        // Falls through to the "No one else is here." branch.
        assert!(result.response.contains("No one"));
    }

    // ── /map command ───────────────────────────────────────────────────────

    fn seed_tile_sources(config: &mut GameConfig) {
        config.tile_sources = vec![
            ("osm".to_string(), "OpenStreetMap".to_string()),
            (
                "historic".to_string(),
                "Historic 6\" OS Ireland (1st ed., via NLS)".to_string(),
            ),
        ];
        config.active_tile_source = "osm".to_string();
    }

    #[test]
    fn map_list_when_no_arg() {
        let (mut world, mut npc, mut config) = default_state();
        seed_tile_sources(&mut config);
        let result = handle_command(Command::Map(None), &mut world, &mut npc, &mut config);
        assert!(result.response.contains("osm"));
        assert!(result.response.contains("historic"));
        assert!(result.response.contains("(active)"));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn map_list_empty_registry() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Map(None), &mut world, &mut npc, &mut config);
        assert!(result.response.contains("No tile sources configured"));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn map_switch_sets_config_and_emits_effect() {
        let (mut world, mut npc, mut config) = default_state();
        seed_tile_sources(&mut config);
        let result = handle_command(
            Command::Map(Some("historic".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(config.active_tile_source, "historic");
        assert!(result.response.contains("Switched"));
        assert_eq!(
            result.effects,
            vec![CommandEffect::ApplyTiles("historic".to_string())]
        );
    }

    #[test]
    fn map_switch_is_case_insensitive() {
        let (mut world, mut npc, mut config) = default_state();
        seed_tile_sources(&mut config);
        let result = handle_command(
            Command::Map(Some("OSM".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert_eq!(config.active_tile_source, "osm");
        assert_eq!(
            result.effects,
            vec![CommandEffect::ApplyTiles("osm".to_string())]
        );
    }

    #[test]
    fn map_unknown_id_returns_error_text() {
        let (mut world, mut npc, mut config) = default_state();
        seed_tile_sources(&mut config);
        let result = handle_command(
            Command::Map(Some("made-up".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("Unknown"));
        assert!(result.response.contains("made-up"));
        assert!(result.response.contains("osm"));
        assert!(result.effects.is_empty());
        assert_eq!(config.active_tile_source, "osm", "active unchanged");
    }

    #[test]
    fn map_disabled_flag_returns_refusal() {
        let (mut world, mut npc, mut config) = default_state();
        seed_tile_sources(&mut config);
        config.flags.disable("period-map-tiles");
        let result = handle_command(
            Command::Map(Some("historic".to_string())),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("/flag enable"));
        assert!(result.effects.is_empty());
        assert_eq!(config.active_tile_source, "osm", "active unchanged");
    }

    #[test]
    fn map_help_lists_command() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Help, &mut world, &mut npc, &mut config);
        assert!(result.response.contains("/map"));
        assert!(result.response.contains("/unexplored"));
    }

    #[test]
    fn unexplored_reveal_updates_config() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(
            Command::Unexplored(Some(true)),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(config.reveal_unexplored_locations);
        assert!(result.response.contains("revealed"));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn unexplored_hide_updates_config() {
        let (mut world, mut npc, mut config) = default_state();
        config.reveal_unexplored_locations = true;
        let result = handle_command(
            Command::Unexplored(Some(false)),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(!config.reveal_unexplored_locations);
        assert!(result.response.contains("hidden"));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn unexplored_none_reports_status_and_usage() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Unexplored(None), &mut world, &mut npc, &mut config);
        assert!(result.response.contains("currently hidden"));
        assert!(result.response.contains("/unexplored reveal|hide"));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn unexplored_disabled_flag_returns_refusal() {
        let (mut world, mut npc, mut config) = default_state();
        config.flags.disable("reveal-unexplored");
        let result = handle_command(
            Command::Unexplored(Some(true)),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("/flag enable"));
        assert!(result.effects.is_empty());
        assert!(!config.reveal_unexplored_locations);
    }

    /// Codex P1: disabling the flag while reveal is already active must clear
    /// `reveal_unexplored_locations`, making the kill-switch effective.
    /// Previously the early return left the boolean true, so map rendering
    /// continued to show unexplored areas even though the feature flag was off.
    #[test]
    fn unexplored_disabled_flag_clears_active_reveal_state() {
        let (mut world, mut npc, mut config) = default_state();
        // Simulate: player ran `/unexplored reveal` while flag was enabled.
        config.reveal_unexplored_locations = true;
        // Now an operator disables the feature flag.
        config.flags.disable("reveal-unexplored");
        // Any attempt to use /unexplored should clear reveal state, not just refuse.
        let result = handle_command(
            Command::Unexplored(Some(true)),
            &mut world,
            &mut npc,
            &mut config,
        );
        assert!(result.response.contains("/flag enable"));
        assert!(result.effects.is_empty());
        // Kill-switch must be complete: reveal state cleared even though we
        // could not execute the command.
        assert!(
            !config.reveal_unexplored_locations,
            "reveal_unexplored_locations must be false when the feature flag is disabled"
        );
    }

    #[test]
    fn unexplored_disabled_flag_clears_active_reveal() {
        let (mut world, mut npc, mut config) = default_state();
        config.reveal_unexplored_locations = true;
        config.flags.disable("reveal-unexplored");
        let result = handle_command(Command::Unexplored(None), &mut world, &mut npc, &mut config);
        assert!(result.response.contains("/flag enable"));
        assert!(
            !config.reveal_unexplored_locations,
            "should clear reveal state when flag is disabled"
        );
    }

    #[test]
    fn help_output_is_tabular_and_column_aligned() {
        let (mut world, mut npc, mut config) = default_state();
        let result = handle_command(Command::Help, &mut world, &mut npc, &mut config);

        assert_eq!(result.presentation, TextPresentation::Tabular);

        // Every row after the "Available commands:" header must contain
        // exactly one em-dash separator, and all em-dashes must share the
        // same character column — that's what makes the list tabular in a
        // monospace font.
        let mut dash_col: Option<usize> = None;
        for line in result.response.lines().skip(1) {
            let matches: Vec<usize> = line.match_indices('—').map(|(i, _)| i).collect();
            assert_eq!(
                matches.len(),
                1,
                "help row should contain exactly one em-dash: {:?}",
                line
            );
            let col = line[..matches[0]].chars().count();
            match dash_col {
                None => dash_col = Some(col),
                Some(expected) => {
                    assert_eq!(col, expected, "em-dash column mismatch on row: {:?}", line)
                }
            }
        }
        assert!(dash_col.is_some(), "help body had no rows");
    }
}
