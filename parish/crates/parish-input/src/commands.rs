//! System command types and validation helpers.
//!
//! Defines the [`Command`] and [`FlagSubcommand`] enums that represent
//! recognised `/`-prefixed meta commands, along with validation helpers
//! used by the parser to sanitise branch and flag names.

use parish_config::InferenceCategory;
use parish_types::GameSpeed;

/// Sub-command for the `/flag` feature-flag system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlagSubcommand {
    /// Enable the named flag.
    Enable(String),
    /// Disable the named flag.
    Disable(String),
    /// List all known flags.
    List,
}

/// A system command entered by the player.
///
/// System commands use a `/` prefix and control game meta-operations
/// like saving, loading, and quitting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Freeze all simulation ticks.
    Pause,
    /// Unfreeze simulation.
    Resume,
    /// Persist state and exit.
    Quit,
    /// Manual snapshot to current branch.
    Save,
    /// Create a new named save branch.
    Fork(String),
    /// Load a named save branch.
    Load(String),
    /// List all save branches.
    Branches,
    /// Show history of current branch.
    Log,
    /// Show current game status.
    Status,
    /// Show help text.
    Help,
    /// Toggle the Irish pronunciation sidebar.
    ToggleSidebar,
    /// Toggle improv craft mode for NPC dialogue.
    ToggleImprov,
    /// Show current LLM provider.
    ShowProvider,
    /// Change LLM provider at runtime.
    SetProvider(String),
    /// Show current model name.
    ShowModel,
    /// Change model name at runtime.
    SetModel(String),
    /// Show current API key (masked).
    ShowKey,
    /// Set API key at runtime.
    SetKey(String),
    /// Debug command with optional subcommand.
    Debug(Option<String>),
    /// Show the loading spinner for a duration (seconds).
    Spinner(u64),
    /// Show cloud provider info.
    ShowCloud,
    /// Change cloud provider at runtime.
    SetCloudProvider(String),
    /// Show cloud model name.
    ShowCloudModel,
    /// Change cloud model at runtime.
    SetCloudModel(String),
    /// Show cloud API key (masked).
    ShowCloudKey,
    /// Set cloud API key at runtime.
    SetCloudKey(String),
    /// Show current game speed.
    ShowSpeed,
    /// Set game speed to a named preset.
    SetSpeed(GameSpeed),
    /// Invalid speed preset was requested.
    InvalidSpeed(String),
    /// Show provider for a specific inference category.
    ShowCategoryProvider(InferenceCategory),
    /// Set provider for a specific inference category.
    SetCategoryProvider(InferenceCategory, String),
    /// Show model for a specific inference category.
    ShowCategoryModel(InferenceCategory),
    /// Set model for a specific inference category.
    SetCategoryModel(InferenceCategory, String),
    /// Show API key for a specific inference category (masked).
    ShowCategoryKey(InferenceCategory),
    /// Set API key for a specific inference category.
    SetCategoryKey(InferenceCategory, String),
    /// Apply a recommended provider preset across all inference categories.
    ApplyPreset(String),
    /// Show usage / list of providers with available presets.
    ShowPreset,
    /// Show about / credits information.
    About,
    /// Show or change the map tile source. No arg = list sources; arg = switch to it.
    Map(Option<String>),
    /// Open the Parish Designer mod editor.
    Designer,
    /// Show NPCs at the current location with details.
    NpcsHere,
    /// Show detailed time, weather, and season info.
    Time,
    /// Wait in place for a number of game minutes, advancing time.
    Wait(u32),
    /// Start a fresh new game, resetting world and NPCs.
    NewGame,
    /// Manually tick NPC schedules without advancing time.
    Tick,
    /// Show or change the UI theme.
    Theme(Option<String>),
    /// Show or set whether unexplored map locations are fully revealed.
    ///
    /// `None` reports current usage/status text.
    /// `Some(true)` reveals all unexplored locations.
    /// `Some(false)` restores normal fog-of-war frontier visibility.
    Unexplored(Option<bool>),
    /// Invalid branch name was provided.
    InvalidBranchName(String),
    /// Feature flag management (`/flag enable|disable|list <name>`).
    Flag(FlagSubcommand),
    /// List all feature flags (alias for `Flag(List)`).
    Flags,
    /// Invalid flag name was provided.
    InvalidFlagName(String),
    /// Show or set the current weather.
    ///
    /// `None` reports the current weather; `Some(name)` forces a weather
    /// state for play-testing (accepts `clear`, `partly cloudy`,
    /// `overcast`, `light rain`, `heavy rain`, `fog`, `storm`, and
    /// common aliases).
    Weather(Option<String>),
}

/// Maximum allowed length for save branch names.
const MAX_BRANCH_NAME_LEN: usize = 255;

/// Maximum allowed length for feature flag names.
const MAX_FLAG_NAME_LEN: usize = 64;

/// Validates a save branch name for length and allowed characters.
///
/// Branch names may contain alphanumerics, spaces, underscores, and hyphens.
pub(crate) fn validate_branch_name(name: &str) -> Result<String, String> {
    if name.len() > MAX_BRANCH_NAME_LEN {
        return Err(format!(
            "Branch name too long (max {} characters).",
            MAX_BRANCH_NAME_LEN
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ')
    {
        return Err(
            "Branch names may only contain letters, numbers, spaces, underscores, and hyphens."
                .to_string(),
        );
    }
    Ok(name.to_string())
}

/// Validates a feature flag name for length and allowed characters.
///
/// Flag names may contain alphanumerics, hyphens, and underscores only.
pub(crate) fn validate_flag_name(name: &str) -> Result<String, String> {
    if name.is_empty() {
        return Err("Flag name cannot be empty.".to_string());
    }
    if name.len() > MAX_FLAG_NAME_LEN {
        return Err(format!(
            "Flag name too long (max {} characters).",
            MAX_FLAG_NAME_LEN
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(
            "Flag names may only contain letters, digits, hyphens, and underscores.".to_string(),
        );
    }
    Ok(name.to_string())
}
