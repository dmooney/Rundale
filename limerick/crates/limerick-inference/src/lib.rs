//! LLM inference pipeline: queue, rate-limit, and dispatch to any provider
//! (OpenAI-compatible / Anthropic Messages API / offline Simulator).
//!
//! The *transport* half — the concrete provider HTTP clients, the offline
//! simulator/mock backends, the unified `AnyClient` dispatch enum, and the
//! outbound rate limiter — lives in the [`limerick_providers`] crate. This
//! crate owns the *scheduling* half (queue, worker, priority lanes, timeout,
//! validation, file logging) and re-exports every moved transport symbol at
//! its former `limerick_inference::*` path so downstream consumers need no
//! import changes.

pub mod client;
pub mod file_log;
pub mod hf_downloader;
pub mod logs;
pub mod queue;
pub mod secret_scrub;
pub mod timeout;
pub mod validate;
pub mod worker;

// ── Transport modules: re-exported from limerick-providers ──────────────────────
//
// These module paths are load-bearing — downstream crates reference
// `limerick_inference::openai_client::OpenAiClient`,
// `limerick_inference::simulator::CORPUS`, `limerick_inference::any_client::*`,
// etc. Re-exporting the whole module keeps those paths valid without a single
// import change, and lets the staying modules (queue/worker/timeout) keep
// their internal `crate::any_client::…` / `crate::openai_client::…` references.
pub use limerick_providers::parse_generation_json;
pub use limerick_providers::{
    anthropic_client, any_client, discovery, fetch_catalog_endpoint, google_client, mock_client,
    openai_client, rate_limit, simulator,
};

// ── Setup/bootstrap module: re-exported from limerick-setup ─────────────────────
//
// The local-inference bootstrap (GPU detect, model select, Ollama/vllm
// provider bootstrap, orchestration) was extracted into the `limerick-setup`
// crate. Downstream consumers reach it via `limerick_inference::setup::*` (e.g.
// `limerick_core::inference::setup::setup_provider_client`,
// `limerick_engine::inference::setup::StdoutProgress`). Re-exporting the whole
// crate as `setup` keeps every one of those paths valid without a single
// import change. The dependency edge is one-directional: limerick-setup depends
// on limerick-providers (not on this crate), so there is no cycle.
pub use limerick_setup as setup;

// ── Re-exports: public API (unchanged paths for downstream crates) ────────────

pub use anthropic_client::AnthropicClient;
pub use google_client::{
    GenerationResult, GoogleClient, ProviderCallError, ProviderMetadata, ProviderUsage,
    ServiceTier, ThinkingLevel,
};
pub use limerick_config::InferenceConfig;
pub use rate_limit::InferenceRateLimiter;

pub use any_client::{
    AnyClient, InferenceClients, TOKEN_CHANNEL_CAPACITY, build_client, build_client_v2,
    build_inference_clients_v2, generate_params_v2,
};
pub use logs::{
    BoundedInferenceLog, DeferredInferenceAudit, DirectInferenceAudit, InferenceAuditSink,
    InferenceLog, InferenceLogEntry, new_inference_log, new_inference_log_with_config,
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
pub use worker::{
    InferenceWorkerConfig, spawn_inference_worker, spawn_inference_worker_with_clients,
};
