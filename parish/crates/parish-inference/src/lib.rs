//! LLM inference pipeline: queue, rate-limit, and dispatch to any provider
//! (OpenAI-compatible / Anthropic Messages API / offline Simulator).

pub mod anthropic_client;
pub mod any_client;
pub mod client;
pub(crate) mod client_base;
pub mod file_log;
pub mod hf_downloader;
pub mod logs;
pub mod mock_client;
pub mod openai_client;
pub mod queue;
pub mod rate_limit;
pub mod secret_scrub;
pub mod setup;
pub mod simulator;
pub mod timeout;
pub(crate) mod utf8_stream;
pub mod validate;
pub mod worker;

/// Result of processing a single SSE line.
pub(crate) enum SseResult {
    /// Continue reading more lines.
    Continue,
    /// Stream is complete.
    Done,
    /// An error event was received mid-stream.
    Error(String),
}

/// Strips Markdown JSON code fences (`` ```json `` or `` ``` ``) from a string.
pub(crate) fn strip_json_fence(raw: &str) -> &str {
    let t = raw.trim();
    if let Some(inner) = t.strip_prefix("```json") {
        return inner
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    if let Some(inner) = t.strip_prefix("```") {
        return inner
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    t
}

// ── Re-exports: public API (unchanged paths for downstream crates) ────────────

pub use anthropic_client::AnthropicClient;
pub use parish_config::InferenceConfig;
pub use rate_limit::InferenceRateLimiter;

pub use any_client::{AnyClient, InferenceClients, TOKEN_CHANNEL_CAPACITY, build_client};
pub use logs::{
    BoundedInferenceLog, InferenceLog, InferenceLogEntry, new_inference_log,
    new_inference_log_with_config,
};
pub use mock_client::{MockClient, MockMatcher};
pub use openai_client::{GenerateParams, JsonSchemaSpec, ResponseFormat};
pub use queue::{
    CancellationToken, InferencePriority, InferenceQueue, InferenceRequest, InferenceResponse,
    QueueRequest,
};
pub use timeout::{
    INFERENCE_RESPONSE_TIMEOUT_SECS, InferenceAwaitOutcome, QUEUE_REQUEST_ID,
    await_inference_response, submit_json, submit_json_streaming,
};
pub use worker::{InferenceWorkerConfig, spawn_inference_worker};
