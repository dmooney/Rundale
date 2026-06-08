//! Build the player + judge actors from a `RunConfig` and a pinned `Rubric`.
//!
//! Extracted from `main.rs` so the worker subcommand can reuse it without
//! duplicating the actor construction logic.

use std::time::Duration;

use crate::actor::{
    ApiJudge, ApiPlayer, Judge, Player, ScriptedJudge, ScriptedPlayer, SubagentJudge, make_client,
};
use crate::config::{ActorMode, RunConfig};
use crate::error::Result;
use crate::persist::default_db_path;
use crate::score::Rubric;

/// Default subagent queue directory: a `subagent-queue` sibling of the harness DB.
fn default_subagent_queue_dir() -> std::path::PathBuf {
    default_db_path()
        .parent()
        .map(|p| p.join("subagent-queue"))
        .unwrap_or_else(|| std::path::PathBuf::from("subagent-queue"))
}

/// Default subagent judge timeout (30 minutes).
const SUBAGENT_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Build the player + judge actors for the configured modes.
///
/// Returns `(player, judge)` ready to pass to [`crate::run::RunParams`].
pub fn build_actors(
    config: &RunConfig,
    rubric: &Rubric,
) -> Result<(Box<dyn Player>, Box<dyn Judge>)> {
    let rubric_sha = rubric.sha256.clone();
    let player: Box<dyn Player> = match config.player.mode {
        ActorMode::Scripted => Box::new(ScriptedPlayer::default()),
        ActorMode::Api => {
            let key = std::env::var("ANTHROPIC_API_KEY").ok();
            let client = make_client("anthropic", None, key.as_deref())?;
            let model = config
                .player
                .model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            Box::new(ApiPlayer::new(client, model))
        }
        // Subagent player is not yet implemented; fall back to scripted so the
        // judge can still run subagent-mode scoring.
        ActorMode::Subagent => Box::new(ScriptedPlayer::default()),
    };
    let judge: Box<dyn Judge> = match config.judge.mode {
        ActorMode::Scripted => Box::new(ScriptedJudge::new(rubric_sha)),
        ActorMode::Api => {
            let key = std::env::var("ANTHROPIC_API_KEY").ok();
            let client = make_client("anthropic", None, key.as_deref())?;
            let model = config
                .judge
                .model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            Box::new(ApiJudge::new(
                client,
                model,
                rubric.manifest.rubric.clone(),
                rubric_sha,
            ))
        }
        ActorMode::Subagent => Box::new(SubagentJudge::new(
            default_subagent_queue_dir(),
            rubric_sha,
            SUBAGENT_TIMEOUT,
        )),
    };
    Ok((player, judge))
}
