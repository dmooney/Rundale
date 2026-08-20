//! Session and pacing timeouts (`[engine.session]`).

use serde::Deserialize;

/// Session and pacing timeouts.
#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionConfig {
    /// Real-time silence threshold before nearby NPCs may start banter.
    #[serde(default = "default_idle_banter_after_secs")]
    pub idle_banter_after_secs: u64,
    /// Real-time inactivity threshold before the game auto-pauses.
    #[serde(default = "default_auto_pause_after_secs")]
    pub auto_pause_after_secs: u64,
    /// Maximum number of concurrent in-memory sessions per server process.
    ///
    /// When the live session count reaches this ceiling, new session
    /// creation is refused with `503 Service Unavailable`.  Each session
    /// holds ~50 MB of game state (world graph, NPC manager, inference
    /// queue), so this knob is the primary lever for the per-process memory
    /// budget: `max_concurrent_sessions * ~50 MB ≈ memory ceiling`.
    ///
    /// Override at runtime with the `PARISH_MAX_SESSIONS` environment
    /// variable (takes precedence over both the TOML setting and this
    /// compiled-in default).
    ///
    /// Defaults to 50, matching the ceiling noted in issue #620.
    #[serde(default = "default_max_concurrent_sessions")]
    pub max_concurrent_sessions: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            idle_banter_after_secs: default_idle_banter_after_secs(),
            auto_pause_after_secs: default_auto_pause_after_secs(),
            max_concurrent_sessions: default_max_concurrent_sessions(),
        }
    }
}

fn default_idle_banter_after_secs() -> u64 {
    120
}
fn default_auto_pause_after_secs() -> u64 {
    300
}
fn default_max_concurrent_sessions() -> usize {
    50
}
