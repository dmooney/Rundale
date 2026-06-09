//! Player / Judge actors. The seam that lets the run loop drive either
//! deterministic scripted actors (CI, no key) or `parish-inference`-backed LLM
//! actors (Anthropic / OpenAI-compat / local vllm-mlx).

pub mod api;
pub mod scripted;
pub mod subagent;
pub mod traits;
pub mod types;
pub(crate) mod verdict;

pub use api::{ApiJudge, ApiPlayer, make_client};
pub use scripted::{ScriptedJudge, ScriptedPlayer};
pub use subagent::SubagentJudge;
pub use traits::{Judge, Player};
pub use types::{
    JudgeVerdict, Observation, PlayerMove, RunTranscript, TurnTranscript, summarize_state,
};
