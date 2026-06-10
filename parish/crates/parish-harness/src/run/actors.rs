//! Build the player + judge actors from a `RunConfig` and a pinned `Rubric`.
//!
//! Extracted from `main.rs` so the worker subcommand can reuse it without
//! duplicating the actor construction logic.

use std::time::Duration;

use crate::actor::{
    ApiJudge, ApiPlayer, Judge, Player, ScriptedJudge, ScriptedPlayer, SubagentJudge, make_client,
};
use crate::config::{ActorMode, RunConfig};
use crate::error::{HarnessError, Result};
use crate::persist::default_db_path;
use crate::score::Rubric;

/// Default cloud provider for the API-backed player + judge (#1363). Both run
/// on the harness operator's own key (resolved from env), kept separate from
/// the game's NPC models (which may be local Qwen via #1364).
const DEFAULT_ACTOR_PROVIDER: &str = "anthropic";

/// Resolves the API key for the API-backed player/judge from the environment,
/// returning a clear, actionable error when it's missing (#1363 AC3).
///
/// Without this, `build_client` would receive `None` and the run would only
/// fail deep inside the first turn with an opaque 401 — far from the operator's
/// invocation. We fail fast at actor construction instead.
fn resolve_actor_key(provider_name: &str, role: &str) -> Result<String> {
    let provider = parish_core::config::Provider::from_str_loose(provider_name).map_err(|e| {
        HarnessError::Config(format!("unknown {role} provider {provider_name:?}: {e}"))
    })?;
    let var = provider.api_key_env_var().ok_or_else(|| {
        HarnessError::Config(format!(
            "{role} provider {provider_name:?} has no API-key env var; \
             use a cloud provider (e.g. anthropic) for --{role} api"
        ))
    })?;
    std::env::var(var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            HarnessError::Config(format!(
                "missing API key for the {role}: set {var} in the environment \
                 (the autonomous --{role} api path is driven only by env keys)."
            ))
        })
}

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
            // Fail fast with a clear error when the env key is missing (#1363
            // AC3) instead of constructing an unauthorised client.
            let key = resolve_actor_key(DEFAULT_ACTOR_PROVIDER, "player")?;
            let client = make_client(DEFAULT_ACTOR_PROVIDER, None, Some(&key))?;
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
            let key = resolve_actor_key(DEFAULT_ACTOR_PROVIDER, "judge")?;
            let client = make_client(DEFAULT_ACTOR_PROVIDER, None, Some(&key))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// AC3 (#1363): an unknown provider name yields a clear Config error naming
    /// the role — env-independent, so safe to run without serialisation.
    #[test]
    fn resolve_actor_key_unknown_provider_is_clear_error() {
        let err = resolve_actor_key("definitely-not-a-provider", "player").unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, HarnessError::Config(_)), "got {err:?}");
        assert!(msg.contains("player"), "error should name the role: {msg}");
    }

    /// AC3 (#1363): a keyless provider (no `api_key_env_var`) used for `--judge
    /// api` is rejected with a clear message — also env-independent.
    #[test]
    fn resolve_actor_key_keyless_provider_is_clear_error() {
        // `simulator` is a local provider with no API-key env var.
        let err = resolve_actor_key("simulator", "judge").unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, HarnessError::Config(_)), "got {err:?}");
        assert!(
            msg.contains("no API-key env var") && msg.contains("judge"),
            "expected keyless-provider error naming the role, got {msg}"
        );
    }
}
