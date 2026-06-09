//! The Player / Judge seam.
//!
//! The run loop is written against these traits, so the player and judge can be
//! either the deterministic scripted impls (no key, CI) or the
//! `parish-inference`-backed LLM impls (Anthropic / OpenAI-compat / local
//! vllm-mlx) without the loop knowing the difference. A future
//! Claude-Code-subagent impl slots in here too.

use async_trait::async_trait;

use crate::error::Result;

use super::types::{JudgeVerdict, Observation, PlayerMove, RunTranscript};

/// Chooses the next player input given the current observation.
#[async_trait]
pub trait Player: Send + Sync {
    async fn choose_action(&self, obs: &Observation<'_>) -> Result<PlayerMove>;
}

/// Scores a finished run transcript: axis scores + discrete findings.
#[async_trait]
pub trait Judge: Send + Sync {
    /// The pinned rubric hash this judge scored against (recorded on the run).
    fn rubric_sha256(&self) -> &str;
    async fn judge_run(&self, transcript: &RunTranscript) -> Result<JudgeVerdict>;
}
