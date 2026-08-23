//! Provider transport for Parish LLM inference.
//!
//! This crate owns the *transport* half of the inference pipeline: the
//! concrete HTTP clients for each provider (OpenAI-compatible, Anthropic
//! Messages API), the offline [`simulator`] and scriptable [`mock_client`]
//! backends, the unified [`any_client::AnyClient`] dispatch enum, and the
//! outbound [`rate_limit::InferenceRateLimiter`]. The *scheduling* half —
//! queue, worker, priority lanes, validation, file logging — lives in
//! `parish-inference`, which depends on this crate and re-exports every
//! public symbol below at its former `parish_inference::*` path so
//! downstream consumers need no import changes.
//!
//! `reqwest` (and the `governor` rate limiter) are contained here; no other
//! backend-agnostic crate pulls an HTTP stack.

pub mod anthropic_client;
pub mod any_client;
pub(crate) mod client_base;
pub mod discovery;
pub mod google_client;
pub mod mock_client;
pub mod openai_client;
pub mod openai_responses_client;
pub mod probe;
pub mod rate_limit;
pub(crate) mod retry;
pub mod simulator;
pub(crate) mod utf8_stream;

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

/// Parses a detailed provider result as JSON without discarding its transport
/// metadata. A malformed structured response becomes a [`ProviderCallError`]
/// so the common audit path records it as a failed call rather than a
/// misleading provider success.
pub fn parse_generation_json<T: serde::de::DeserializeOwned>(
    result: GenerationResult,
    context: &str,
) -> Result<(GenerationResult, T), ProviderCallError> {
    match serde_json::from_str(strip_json_fence(&result.text)) {
        Ok(parsed) => Ok((result, parsed)),
        Err(error) => Err(ProviderCallError {
            message: format!("{context} JSON parse failed: {error}"),
            partial_text: result.text,
            metadata: Box::new(result.metadata),
        }),
    }
}

// ── Public API re-exports ─────────────────────────────────────────────────────

pub use anthropic_client::AnthropicClient;
pub use any_client::{
    AnyClient, InferenceClients, TOKEN_CHANNEL_CAPACITY, build_client, build_client_v2,
    build_inference_clients_v2, generate_params_v2,
};
pub use discovery::fetch_catalog_endpoint;
pub use google_client::{
    GenerationResult, GoogleClient, ProviderCallError, ProviderMetadata, ProviderUsage,
    ServiceTier, ThinkingLevel,
};
pub use mock_client::{MockClient, MockMatcher};
pub use openai_client::{GenerateParams, JsonSchemaSpec, ResponseFormat};
pub use openai_responses_client::OpenAiResponsesClient;
pub use probe::{
    RawProbeResponse, is_definitive_model_not_found, probe_route_raw, validate_probe_response,
};
pub use rate_limit::InferenceRateLimiter;
