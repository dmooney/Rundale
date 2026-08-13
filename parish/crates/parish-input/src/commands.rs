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

/// Sub-command for the `/inference-log` disk-log toggle.
///
/// Controls the same writer used by the on-disk inference + chat
/// transcript logs that ship in user-zippable bug reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceLogSub {
    /// Re-enable writes after a previous off.
    On,
    /// Stop writing future records (writer task stays alive).
    Off,
    /// Show on/off status and the file path.
    Status,
    /// Show the inference-log file path.
    Path,
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
    /// Freeze simulation without emitting a user-visible message.
    ///
    /// Used by the frontend's `visibilitychange` / focus-loss handler so that
    /// switching away from the window pauses the clock (NPC ticks stop) but
    /// does **not** print "The clocks of the parish stand still." in the chat
    /// log.  User-typed `/pause` still goes through [`Command::Pause`] and
    /// emits the full message.
    PauseSilent,
    /// Unfreeze simulation without emitting a user-visible message.
    ///
    /// Symmetric counterpart to [`Command::PauseSilent`].  Used when the
    /// window regains focus so the clock resumes silently.
    ResumeSilent,
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
    /// Show the base provider URL.
    ShowBaseUrl,
    /// Set the base provider URL (the Dialogue/base slot). Needed for BYOK
    /// dialogue targets whose URL differs from the provider default.
    SetBaseUrl(String),
    /// Show base URL for a specific inference category.
    ShowCategoryBaseUrl(InferenceCategory),
    /// Set base URL for a specific inference category (multi-slot loadouts +
    /// BYOK targets whose URL differs from the provider default).
    SetCategoryBaseUrl(InferenceCategory, String),
    /// Apply a recommended provider preset across all inference categories.
    ApplyPreset(String),
    /// Show usage / list of providers with available presets.
    ShowPreset,
    /// Re-open the BYOK provider picker (wipes the .onboarded sentinel so the
    /// next launch would too).
    ResetByok,
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
    /// An unrecognised slash-prefixed command was entered.
    ///
    /// Keeping unknown slash input in the system-command channel prevents a
    /// misspelling or stale advertised command from reaching NPC inference.
    InvalidSystemCommand(String),
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
    /// Listen to the traditional music session at the pub.
    ///
    /// Location-gated (the pub) and time-gated (dusk through midnight).
    /// Produces a short evocative vignette generated from the game clock.
    Session,
    /// Toggle or inspect the on-disk inference log.
    ///
    /// `On`/`Off` flip the shared enable flag at runtime.
    /// `Status` reports whether logging is active and the file path.
    /// `Path` prints the file path only.
    InferenceLog(InferenceLogSub),
}

/// Maximum allowed length for save branch names.
///
/// Canonical limit shared by all entry points (Tauri IPC, HTTP server, CLI parser).
/// Must match the server's HTTP 400 boundary and the Tauri validation gate.
pub const MAX_BRANCH_NAME_LEN: usize = 64;

/// Maximum allowed length for feature flag names.
const MAX_FLAG_NAME_LEN: usize = 64;

/// Validates a save branch name for length and allowed characters.
///
/// Branch names must be non-empty, at most [`MAX_BRANCH_NAME_LEN`] ASCII
/// characters, and may only contain alphanumerics, spaces, underscores, and
/// hyphens.  This is the single canonical implementation — all entry points
/// (Tauri IPC, HTTP server, CLI parser) call this function so the rule is
/// never duplicated.
pub fn validate_branch_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    if name.len() > MAX_BRANCH_NAME_LEN {
        return Err(format!(
            "Branch name too long (max {} characters).",
            MAX_BRANCH_NAME_LEN
        ));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b' ')
    {
        return Err(
            "Branch names may only contain letters, numbers, spaces, underscores, and hyphens."
                .to_string(),
        );
    }
    Ok(())
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_branch_name_valid() {
        assert!(validate_branch_name("my-save").is_ok());
        assert!(validate_branch_name("save_1").is_ok());
        assert!(validate_branch_name("My Save Game").is_ok());
    }
    #[test]
    fn test_validate_branch_name_empty_is_rejected() {
        let err = validate_branch_name("").unwrap_err();
        assert!(err.contains("cannot be empty"), "got: {err}");
    }
    #[test]
    fn test_validate_branch_name_too_long() {
        // Canonical cap is 64 — 65 chars must be rejected.
        let long_name = "a".repeat(65);
        assert!(validate_branch_name(&long_name).is_err());
    }
    #[test]
    fn test_validate_branch_name_invalid_chars() {
        assert!(validate_branch_name("save/game").is_err());
        assert!(validate_branch_name("save;drop").is_err());
        assert!(validate_branch_name("../../etc").is_err());
    }
    // --- validate_flag_name tests ---
    #[test]
    fn test_validate_flag_name_valid() {
        assert!(validate_flag_name("experimental").is_ok());
        assert!(validate_flag_name("my-flag").is_ok());
        assert!(validate_flag_name("test_flag").is_ok());
        assert!(validate_flag_name("a1b2c3").is_ok());
    }
    #[test]
    fn test_validate_flag_name_empty() {
        let err = validate_flag_name("").unwrap_err();
        assert!(err.contains("cannot be empty"));
    }
    #[test]
    fn test_validate_flag_name_too_long() {
        let long_name = "a".repeat(65);
        let err = validate_flag_name(&long_name).unwrap_err();
        assert!(err.contains("max 64"));
    }
    #[test]
    fn test_validate_flag_name_at_max_length() {
        let name = "a".repeat(64);
        assert!(validate_flag_name(&name).is_ok());
    }
    #[test]
    fn test_validate_flag_name_invalid_chars() {
        assert!(validate_flag_name("bad name").is_err());
        assert!(validate_flag_name("bad/name").is_err());
        assert!(validate_flag_name("bad.name").is_err());
        assert!(validate_flag_name("bad!flag").is_err());
        assert!(validate_flag_name("bad@flag").is_err());
    }
    // --- validate_branch_name edge cases ---
    #[test]
    fn test_validate_branch_name_at_max_length() {
        // Exactly 64 chars must be accepted.
        let name = "a".repeat(64);
        assert!(validate_branch_name(&name).is_ok());
    }
    #[test]
    fn test_validate_branch_name_just_over_max() {
        // 65 chars must be rejected with the canonical message.
        let name = "a".repeat(65);
        let err = validate_branch_name(&name).unwrap_err();
        assert!(err.contains("max 64"), "got: {err}");
    }
    #[test]
    fn test_validate_branch_name_with_special_chars() {
        assert!(validate_branch_name("save!game").is_err());
        assert!(validate_branch_name("save@game").is_err());
        assert!(validate_branch_name("save#game").is_err());
        assert!(validate_branch_name("save.game").is_err());
    }
}
