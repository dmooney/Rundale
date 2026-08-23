//! Response types returned by [`handle_command`](super::handle_command).

use crate::input::InferenceLogSub;

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
    /// Persist the current feature flag state to disk.
    SaveFlags,
    /// Apply a user-selected UI theme; frontend resolves the actual palette colors.
    /// Carries (theme_name, mode) where mode is "light", "dark", "auto", or "".
    ApplyTheme(String, String),
    /// Switch the full-map base tile source. Carries the source id
    /// (e.g. "osm", "historic") — frontend looks up URL etc.
    /// from the tile registry it received via `UiConfigSnapshot`.
    ApplyTiles(String),
    /// Wipe BYOK config (keychain entry, parish.toml, .onboarded sentinel,
    /// GameConfig.api_key) and signal the frontend to re-open the fork
    /// screen via `EVENT_SETUP_NEEDS_ONBOARDING`.
    ResetByok,
    /// Toggle or inspect the on-disk inference log. The runtime is
    /// responsible for flipping the `InferenceFileLog` /
    /// `ChatTranscriptLog` enable flag and replying with a status string.
    InferenceLog(InferenceLogSub),
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
    pub(super) fn text(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            effects: vec![],
            presentation: TextPresentation::Prose,
        }
    }

    pub(super) fn text_tabular(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            effects: vec![],
            presentation: TextPresentation::Tabular,
        }
    }

    pub(super) fn with_effect(response: impl Into<String>, effect: CommandEffect) -> Self {
        Self {
            response: response.into(),
            effects: vec![effect],
            presentation: TextPresentation::Prose,
        }
    }

    pub(super) fn effect_only(effect: CommandEffect) -> Self {
        Self {
            response: String::new(),
            effects: vec![effect],
            presentation: TextPresentation::Prose,
        }
    }
}
