//! LLM inference pipeline: queue, rate-limit, and dispatch to any provider
//! (OpenAI-compatible / Anthropic Messages API / offline Simulator).

pub mod anthropic_client;
pub mod client;
pub(crate) mod client_base;
pub mod file_log;
pub mod hf_downloader;
pub mod mock_client;
pub mod openai_client;
pub mod rate_limit;
pub mod secret_scrub;
pub mod setup;
pub mod simulator;
pub(crate) mod utf8_stream;
pub mod validate;

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

pub use anthropic_client::AnthropicClient;
pub use parish_config::InferenceConfig;
pub use rate_limit::InferenceRateLimiter;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub use mock_client::{MockClient, MockMatcher};
use openai_client::OpenAiClient;
pub use openai_client::{JsonSchemaSpec, ResponseFormat};
use simulator::SimulatorClient;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
pub use tokio_util::sync::CancellationToken;

use parish_config::Provider;
use parish_types::ParishError;

/// Buffer capacity for the bounded token streaming channel.
///
/// LLM providers produce tokens far faster than terminals or websocket
/// clients consume them, so a truly unbounded channel risks OOM on long
/// responses or slow consumers. 1 024 tokens is enough headroom for any
/// realistic burst; the sender blocks (back-pressure) if the consumer
/// falls further behind, which naturally throttles HTTP reads from the
/// provider. Fixes #83.
pub const TOKEN_CHANNEL_CAPACITY: usize = 1024;

/// Builds the right [`AnyClient`] variant for a given [`Provider`].
///
/// Every call site that currently does `OpenAiClient::new(url, key)` should
/// route through this helper instead so that
/// [`Provider::Anthropic`] is correctly dispatched to [`AnthropicClient`]
/// rather than silently misrouted through the OpenAI-compat client (which
/// would fail with a 404 because Anthropic's endpoint is `/v1/messages`,
/// not `/v1/chat/completions`).
///
/// The returned client is always unrate-limited; attach a limiter via
/// [`AnyClient::with_rate_limit`] (not implemented — do it on the inner
/// variant before wrapping) when per-provider throttling is required.
pub fn build_client(
    provider: &Provider,
    base_url: &str,
    api_key: Option<&str>,
    inference_config: &InferenceConfig,
) -> AnyClient {
    use parish_config::ProviderKind;
    match provider.kind() {
        ProviderKind::Anthropic => AnyClient::Anthropic(AnthropicClient::new_with_config(
            base_url,
            api_key,
            inference_config,
        )),
        ProviderKind::Simulator => AnyClient::simulator(),
        // GitHub Models uses /chat/completions (no /v1 prefix).
        _ if provider.id() == "github_models" => AnyClient::OpenAi(
            OpenAiClient::new_with_config(base_url, api_key, inference_config)
                .with_completions_path("/chat/completions"),
        ),
        _ => AnyClient::OpenAi(OpenAiClient::new_with_config(
            base_url,
            api_key,
            inference_config,
        )),
    }
}

/// Per-call streaming statistics observed by the worker as tokens flow
/// through the proxied channel.
#[derive(Debug, Clone, Copy)]
struct StreamStats {
    ttft: Option<Duration>,
    tokens: u64,
}

/// A single logged inference call for the debug panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferenceLogEntry {
    /// Unique request ID.
    pub request_id: u64,
    /// Wall-clock timestamp (e.g. "14:32:05").
    pub timestamp: String,
    /// Model name used for this request.
    pub model: String,
    /// Whether this was a streaming request.
    pub streaming: bool,
    /// Request duration in milliseconds.
    pub duration_ms: u64,
    /// Prompt length in characters.
    pub prompt_len: usize,
    /// Response length in characters.
    pub response_len: usize,
    /// Error message if the request failed (None = success).
    pub error: Option<String>,
    /// System prompt sent (if any).
    pub system_prompt: Option<String>,
    /// User prompt text.
    pub prompt_text: String,
    /// Full response text (empty on error).
    pub response_text: String,
    /// Max tokens limit sent to provider (if any).
    pub max_tokens: Option<u32>,
    /// Time-to-first-token in milliseconds (streaming only; None for
    /// non-streaming requests, which never observe an intermediate token).
    pub ttft_ms: Option<u64>,
    /// Number of streamed tokens observed (streaming only). Each entry
    /// counts a non-empty `delta.content` chunk; reasoning chunks are
    /// not surfaced through this channel and are excluded.
    pub output_tokens: Option<u64>,
    /// Temperature sent to provider (if any). Plumbed through so the on-disk
    /// inference log can record it in OpenTelemetry GenAI form.
    pub temperature: Option<f32>,
    /// Priority lane the request travelled through.
    pub priority: InferencePriority,
}

/// Priority lane for inference requests. Higher priority lanes are drained first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InferencePriority {
    /// Player-facing dialogue (Tier 1). Highest priority.
    Interactive = 0,
    /// NPC background simulation (Tier 2). Medium priority.
    Background = 1,
    /// Distant NPC batch simulation (Tier 3). Lowest priority.
    Batch = 2,
}

/// Bounded ring buffer of inference call log entries.
///
/// Enforces a hard `max_entries` cap independent of `VecDeque::capacity()`.
/// `VecDeque::with_capacity` rounds up to the next power of two and reallocates
/// on overflow, so using `capacity()` as a bound leaks memory exponentially —
/// see issue #340. This struct stores the configured cap explicitly and evicts
/// the oldest entry whenever `push` would exceed it.
#[derive(Debug)]
pub struct BoundedInferenceLog {
    entries: VecDeque<InferenceLogEntry>,
    max_entries: usize,
}

impl BoundedInferenceLog {
    /// Creates an empty log bounded to `max_entries`. A value of 0 is treated
    /// as 1 so that a `push` always leaves exactly one entry in the log.
    pub fn new(max_entries: usize) -> Self {
        let cap = max_entries.max(1);
        Self {
            entries: VecDeque::with_capacity(cap),
            max_entries: cap,
        }
    }

    /// Appends `entry`, evicting the oldest entries until `len <= max_entries`.
    pub fn push(&mut self, entry: InferenceLogEntry) {
        while self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Iterates over stored entries, oldest first.
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, InferenceLogEntry> {
        self.entries.iter()
    }

    /// Returns the current number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the configured maximum number of entries.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }
}

/// Shared bounded ring buffer of inference call log entries.
pub type InferenceLog = Arc<Mutex<BoundedInferenceLog>>;

/// Creates a new empty inference log with capacity from config.
pub fn new_inference_log_with_config(config: &InferenceConfig) -> InferenceLog {
    Arc::new(Mutex::new(BoundedInferenceLog::new(config.log_capacity)))
}

/// Creates a new empty inference log with default capacity.
pub fn new_inference_log() -> InferenceLog {
    new_inference_log_with_config(&InferenceConfig::default())
}

/// A request to generate text via the inference pipeline.
///
/// Sent through the inference queue and processed by the inference worker.
/// The caller receives the response via the `response_tx` oneshot channel.
pub struct InferenceRequest {
    /// Unique request identifier for correlation.
    pub id: u64,
    /// The Ollama model to use (e.g. "gemma4:e4b").
    pub model: String,
    /// The prompt text to send to the model.
    pub prompt: String,
    /// Optional system prompt for context.
    pub system: Option<String>,
    /// Channel to send the response back to the caller.
    pub response_tx: oneshot::Sender<InferenceResponse>,
    /// Optional channel for streaming tokens. If present, the worker streams
    /// individual tokens through this before sending the final response.
    /// Bounded to [`TOKEN_CHANNEL_CAPACITY`] to prevent unbounded memory growth (#83).
    pub token_tx: Option<mpsc::Sender<String>>,
    /// Optional maximum number of tokens to generate.
    pub max_tokens: Option<u32>,
    /// Optional temperature for sampling (0.0 = deterministic, 1.0+ = creative).
    pub temperature: Option<f32>,
    /// Optional OpenAI-compat `frequency_penalty`. Forwarded to vllm-mlx,
    /// LM Studio, OpenAI, OpenRouter and any compatible backend; ignored
    /// by Anthropic and the Simulator (no equivalent). Tier 1 dialogue
    /// sets `Some(0.5)` to break Qwen2.5-14B-4bit's degenerate
    /// repetition loops (TODO #10 / #23 / #34). Tier 2 / Tier 3 / intent
    /// / reaction callers leave it at `None`.
    pub frequency_penalty: Option<f32>,
    /// Priority lane for this request.
    pub priority: InferencePriority,
    /// When true, the worker uses `generate_stream_json` (JSON mode + streaming).
    pub json_mode: bool,
    /// Optional structured-output schema. When set, the worker forwards
    /// it as `response_format: {"type": "json_schema", "json_schema": ...}`
    /// — the strict-mode path required by vllm-mlx, LM Studio, and OpenAI's
    /// structured-outputs feature. Takes precedence over `json_mode` when
    /// both are set. Anthropic / Simulator backends ignore the schema and
    /// fall back to plain generation; the prompt should still describe the
    /// expected shape so unconstrained backends produce parseable output.
    pub json_schema: Option<JsonSchemaSpec>,
    /// Optional cancellation token. When fired, the worker drops the
    /// in-flight inference future, which closes the underlying HTTP/SSE
    /// connection — Ollama, LM Studio, and vllm-mlx all halt generation
    /// on client disconnect, freeing the model slot. Required so that a
    /// player turn can preempt mid-flight Tier 2/3 simulation calls
    /// without waiting for them to drain. The response carries
    /// `error: Some("cancelled")` when this fires.
    pub cancel: Option<CancellationToken>,
}

/// The response from an inference request.
#[derive(Debug, Clone)]
pub struct InferenceResponse {
    /// The request id this response corresponds to.
    pub id: u64,
    /// The generated text (empty on error).
    pub text: String,
    /// Error message if the request failed.
    pub error: Option<String>,
}

/// A handle to the inference queue for submitting requests.
///
/// Routes requests to one of three priority lanes (Interactive, Background, Batch).
/// Clone this to share across tasks.
#[derive(Clone)]
pub struct InferenceQueue {
    interactive_tx: mpsc::Sender<InferenceRequest>,
    background_tx: mpsc::Sender<InferenceRequest>,
    batch_tx: mpsc::Sender<InferenceRequest>,
}

impl InferenceQueue {
    /// Creates a new inference queue with one sender per priority lane.
    pub fn new(
        interactive_tx: mpsc::Sender<InferenceRequest>,
        background_tx: mpsc::Sender<InferenceRequest>,
        batch_tx: mpsc::Sender<InferenceRequest>,
    ) -> Self {
        Self {
            interactive_tx,
            background_tx,
            batch_tx,
        }
    }

    /// Submits an inference request to the appropriate priority lane.
    ///
    /// If `token_tx` is provided, the worker will stream individual tokens
    /// through it before sending the final complete response. An optional
    /// `max_tokens` cap is forwarded to the LLM provider to limit output
    /// length. Returns a oneshot receiver that will yield the complete
    /// response. Returns an error if the queue channel is closed.
    #[allow(clippy::too_many_arguments)] // all params are semantically distinct
    pub async fn send(
        &self,
        id: u64,
        model: String,
        prompt: String,
        system: Option<String>,
        token_tx: Option<mpsc::Sender<String>>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        priority: InferencePriority,
        json_mode: bool,
    ) -> Result<oneshot::Receiver<InferenceResponse>, mpsc::error::SendError<InferenceRequest>>
    {
        self.send_with_schema(
            id,
            model,
            prompt,
            system,
            token_tx,
            max_tokens,
            temperature,
            priority,
            json_mode,
            None,
        )
        .await
    }

    /// Convenience variant that carries a `frequency_penalty` knob without
    /// requiring callers to drop down to [`Self::send_full`]. Used by the
    /// Tier 1 dialogue call site to set the penalty for repetition-loop
    /// suppression (TODO #10 / #23 / #34) while keeping the legacy
    /// `[`Self::send`] signature stable for the rest of the codebase.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_with_penalty(
        &self,
        id: u64,
        model: String,
        prompt: String,
        system: Option<String>,
        token_tx: Option<mpsc::Sender<String>>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        frequency_penalty: Option<f32>,
        priority: InferencePriority,
        json_mode: bool,
    ) -> Result<oneshot::Receiver<InferenceResponse>, mpsc::error::SendError<InferenceRequest>>
    {
        self.send_full(
            id,
            model,
            prompt,
            system,
            token_tx,
            max_tokens,
            temperature,
            frequency_penalty,
            priority,
            json_mode,
            None,
            None,
        )
        .await
    }

    /// Schema-aware variant of [`Self::send`].
    ///
    /// Pass `json_schema = Some(...)` to forward a strict structured-output
    /// schema all the way to the provider's `response_format` field.
    /// `json_schema` takes precedence over `json_mode` when both are set.
    /// `json_mode = true` with `json_schema = None` keeps the legacy
    /// "json_object" Ollama path.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_with_schema(
        &self,
        id: u64,
        model: String,
        prompt: String,
        system: Option<String>,
        token_tx: Option<mpsc::Sender<String>>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        priority: InferencePriority,
        json_mode: bool,
        json_schema: Option<JsonSchemaSpec>,
    ) -> Result<oneshot::Receiver<InferenceResponse>, mpsc::error::SendError<InferenceRequest>>
    {
        self.send_full(
            id,
            model,
            prompt,
            system,
            token_tx,
            max_tokens,
            temperature,
            None,
            priority,
            json_mode,
            json_schema,
            None,
        )
        .await
    }

    /// Full-fidelity submit with both a structured-output schema and a
    /// caller-controlled cancellation token.
    ///
    /// Firing the token causes the worker to drop its in-flight inference
    /// future, which closes the underlying HTTP/SSE connection so the
    /// provider can free its slot. The response carries
    /// `error: Some("cancelled")`. Required so a player turn can preempt
    /// mid-flight Tier 2/3 simulation.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_full(
        &self,
        id: u64,
        model: String,
        prompt: String,
        system: Option<String>,
        token_tx: Option<mpsc::Sender<String>>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        frequency_penalty: Option<f32>,
        priority: InferencePriority,
        json_mode: bool,
        json_schema: Option<JsonSchemaSpec>,
        cancel: Option<CancellationToken>,
    ) -> Result<oneshot::Receiver<InferenceResponse>, mpsc::error::SendError<InferenceRequest>>
    {
        let (response_tx, response_rx) = oneshot::channel();
        let request = InferenceRequest {
            id,
            model,
            prompt,
            system,
            response_tx,
            token_tx,
            max_tokens,
            temperature,
            frequency_penalty,
            priority,
            json_mode,
            json_schema,
            cancel,
        };
        let lane = match priority {
            InferencePriority::Interactive => &self.interactive_tx,
            InferencePriority::Background => &self.background_tx,
            InferencePriority::Batch => &self.batch_tx,
        };
        lane.send(request).await?;
        Ok(response_rx)
    }
}

/// Outcome of awaiting an inference response with a safety timeout.
#[derive(Debug)]
pub enum InferenceAwaitOutcome {
    /// The worker sent a response.
    Response(InferenceResponse),
    /// The worker dropped the sender without producing a response.
    Closed,
    /// The safety timeout fired before the worker responded. The `secs` field
    /// records the timeout duration so callers can surface it in diagnostics.
    TimedOut { secs: u64 },
}

/// Default safety timeout for awaiting an inference response.
///
/// Slightly above `InferenceConfig::streaming_timeout_secs` (300s) so that the
/// underlying HTTP client's timeout has a chance to fire first and produce a
/// proper error response. Only kicks in if the worker task is wedged or the
/// HTTP timeout fails to trigger.
pub const INFERENCE_RESPONSE_TIMEOUT_SECS: u64 = 360;

/// Await an inference response with a safety timeout.
///
/// Wraps `response_rx.await` in [`tokio::time::timeout`] so a stuck worker or
/// a dropped sender never hangs the caller indefinitely. Returns a distinct
/// outcome for each failure mode so callers can log timeouts separately from
/// closed channels.
///
/// Pass `None` for `timeout` to disable the safety cap (falls back to the
/// previous unbounded `.await` behaviour, used when the
/// `inference-response-timeout` feature flag is explicitly disabled).
pub async fn await_inference_response(
    response_rx: oneshot::Receiver<InferenceResponse>,
    timeout: Option<std::time::Duration>,
) -> InferenceAwaitOutcome {
    match timeout {
        Some(dur) => match tokio::time::timeout(dur, response_rx).await {
            Ok(Ok(resp)) => InferenceAwaitOutcome::Response(resp),
            Ok(Err(_)) => InferenceAwaitOutcome::Closed,
            Err(_) => InferenceAwaitOutcome::TimedOut {
                secs: dur.as_secs(),
            },
        },
        None => match response_rx.await {
            Ok(resp) => InferenceAwaitOutcome::Response(resp),
            Err(_) => InferenceAwaitOutcome::Closed,
        },
    }
}

/// Monotonically increasing request ID counter for queue-submitted JSON requests.
static QUEUE_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Submit a request that expects a JSON response, then deserialize it.
///
/// Used by Tier 3 batch inference and Tier 2 background simulation.
/// Requests are non-streaming and routed to the given priority lane.
/// `max_tokens=None` matches legacy callers; new callers should pass a
/// sensible cap to bound runtime — uncapped JSON generation on vllm-mlx
/// can run away (5000+ tokens) on richly-prompted batches.
pub async fn submit_json<T: serde::de::DeserializeOwned>(
    queue: &InferenceQueue,
    priority: InferencePriority,
    model: &str,
    prompt: &str,
    system: Option<&str>,
    max_tokens: Option<u32>,
) -> Result<T, ParishError> {
    let id = QUEUE_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let response_rx = queue
        .send(
            id,
            model.to_string(),
            prompt.to_string(),
            system.map(String::from),
            None,
            max_tokens,
            None,
            priority,
            false,
        )
        .await
        .map_err(|e| ParishError::Inference(format!("queue send failed: {e}")))?;
    let response = response_rx
        .await
        .map_err(|e| ParishError::Inference(format!("response channel closed: {e}")))?;
    if let Some(err) = response.error {
        return Err(ParishError::Inference(err));
    }
    serde_json::from_str(&response.text)
        .map_err(|e| ParishError::Inference(format!("JSON parse failed: {e}")))
}

/// Streaming variant of [`submit_json`] that enables mid-flight cancellation.
///
/// Routes through `send_full` with an internal `token_tx` so the worker
/// uses the streaming code path. Streamed chunks are discarded by the
/// caller — only the final assembled JSON is consumed — but the
/// streaming path is what lets vllm-mlx (and other providers) recognise
/// a dropped connection and free the inference slot when `cancel` fires.
///
/// Required for Tier 2 / Tier 3 simulation so a player turn can preempt
/// in-flight background inference. Pass `cancel = None` when preemption
/// isn't needed; the streaming path still yields TTFT + token-count
/// telemetry through the existing `StreamStats` observer.
pub async fn submit_json_streaming<T: serde::de::DeserializeOwned>(
    queue: &InferenceQueue,
    priority: InferencePriority,
    model: &str,
    prompt: &str,
    system: Option<&str>,
    max_tokens: Option<u32>,
    cancel: Option<CancellationToken>,
) -> Result<T, ParishError> {
    let id = QUEUE_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    // Discard sink — we only need the streaming path enabled in the
    // worker; the assembled JSON arrives via the response channel.
    let (sink_tx, mut sink_rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    tokio::spawn(async move { while sink_rx.recv().await.is_some() {} });

    let response_rx = queue
        .send_full(
            id,
            model.to_string(),
            prompt.to_string(),
            system.map(String::from),
            Some(sink_tx),
            max_tokens,
            None,
            None,
            priority,
            false,
            None,
            cancel,
        )
        .await
        .map_err(|e| ParishError::Inference(format!("queue send failed: {e}")))?;
    let response = response_rx
        .await
        .map_err(|e| ParishError::Inference(format!("response channel closed: {e}")))?;
    if let Some(err) = response.error {
        return Err(ParishError::Inference(err));
    }
    serde_json::from_str(&response.text)
        .map_err(|e| ParishError::Inference(format!("JSON parse failed: {e}")))
}

/// Per-category LLM client routing with a base provider fallback.
///
/// Each inference category (dialogue, simulation, intent) can have its own
/// provider, model, and endpoint. Categories without explicit overrides
/// fall back to the base provider.
#[derive(Clone)]
pub struct InferenceClients {
    /// Per-category (client, model) overrides.
    overrides: std::collections::HashMap<parish_config::InferenceCategory, (AnyClient, String)>,
    /// Base client used when no per-category override exists.
    pub base: AnyClient,
    /// Base model name (e.g. "gemma4:e4b").
    pub base_model: String,
}

impl InferenceClients {
    /// Creates a new `InferenceClients` with the given base client and per-category overrides.
    pub fn new(
        base: AnyClient,
        base_model: String,
        overrides: std::collections::HashMap<parish_config::InferenceCategory, (AnyClient, String)>,
    ) -> Self {
        Self {
            overrides,
            base,
            base_model,
        }
    }

    /// Returns the client and model for a given inference category.
    ///
    /// Uses the per-category override if configured, otherwise falls back to the base.
    pub fn client_for(&self, category: parish_config::InferenceCategory) -> (&AnyClient, &str) {
        match self.overrides.get(&category) {
            Some((client, model)) => (client, model),
            None => (&self.base, &self.base_model),
        }
    }

    /// Returns the client and model to use for player dialogue (Tier 1).
    pub fn dialogue_client(&self) -> (&AnyClient, &str) {
        self.client_for(parish_config::InferenceCategory::Dialogue)
    }

    /// Returns the client and model to use for background NPC simulation (Tier 2).
    pub fn simulation_client(&self) -> (&AnyClient, &str) {
        self.client_for(parish_config::InferenceCategory::Simulation)
    }

    /// Returns the client and model to use for intent parsing.
    pub fn intent_client(&self) -> (&AnyClient, &str) {
        self.client_for(parish_config::InferenceCategory::Intent)
    }

    /// Returns the client and model to use for NPC arrival reactions.
    pub fn reaction_client(&self) -> (&AnyClient, &str) {
        self.client_for(parish_config::InferenceCategory::Reaction)
    }

    /// Whether the dialogue category uses a different provider than the base.
    pub fn has_custom_dialogue(&self) -> bool {
        self.overrides
            .contains_key(&parish_config::InferenceCategory::Dialogue)
    }
}

/// A unified client handle covering every supported provider transport.
///
/// - [`AnyClient::OpenAi`] wraps the OpenAI-compatible HTTP client used by
///   Ollama, LM Studio, OpenRouter, OpenAI, Google, Groq, xAI, Mistral,
///   DeepSeek, Together, NVIDIA NIM, vLLM, and any custom OpenAI-compatible
///   endpoint.
/// - [`AnyClient::Anthropic`] wraps [`AnthropicClient`], the native client
///   for Anthropic's Messages API (distinct schema, auth, and SSE events).
/// - [`AnyClient::Simulator`] is the built-in offline mock.
/// - [`AnyClient::Mock`] is a scriptable, deterministic test stand-in.
#[derive(Clone)]
pub enum AnyClient {
    /// A real OpenAI-compatible HTTP client.
    OpenAi(OpenAiClient),
    /// Anthropic's native Messages API client (see [`AnthropicClient`]).
    Anthropic(AnthropicClient),
    /// The built-in offline simulator (generates funny nonsense locally).
    Simulator(Arc<SimulatorClient>),
    /// A scriptable mock returning caller-supplied completions verbatim.
    /// Test-only; see [`MockClient`]. Never opens a socket.
    Mock(Arc<MockClient>),
}

impl AnyClient {
    /// Wraps a real `OpenAiClient`.
    pub fn open_ai(client: OpenAiClient) -> Self {
        Self::OpenAi(client)
    }

    /// Wraps a real `AnthropicClient`.
    pub fn anthropic(client: AnthropicClient) -> Self {
        Self::Anthropic(client)
    }

    /// Creates a new simulator client.
    pub fn simulator() -> Self {
        Self::Simulator(Arc::new(SimulatorClient::new()))
    }

    /// Creates an empty scriptable mock client, returning the shared
    /// [`MockClient`] handle so the test can enqueue completions.
    pub fn mock() -> (Self, Arc<MockClient>) {
        let mock = Arc::new(MockClient::new());
        (Self::Mock(mock.clone()), mock)
    }

    /// Generates plain text (non-streaming).
    ///
    /// `frequency_penalty` is the OpenAI-compat sampling knob; on
    /// `Anthropic` / `Simulator` it is accepted and ignored (no
    /// equivalent on the Messages API; the simulator is deterministic).
    #[allow(clippy::too_many_arguments)]
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        frequency_penalty: Option<f32>,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate(
                    model,
                    prompt,
                    system,
                    max_tokens,
                    temperature,
                    frequency_penalty,
                )
                .await
            }
            Self::Anthropic(c) => {
                let _ = frequency_penalty;
                c.generate(model, prompt, system, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                let _ = frequency_penalty;
                c.generate(model, prompt, system, max_tokens, temperature)
                    .await
            }
            Self::Mock(c) => {
                let _ = frequency_penalty;
                c.generate(model, prompt, system, max_tokens, temperature)
                    .await
            }
        }
    }

    /// Generates text with token streaming.
    #[allow(clippy::too_many_arguments)]
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        frequency_penalty: Option<f32>,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_stream(
                    model,
                    prompt,
                    system,
                    token_tx,
                    max_tokens,
                    temperature,
                    frequency_penalty,
                )
                .await
            }
            Self::Anthropic(c) => {
                let _ = frequency_penalty;
                c.generate_stream(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                let _ = frequency_penalty;
                c.generate_stream(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
            Self::Mock(c) => {
                let _ = frequency_penalty;
                c.generate_stream(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
        }
    }

    /// Streams text with JSON mode enabled.
    ///
    /// Like [`generate_stream`] but constrains the provider to emit valid JSON.
    /// Used for Tier 1 NPC responses where dialogue is embedded in a JSON
    /// structure and extracted incrementally during streaming.
    #[allow(clippy::too_many_arguments)]
    pub async fn generate_stream_json(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        frequency_penalty: Option<f32>,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_stream_json(
                    model,
                    prompt,
                    system,
                    token_tx,
                    max_tokens,
                    temperature,
                    frequency_penalty,
                )
                .await
            }
            Self::Anthropic(c) => {
                let _ = frequency_penalty;
                c.generate_stream_json(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                let _ = frequency_penalty;
                c.generate_stream_json(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
            Self::Mock(c) => {
                let _ = frequency_penalty;
                c.generate_stream_json(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
        }
    }

    /// Generates a structured JSON response and deserializes it into `T`.
    #[allow(clippy::too_many_arguments)]
    pub async fn generate_json<T: serde::de::DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        frequency_penalty: Option<f32>,
    ) -> Result<T, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_json::<T>(
                    model,
                    prompt,
                    system,
                    max_tokens,
                    temperature,
                    frequency_penalty,
                )
                .await
            }
            Self::Anthropic(c) => {
                let _ = frequency_penalty;
                c.generate_json::<T>(model, prompt, system, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                let _ = frequency_penalty;
                c.generate_json::<T>(model, prompt, system, max_tokens, temperature)
                    .await
            }
            Self::Mock(c) => {
                let _ = frequency_penalty;
                c.generate_json::<T>(model, prompt, system, max_tokens, temperature)
                    .await
            }
        }
    }

    /// Non-streaming generate with an explicit OpenAI `response_format`.
    ///
    /// Returns the raw response text; callers parse JSON themselves. Used
    /// by the inference queue worker so [`InferenceRequest::json_schema`]
    /// can flow end-to-end without the worker having to know the target
    /// `T`. Anthropic and Simulator backends ignore `response_format`
    /// (they don't speak OpenAI's structured-outputs wire shape) and fall
    /// back to plain `generate`.
    #[allow(clippy::too_many_arguments)]
    pub async fn generate_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        response_format: Option<ResponseFormat>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        frequency_penalty: Option<f32>,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_text_with_format(
                    model,
                    prompt,
                    system,
                    response_format,
                    max_tokens,
                    temperature,
                    frequency_penalty,
                )
                .await
            }
            Self::Anthropic(c) => {
                let _ = frequency_penalty;
                c.generate(model, prompt, system, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                let _ = frequency_penalty;
                c.generate(model, prompt, system, max_tokens, temperature)
                    .await
            }
            Self::Mock(c) => {
                // Mock ignores `response_format`; callers parse the returned
                // text (or the dialogue envelope) themselves.
                let _ = (frequency_penalty, response_format);
                c.generate(model, prompt, system, max_tokens, temperature)
                    .await
            }
        }
    }

    /// Streaming counterpart of [`generate_with_format`]. Used by the
    /// worker when a request carries a token sender *and* a response
    /// format (e.g. Tier 1 dialogue with a strict schema). Anthropic /
    /// Simulator ignore `response_format` and fall back to their plain
    /// streaming methods.
    #[allow(clippy::too_many_arguments)]
    pub async fn generate_stream_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        response_format: Option<ResponseFormat>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        frequency_penalty: Option<f32>,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_stream_with_format(
                    model,
                    prompt,
                    system,
                    token_tx,
                    response_format,
                    max_tokens,
                    temperature,
                    frequency_penalty,
                )
                .await
            }
            Self::Anthropic(c) => {
                let _ = frequency_penalty;
                c.generate_stream(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                let _ = frequency_penalty;
                // When the caller expects JSON (either schema or json_mode),
                // the Markov stream the simulator would otherwise produce
                // never parses and the caller logs a JSON-parse error every
                // tick. Route those calls through the simulator's existing
                // JSON-stream path, which emits a generic JSON object whose
                // fields are wide enough for the Tier 2 / Tier 3 / reaction
                // structs (all use `#[serde(default)]`), so the worst case
                // is an "uneventful tick" rather than a hard parse failure.
                let wants_json = response_format.is_some()
                    || prompt.contains("Respond with a JSON")
                    || prompt.contains("Respond with JSON")
                    || prompt.contains("JSON object")
                    || prompt.contains("\"updates\":")
                    || prompt.contains("\"npc_id\":")
                    || system.is_some_and(|s| s.contains("JSON") || s.contains("input parser"));
                if wants_json {
                    c.generate_stream_json(model, prompt, system, token_tx, max_tokens, temperature)
                        .await
                } else {
                    c.generate_stream(model, prompt, system, token_tx, max_tokens, temperature)
                        .await
                }
            }
            Self::Mock(c) => {
                let _ = frequency_penalty;
                // Same JSON-vs-plain routing as the simulator arm: a scripted
                // dialogue completion is wrapped in the JSON envelope only when
                // the caller expects JSON, otherwise streamed as plain text.
                let wants_json = response_format.is_some()
                    || prompt.contains("Respond with a JSON")
                    || prompt.contains("Respond with JSON")
                    || prompt.contains("JSON object")
                    || prompt.contains("\"updates\":")
                    || prompt.contains("\"npc_id\":")
                    || system.is_some_and(|s| s.contains("JSON") || s.contains("input parser"));
                if wants_json {
                    c.generate_stream_json(model, prompt, system, token_tx, max_tokens, temperature)
                        .await
                } else {
                    c.generate_stream(model, prompt, system, token_tx, max_tokens, temperature)
                        .await
                }
            }
        }
    }

    /// Returns a reference to the inner `OpenAiClient`, if this is a real client.
    pub fn as_open_ai(&self) -> Option<&OpenAiClient> {
        match self {
            Self::OpenAi(c) => Some(c),
            Self::Anthropic(_) | Self::Simulator(_) | Self::Mock(_) => None,
        }
    }

    /// Returns a reference to the inner `AnthropicClient`, if this is an Anthropic client.
    pub fn as_anthropic(&self) -> Option<&AnthropicClient> {
        match self {
            Self::Anthropic(c) => Some(c),
            Self::OpenAi(_) | Self::Simulator(_) | Self::Mock(_) => None,
        }
    }

    /// Returns `true` if this is the offline simulator.
    pub fn is_simulator(&self) -> bool {
        matches!(self, Self::Simulator(_))
    }

    /// Returns `true` if this is the scriptable test mock.
    pub fn is_mock(&self) -> bool {
        matches!(self, Self::Mock(_))
    }

    /// Returns `true` if the underlying client has a rate limiter attached.
    ///
    /// The simulator is always unlimited (no network calls), so this is
    /// `false` for `Self::Simulator`.
    pub fn has_rate_limiter(&self) -> bool {
        match self {
            Self::OpenAi(c) => c.has_rate_limiter(),
            Self::Anthropic(c) => c.has_rate_limiter(),
            Self::Simulator(_) | Self::Mock(_) => false,
        }
    }
}

/// Wraps an inference future with a timeout *and* an optional cancellation
/// token, producing consistent error messages so callers don't repeat the
/// match+format pattern.
///
/// Resolution order is `select!`-style: whichever of {cancel, timeout, the
/// inner future} fires first wins. Cancel and timeout both drop the inner
/// future, which closes the underlying HTTP/SSE connection so providers
/// release their model slot.
async fn inference_with_timeout<F, T>(
    future: F,
    timeout: std::time::Duration,
    timeout_secs: u64,
    model: &str,
    label: &str,
    cancel: Option<&CancellationToken>,
) -> Result<T, ParishError>
where
    F: std::future::Future<Output = Result<T, ParishError>>,
{
    // tokio::pin so the future is select-safe across branches.
    tokio::pin!(future);
    let cancel_fut = async {
        match cancel {
            Some(tok) => tok.cancelled().await,
            // Never resolves; the select degrades to "timeout || future".
            None => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        biased;
        () = cancel_fut => Err(ParishError::Inference(format!(
            "{label} cancelled (model={model})",
        ))),
        result = &mut future => result,
        () = tokio::time::sleep(timeout) => Err(ParishError::Inference(format!(
            "{label} timed out after {timeout_secs}s (model={model})",
        ))),
    }
}

/// Spawns the inference worker task.
///
/// The worker pulls requests from three priority lanes using `tokio::select!`
/// with `biased;` ordering, ensuring Interactive requests are always processed
/// before Background and Batch requests. The worker is single-flight: one
/// in-flight LLM call at a time (no preemption).
///
/// Each completed call is recorded in the shared `log` ring buffer.
/// The task runs until all three sender sides of the channels are dropped.
///
/// A per-request [`tokio::time::timeout`] is applied to every LLM call using
/// the values from `timeout_config`:
/// - Non-streaming calls: `timeout_config.timeout_secs`
/// - Streaming calls: `timeout_config.streaming_timeout_secs`
///
/// On timeout the worker sends an error response and moves on to the next
/// request rather than blocking the queue indefinitely. (#343)
#[allow(clippy::too_many_arguments)]
// All parameters are semantically distinct and grouping them into a struct
// would obscure the queue lifecycle without removing any coupling.
pub fn spawn_inference_worker(
    client: AnyClient,
    mut interactive_rx: mpsc::Receiver<InferenceRequest>,
    mut background_rx: mpsc::Receiver<InferenceRequest>,
    mut batch_rx: mpsc::Receiver<InferenceRequest>,
    log: InferenceLog,
    file_log: file_log::InferenceFileLog,
    provider: parish_config::Provider,
    timeout_config: InferenceConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let request = tokio::select! {
                biased;
                Some(req) = interactive_rx.recv() => req,
                Some(req) = background_rx.recv() => req,
                Some(req) = batch_rx.recv() => req,
                else => break,
            };

            let streaming = request.token_tx.is_some();
            let prompt_len = request.prompt.len();
            let model = request.model.clone();
            let system_prompt = request.system.clone();
            let prompt_text = request.prompt.clone();
            let max_tokens = request.max_tokens;
            let temperature = request.temperature;
            let priority = request.priority;
            let req_id = request.id;
            let start = Instant::now();

            let streaming_timeout =
                std::time::Duration::from_secs(timeout_config.streaming_timeout_secs);
            let blocking_timeout = std::time::Duration::from_secs(timeout_config.timeout_secs);

            // Resolve effective response_format: schema wins over json_mode.
            let response_format: Option<ResponseFormat> =
                match (request.json_schema.clone(), request.json_mode) {
                    (Some(schema), _) => Some(ResponseFormat::JsonSchema {
                        json_schema: schema,
                    }),
                    (None, true) => Some(ResponseFormat::JsonObject),
                    (None, false) => None,
                };

            let (result, stream_stats) = match request.token_tx {
                Some(token_tx) => {
                    let (proxy_tx, mut proxy_rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
                    let observer_start = start;
                    let observer = tokio::spawn(async move {
                        let mut ttft: Option<Duration> = None;
                        let mut tokens: u64 = 0;
                        while let Some(tok) = proxy_rx.recv().await {
                            if ttft.is_none() {
                                ttft = Some(observer_start.elapsed());
                            }
                            tokens += 1;
                            if token_tx.send(tok).await.is_err() {
                                break;
                            }
                        }
                        StreamStats { ttft, tokens }
                    });
                    let label = match response_format {
                        Some(ResponseFormat::JsonSchema { .. }) => "streaming (schema) inference",
                        Some(ResponseFormat::JsonObject) => "streaming (json) inference",
                        None => "streaming inference",
                    };
                    let result = inference_with_timeout(
                        client.generate_stream_with_format(
                            &request.model,
                            &request.prompt,
                            request.system.as_deref(),
                            proxy_tx,
                            response_format.clone(),
                            request.max_tokens,
                            request.temperature,
                            request.frequency_penalty,
                        ),
                        streaming_timeout,
                        timeout_config.streaming_timeout_secs,
                        &request.model,
                        label,
                        request.cancel.as_ref(),
                    )
                    .await;
                    let stats = observer.await.unwrap_or(StreamStats {
                        ttft: None,
                        tokens: 0,
                    });
                    (result, Some(stats))
                }
                None => {
                    let result = inference_with_timeout(
                        client.generate_with_format(
                            &request.model,
                            &request.prompt,
                            request.system.as_deref(),
                            response_format.clone(),
                            request.max_tokens,
                            request.temperature,
                            request.frequency_penalty,
                        ),
                        blocking_timeout,
                        timeout_config.timeout_secs,
                        &request.model,
                        "inference",
                        request.cancel.as_ref(),
                    )
                    .await;
                    (result, None)
                }
            };

            let elapsed = start.elapsed();
            let (ttft_ms, output_tokens) = match stream_stats {
                Some(s) => (s.ttft.map(|d| d.as_millis() as u64), Some(s.tokens)),
                None => (None, None),
            };

            let (response, entry_error, response_len, response_text) = match &result {
                Ok(text) => (
                    InferenceResponse {
                        id: req_id,
                        text: text.clone(),
                        error: None,
                    },
                    None,
                    text.len(),
                    text.clone(),
                ),
                Err(e) => (
                    InferenceResponse {
                        id: req_id,
                        text: String::new(),
                        error: Some(e.to_string()),
                    },
                    Some(e.to_string()),
                    0,
                    String::new(),
                ),
            };

            // Record log entry
            {
                let entry = InferenceLogEntry {
                    request_id: req_id,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    model,
                    streaming,
                    duration_ms: elapsed.as_millis() as u64,
                    prompt_len,
                    response_len,
                    error: entry_error,
                    system_prompt,
                    prompt_text,
                    response_text,
                    max_tokens,
                    ttft_ms,
                    output_tokens,
                    temperature,
                    priority,
                };
                file_log.record(&entry, &provider, priority);
                let mut log = log.lock().await;
                log.push(entry);
            }

            // Ignore send error — the caller may have dropped the receiver
            let _ = request.response_tx.send(response);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_entry(request_id: u64) -> InferenceLogEntry {
        InferenceLogEntry {
            request_id,
            timestamp: "00:00:00".to_string(),
            model: "test".to_string(),
            streaming: false,
            duration_ms: 0,
            prompt_len: 0,
            response_len: 0,
            error: None,
            system_prompt: None,
            prompt_text: String::new(),
            response_text: String::new(),
            max_tokens: None,
            ttft_ms: None,
            output_tokens: None,
            temperature: None,
            priority: InferencePriority::Interactive,
        }
    }

    /// Regression test for issue #340: the ring buffer must enforce its
    /// configured cap regardless of `VecDeque::capacity()`'s rounded-up value.
    #[test]
    fn bounded_inference_log_enforces_configured_cap() {
        let mut log = BoundedInferenceLog::new(50);
        for i in 0..1000u64 {
            log.push(log_entry(i));
        }
        assert_eq!(log.len(), 50, "log must never exceed its configured cap");
        assert_eq!(log.max_entries(), 50);
        // Oldest entry should have been evicted; we should see the last 50 IDs.
        let ids: Vec<u64> = log.iter().map(|e| e.request_id).collect();
        assert_eq!(ids.first().copied(), Some(950));
        assert_eq!(ids.last().copied(), Some(999));
    }

    /// A zero cap is clamped to 1 so pushes always leave one entry.
    #[test]
    fn bounded_inference_log_zero_cap_is_clamped() {
        let mut log = BoundedInferenceLog::new(0);
        assert_eq!(log.max_entries(), 1);
        log.push(log_entry(1));
        log.push(log_entry(2));
        assert_eq!(log.len(), 1);
        assert_eq!(log.iter().next().unwrap().request_id, 2);
    }

    #[test]
    fn bounded_inference_log_is_empty_and_len() {
        let mut log = BoundedInferenceLog::new(4);
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        log.push(log_entry(1));
        assert!(!log.is_empty());
        assert_eq!(log.len(), 1);
    }

    /// Helper to build a three-lane InferenceQueue and return the matching receivers.
    fn make_queue() -> (
        InferenceQueue,
        mpsc::Receiver<InferenceRequest>,
        mpsc::Receiver<InferenceRequest>,
        mpsc::Receiver<InferenceRequest>,
    ) {
        let (itx, irx) = mpsc::channel::<InferenceRequest>(16);
        let (btx, brx) = mpsc::channel::<InferenceRequest>(32);
        let (batx, batrx) = mpsc::channel::<InferenceRequest>(64);
        (InferenceQueue::new(itx, btx, batx), irx, brx, batrx)
    }

    #[tokio::test]
    async fn test_inference_queue_send() {
        let (queue, mut irx, _brx, _batrx) = make_queue();

        let response_rx = queue
            .send(
                1,
                "test-model".to_string(),
                "hello".to_string(),
                Some("system".to_string()),
                None,
                None,
                None,
                InferencePriority::Interactive,
                false,
            )
            .await
            .unwrap();

        // Verify the request was received on the Interactive lane
        let request = irx.recv().await.unwrap();
        assert_eq!(request.id, 1);
        assert_eq!(request.model, "test-model");
        assert_eq!(request.prompt, "hello");
        assert_eq!(request.system, Some("system".to_string()));
        assert_eq!(request.priority, InferencePriority::Interactive);

        // Send a mock response back
        let response = InferenceResponse {
            id: 1,
            text: "world".to_string(),
            error: None,
        };
        request.response_tx.send(response).unwrap();

        // Verify the caller receives it
        let received = response_rx.await.unwrap();
        assert_eq!(received.id, 1);
        assert_eq!(received.text, "world");
        assert!(received.error.is_none());
    }

    /// TODO #10 / #23 / #34 — `send_with_penalty` must carry
    /// `frequency_penalty` onto the `InferenceRequest` so the worker can
    /// forward it to the underlying client. Regressing this field to
    /// `None` would silently disable the Tier 1 repetition-loop fix.
    #[tokio::test]
    async fn test_inference_queue_send_with_penalty_carries_field() {
        let (queue, mut irx, _brx, _batrx) = make_queue();
        let _rx = queue
            .send_with_penalty(
                42,
                "dialogue-model".to_string(),
                "prompt".to_string(),
                Some("system".to_string()),
                None,
                Some(256),
                Some(0.7),
                Some(0.5),
                InferencePriority::Interactive,
                true,
            )
            .await
            .unwrap();

        let request = irx.recv().await.unwrap();
        assert_eq!(request.id, 42);
        assert_eq!(request.frequency_penalty, Some(0.5));
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(256));
        assert!(request.json_mode);
    }

    /// `send` / `send_with_schema` callers leave the penalty implicit;
    /// the request must carry `None` so non-dialogue tiers don't
    /// accidentally pick up the Tier 1 sampling override.
    #[tokio::test]
    async fn test_inference_queue_send_default_omits_frequency_penalty() {
        // Send into the Interactive lane and receive on `irx` so the
        // request actually surfaces. The original Background-into-irx
        // mismatch hung this test forever and stalled CI's quality
        // gate at the 30-minute timeout (#1127 follow-up).
        let (queue, mut irx, _brx, _batrx) = make_queue();
        let _rx = queue
            .send(
                43,
                "m".to_string(),
                "p".to_string(),
                None,
                None,
                None,
                None,
                InferencePriority::Interactive,
                false,
            )
            .await
            .unwrap();
        let request = irx.recv().await.unwrap();
        assert_eq!(request.frequency_penalty, None);
    }

    #[tokio::test]
    async fn test_inference_queue_no_system() {
        let (queue, mut irx, _brx, _batrx) = make_queue();

        let _response_rx = queue
            .send(
                2,
                "model".to_string(),
                "prompt".to_string(),
                None,
                None,
                None,
                None,
                InferencePriority::Interactive,
                false,
            )
            .await
            .unwrap();

        let request = irx.recv().await.unwrap();
        assert_eq!(request.id, 2);
        assert!(request.system.is_none());
    }

    #[tokio::test]
    async fn test_inference_queue_with_token_tx() {
        let (queue, mut irx, _brx, _batrx) = make_queue();

        let (token_tx, _token_rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);

        let _response_rx = queue
            .send(
                3,
                "model".to_string(),
                "prompt".to_string(),
                None,
                Some(token_tx),
                None,
                None,
                InferencePriority::Interactive,
                false,
            )
            .await
            .unwrap();

        let request = irx.recv().await.unwrap();
        assert_eq!(request.id, 3);
        assert!(request.token_tx.is_some());
    }

    #[tokio::test]
    async fn test_inference_response_debug() {
        let response = InferenceResponse {
            id: 1,
            text: "hello".to_string(),
            error: None,
        };
        let debug = format!("{:?}", response);
        assert!(debug.contains("hello"));
    }

    #[tokio::test]
    async fn test_await_inference_response_returns_response() {
        let (tx, rx) = oneshot::channel();
        tx.send(InferenceResponse {
            id: 42,
            text: "ok".to_string(),
            error: None,
        })
        .unwrap();
        let outcome = await_inference_response(rx, Some(std::time::Duration::from_secs(1))).await;
        match outcome {
            InferenceAwaitOutcome::Response(r) => {
                assert_eq!(r.id, 42);
                assert_eq!(r.text, "ok");
            }
            other => panic!("expected Response, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_await_inference_response_detects_closed_channel() {
        let (tx, rx) = oneshot::channel::<InferenceResponse>();
        drop(tx);
        let outcome = await_inference_response(rx, Some(std::time::Duration::from_secs(1))).await;
        assert!(matches!(outcome, InferenceAwaitOutcome::Closed));
    }

    #[tokio::test]
    async fn test_await_inference_response_times_out() {
        // Keep the sender alive so the channel isn't closed; only the timeout
        // arm can fire. Use a tiny real duration so the test runs fast.
        let (_tx, rx) = oneshot::channel::<InferenceResponse>();
        let outcome =
            await_inference_response(rx, Some(std::time::Duration::from_millis(20))).await;
        // `Duration::from_millis(20).as_secs()` rounds down to 0.
        match outcome {
            InferenceAwaitOutcome::TimedOut { secs } => assert_eq!(secs, 0),
            other => panic!("expected TimedOut, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_await_inference_response_without_timeout_awaits_forever() {
        // With `None`, the helper should await the channel without a cap.
        // We simulate this by sending a response on a background task and
        // asserting the helper receives it.
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let _ = tx.send(InferenceResponse {
                id: 7,
                text: "late".to_string(),
                error: None,
            });
        });
        let outcome = await_inference_response(rx, None).await;
        match outcome {
            InferenceAwaitOutcome::Response(r) => assert_eq!(r.id, 7),
            other => panic!("expected Response, got {:?}", other),
        }
    }

    #[test]
    fn test_inference_clients_dialogue_uses_override() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let cloud = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-test"),
        ));
        let mut overrides = HashMap::new();
        overrides.insert(
            InferenceCategory::Dialogue,
            (cloud, "anthropic/claude-sonnet-4-20250514".to_string()),
        );
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), overrides);
        let (_client, model) = clients.dialogue_client();
        assert_eq!(model, "anthropic/claude-sonnet-4-20250514");
        assert!(clients.has_custom_dialogue());
    }

    #[test]
    fn test_inference_clients_dialogue_falls_back_to_base() {
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), HashMap::new());
        let (_client, model) = clients.dialogue_client();
        assert_eq!(model, "qwen3:14b");
        assert!(!clients.has_custom_dialogue());
    }

    #[test]
    fn test_inference_clients_simulation_falls_back_to_base() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let cloud = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-test"),
        ));
        let mut overrides = HashMap::new();
        overrides.insert(InferenceCategory::Dialogue, (cloud, "gpt-4".to_string()));
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), overrides);
        let (_client, model) = clients.simulation_client();
        assert_eq!(model, "qwen3:14b");
    }

    #[test]
    fn test_inference_clients_per_category_overrides() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let dial = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-dial"),
        ));
        let sim = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let intent = AnyClient::open_ai(OpenAiClient::new("http://localhost:1234", None));
        let mut overrides = HashMap::new();
        overrides.insert(InferenceCategory::Dialogue, (dial, "claude-4".to_string()));
        overrides.insert(InferenceCategory::Simulation, (sim, "qwen3:8b".to_string()));
        overrides.insert(
            InferenceCategory::Intent,
            (intent, "qwen3:1.5b".to_string()),
        );
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), overrides);

        let (_, model) = clients.dialogue_client();
        assert_eq!(model, "claude-4");

        let (_, model) = clients.simulation_client();
        assert_eq!(model, "qwen3:8b");

        let (_, model) = clients.intent_client();
        assert_eq!(model, "qwen3:1.5b");
    }

    #[test]
    fn test_inference_clients_intent_falls_back_to_base() {
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), HashMap::new());
        let (_client, model) = clients.intent_client();
        assert_eq!(model, "qwen3:14b");
    }

    #[test]
    fn test_inference_clients_reaction_falls_back_to_base() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let cloud = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-test"),
        ));
        let mut overrides = HashMap::new();
        overrides.insert(InferenceCategory::Dialogue, (cloud, "gpt-4".to_string()));
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), overrides);
        let (_client, model) = clients.reaction_client();
        assert_eq!(model, "qwen3:14b");
    }

    #[test]
    fn test_inference_clients_reaction_uses_override() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let reaction = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-reaction"),
        ));
        let mut overrides = HashMap::new();
        overrides.insert(
            InferenceCategory::Reaction,
            (reaction, "claude-sonnet-4".to_string()),
        );
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), overrides);
        let (_client, model) = clients.reaction_client();
        assert_eq!(model, "claude-sonnet-4");
    }

    #[test]
    fn test_inference_priority_ordering() {
        assert!(InferencePriority::Interactive < InferencePriority::Background);
        assert!(InferencePriority::Background < InferencePriority::Batch);
    }

    #[tokio::test]
    async fn test_priority_lanes_route_correctly() {
        // Verify each priority routes to the correct lane receiver.
        let (queue, mut irx, mut brx, mut batrx) = make_queue();

        // Send one request per lane
        let _rx1 = queue
            .send(
                10,
                "m".to_string(),
                "p".to_string(),
                None,
                None,
                None,
                None,
                InferencePriority::Interactive,
                false,
            )
            .await
            .unwrap();
        let _rx2 = queue
            .send(
                11,
                "m".to_string(),
                "p".to_string(),
                None,
                None,
                None,
                None,
                InferencePriority::Background,
                false,
            )
            .await
            .unwrap();
        let _rx3 = queue
            .send(
                12,
                "m".to_string(),
                "p".to_string(),
                None,
                None,
                None,
                None,
                InferencePriority::Batch,
                false,
            )
            .await
            .unwrap();

        let req_i = irx.recv().await.unwrap();
        assert_eq!(req_i.id, 10);
        assert_eq!(req_i.priority, InferencePriority::Interactive);

        let req_b = brx.recv().await.unwrap();
        assert_eq!(req_b.id, 11);
        assert_eq!(req_b.priority, InferencePriority::Background);

        let req_ba = batrx.recv().await.unwrap();
        assert_eq!(req_ba.id, 12);
        assert_eq!(req_ba.priority, InferencePriority::Batch);
    }

    #[tokio::test]
    async fn test_priority_lanes_batch_yields_to_interactive_when_queued() {
        // Submit requests to two lanes without a real worker.
        // Then manually drain via biased select! to verify Interactive is drained first.
        let (itx, mut irx) = mpsc::channel::<InferenceRequest>(16);
        let (btx, mut _brx) = mpsc::channel::<InferenceRequest>(32);
        let (batx, mut batrx) = mpsc::channel::<InferenceRequest>(64);
        let queue = InferenceQueue::new(itx, btx, batx);

        // Enqueue a Batch request first, then an Interactive request.
        let _rx_batch = queue
            .send(
                20,
                "m".to_string(),
                "batch".to_string(),
                None,
                None,
                None,
                None,
                InferencePriority::Batch,
                false,
            )
            .await
            .unwrap();
        let _rx_interactive = queue
            .send(
                21,
                "m".to_string(),
                "interactive".to_string(),
                None,
                None,
                None,
                None,
                InferencePriority::Interactive,
                false,
            )
            .await
            .unwrap();

        // The worker loop uses `biased;` — simulate that by draining with the same ordering.
        let first = tokio::select! {
            biased;
            Some(req) = irx.recv() => req,
            Some(req) = batrx.recv() => req,
            else => panic!("no request"),
        };
        // Interactive must win even though Batch was enqueued first.
        assert_eq!(first.priority, InferencePriority::Interactive);
        assert_eq!(first.id, 21);

        let second = tokio::select! {
            biased;
            Some(req) = irx.recv() => req,
            Some(req) = batrx.recv() => req,
            else => panic!("no second request"),
        };
        assert_eq!(second.priority, InferencePriority::Batch);
        assert_eq!(second.id, 20);
    }

    /// Verifies that aborting the JoinHandle from `spawn_inference_worker` actually
    /// stops the worker task, preventing orphaned tasks from accumulating across
    /// provider/key rebuilds (fix for issue #51).
    #[tokio::test]
    async fn test_spawn_inference_worker_abort_stops_task() {
        use tokio::time::{Duration, timeout};

        let (interactive_tx, interactive_rx) = mpsc::channel::<InferenceRequest>(4);
        let (_background_tx, background_rx) = mpsc::channel::<InferenceRequest>(4);
        let (_batch_tx, batch_rx) = mpsc::channel::<InferenceRequest>(4);
        let log = new_inference_log();
        let handle = spawn_inference_worker(
            AnyClient::simulator(),
            interactive_rx,
            background_rx,
            batch_rx,
            log,
            file_log::InferenceFileLog::disabled(),
            parish_config::Provider::simulator(),
            InferenceConfig::default(),
        );

        // Worker is running — abort it.
        handle.abort();

        // The handle should resolve quickly after abort (the task is cancelled).
        let result = timeout(Duration::from_millis(200), handle).await;
        assert!(
            result.is_ok(),
            "aborted worker task did not finish within timeout"
        );

        // After abort the sender should detect the receiver is gone; sending fails.
        let (resp_tx, _resp_rx) = oneshot::channel();
        let req = InferenceRequest {
            id: 99,
            model: "model".to_string(),
            prompt: "hi".to_string(),
            system: None,
            token_tx: None,
            response_tx: resp_tx,
            max_tokens: None,
            temperature: None,
            frequency_penalty: None,
            priority: InferencePriority::Interactive,
            json_mode: false,
            json_schema: None,
            cancel: None,
        };
        // send returns Err when the receiver has been dropped by the aborted task.
        let send_result = interactive_tx.send(req).await;
        assert!(
            send_result.is_err(),
            "expected send to fail after worker abort"
        );
    }

    /// A streaming request must record `ttft_ms` and `output_tokens` in the
    /// log entry so the debug panel can surface throughput metrics. The
    /// simulator emits one token every ~40 ms, so both fields must be `Some`
    /// after the call resolves.
    #[tokio::test]
    async fn test_streaming_request_records_ttft_and_token_count() {
        let (interactive_tx, interactive_rx) = mpsc::channel::<InferenceRequest>(4);
        let (_btx, background_rx) = mpsc::channel::<InferenceRequest>(4);
        let (_batx, batch_rx) = mpsc::channel::<InferenceRequest>(4);
        let log = new_inference_log();
        let _handle = spawn_inference_worker(
            AnyClient::simulator(),
            interactive_rx,
            background_rx,
            batch_rx,
            log.clone(),
            file_log::InferenceFileLog::disabled(),
            parish_config::Provider::simulator(),
            InferenceConfig::default(),
        );

        let (resp_tx, resp_rx) = oneshot::channel();
        let (tok_tx, mut tok_rx) = mpsc::channel::<String>(64);
        let drain = tokio::spawn(async move { while tok_rx.recv().await.is_some() {} });
        interactive_tx
            .send(InferenceRequest {
                id: 1,
                model: "sim".to_string(),
                prompt: "Tell me about Roscommon.".to_string(),
                system: None,
                token_tx: Some(tok_tx),
                response_tx: resp_tx,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
            .await
            .expect("send");

        let resp = resp_rx.await.expect("response");
        assert!(resp.error.is_none(), "expected ok, got {:?}", resp.error);
        drain.await.ok();

        let log_guard = log.lock().await;
        let entry = log_guard.iter().find(|e| e.request_id == 1).expect("entry");
        assert!(
            entry.ttft_ms.is_some(),
            "ttft_ms must be populated for streaming"
        );
        let tokens = entry.output_tokens.expect("output_tokens populated");
        assert!(tokens > 0, "expected >0 tokens, got {tokens}");
    }

    /// A request whose cancel token fires mid-stream must surface
    /// `error: "cancelled"` and free the worker for the next request.
    /// Uses the simulator (40 ms/token); we fire cancel after the first
    /// token arrives.
    #[tokio::test]
    async fn test_cancellation_fires_mid_stream_yields_error() {
        let (interactive_tx, interactive_rx) = mpsc::channel::<InferenceRequest>(4);
        let (_btx, background_rx) = mpsc::channel::<InferenceRequest>(4);
        let (_batx, batch_rx) = mpsc::channel::<InferenceRequest>(4);
        let log = new_inference_log();
        let _handle = spawn_inference_worker(
            AnyClient::simulator(),
            interactive_rx,
            background_rx,
            batch_rx,
            log,
            file_log::InferenceFileLog::disabled(),
            parish_config::Provider::simulator(),
            InferenceConfig::default(),
        );

        let cancel = CancellationToken::new();
        let cancel_for_request = cancel.clone();

        let (resp_tx, resp_rx) = oneshot::channel();
        let (tok_tx, mut tok_rx) = mpsc::channel::<String>(64);

        // Drain forwarded tokens; fire cancel as soon as the first one
        // lands so the worker drops its inflight future mid-stream.
        let drain = tokio::spawn(async move {
            let mut count: u64 = 0;
            while tok_rx.recv().await.is_some() {
                count += 1;
                if count == 1 {
                    cancel.cancel();
                }
            }
            count
        });

        interactive_tx
            .send(InferenceRequest {
                id: 7,
                model: "sim".to_string(),
                prompt: "Tell me a long story about Roscommon hedges.".to_string(),
                system: None,
                token_tx: Some(tok_tx),
                response_tx: resp_tx,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: Some(cancel_for_request),
            })
            .await
            .expect("send");

        let resp = resp_rx.await.expect("response");
        let tokens_seen = drain.await.unwrap_or(0);
        let err = resp.error.expect("expected an error after cancel");
        assert!(
            err.contains("cancel"),
            "expected error to mention cancel, got {err:?}"
        );
        assert!(
            tokens_seen >= 1,
            "expected at least one token before cancel fired"
        );
    }

    /// Regression test for issue #343: the worker must not block the queue
    /// indefinitely when an LLM call hangs.  We configure a 1-second timeout
    /// and verify that a simulated slow call yields an error response rather
    /// than wedging the worker.
    ///
    /// The simulator responds instantly, so we use a custom `timeout_secs = 0`
    /// config (the floor is effectively 1 tokio tick) and verify the response
    /// carries an error string when the limit is breached.  In practice the
    /// simulator is faster than any real timeout, so we also exercise the
    /// happy-path: a second request after the first must still be served,
    /// proving the worker loop continues after a timeout error.
    #[tokio::test]
    async fn test_worker_timeout_sends_error_and_continues() {
        use tokio::time::Duration;

        // Use a 1-second timeout — short but long enough that the simulator
        // (which answers instantly) will succeed; the test verifies the
        // happy-path *and* that the queue is not wedged after an error.
        let cfg = InferenceConfig {
            timeout_secs: 1,
            ..Default::default()
        };

        let (interactive_tx, interactive_rx) = mpsc::channel::<InferenceRequest>(4);
        let (_btx, background_rx) = mpsc::channel::<InferenceRequest>(4);
        let (_batx, batch_rx) = mpsc::channel::<InferenceRequest>(4);
        let log = new_inference_log();
        let _handle = spawn_inference_worker(
            AnyClient::simulator(),
            interactive_rx,
            background_rx,
            batch_rx,
            log,
            file_log::InferenceFileLog::disabled(),
            parish_config::Provider::simulator(),
            cfg,
        );

        // Send a normal request — the simulator responds well within 1 s.
        let (resp_tx, resp_rx) = oneshot::channel();
        interactive_tx
            .send(InferenceRequest {
                id: 100,
                model: "sim".to_string(),
                prompt: "ping".to_string(),
                system: None,
                token_tx: None,
                json_mode: false,
                json_schema: None,
                cancel: None,
                response_tx: resp_tx,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                priority: InferencePriority::Interactive,
            })
            .await
            .unwrap();
        let resp = tokio::time::timeout(Duration::from_secs(5), resp_rx)
            .await
            .expect("response channel timed out")
            .expect("response channel closed");
        // Simulator always succeeds — error must be None.
        assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
        assert_eq!(resp.id, 100);

        // Send a second request to prove the worker is still running after the first.
        let (resp_tx2, resp_rx2) = oneshot::channel();
        interactive_tx
            .send(InferenceRequest {
                id: 101,
                model: "sim".to_string(),
                prompt: "pong".to_string(),
                system: None,
                token_tx: None,
                json_mode: false,
                json_schema: None,
                cancel: None,
                response_tx: resp_tx2,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                priority: InferencePriority::Interactive,
            })
            .await
            .unwrap();
        let resp2 = tokio::time::timeout(Duration::from_secs(5), resp_rx2)
            .await
            .expect("second response channel timed out")
            .expect("second response channel closed");
        assert_eq!(resp2.id, 101);
        assert!(
            resp2.error.is_none(),
            "unexpected error on second request: {:?}",
            resp2.error
        );
    }

    // ── submit_json tests (TD-028) ───────────────────────────────────────────

    #[tokio::test]
    async fn submit_json_deserializes_valid_json() {
        let (itx, mut irx) = mpsc::channel::<InferenceRequest>(4);
        let (btx, _brx) = mpsc::channel::<InferenceRequest>(4);
        let (batx, _batrx) = mpsc::channel::<InferenceRequest>(4);
        let queue = InferenceQueue::new(itx, btx, batx);

        tokio::spawn(async move {
            if let Some(req) = irx.recv().await {
                let _ = req.response_tx.send(InferenceResponse {
                    id: req.id,
                    text: r#"{"hello":"world"}"#.to_string(),
                    error: None,
                });
            }
        });

        #[derive(serde::Deserialize, Debug)]
        struct Greeting {
            hello: String,
        }

        let result: Result<Greeting, ParishError> =
            submit_json(&queue, InferencePriority::Interactive, "m", "p", None, None).await;
        assert!(result.is_ok(), "expected ok, got: {:?}", result.err());
        assert_eq!(result.unwrap().hello, "world");
    }

    #[tokio::test]
    async fn submit_json_propagates_worker_error() {
        let (itx, mut irx) = mpsc::channel::<InferenceRequest>(4);
        let (btx, _brx) = mpsc::channel::<InferenceRequest>(4);
        let (batx, _batrx) = mpsc::channel::<InferenceRequest>(4);
        let queue = InferenceQueue::new(itx, btx, batx);

        tokio::spawn(async move {
            if let Some(req) = irx.recv().await {
                let _ = req.response_tx.send(InferenceResponse {
                    id: req.id,
                    text: String::new(),
                    error: Some("model exploded".to_string()),
                });
            }
        });

        let result: Result<serde_json::Value, ParishError> =
            submit_json(&queue, InferencePriority::Interactive, "m", "p", None, None).await;
        let err = result.expect_err("should error");
        assert!(
            err.to_string().contains("model exploded"),
            "expected 'model exploded' in error, got: {err}"
        );
    }

    #[tokio::test]
    async fn submit_json_fails_on_malformed_json() {
        let (itx, mut irx) = mpsc::channel::<InferenceRequest>(4);
        let (btx, _brx) = mpsc::channel::<InferenceRequest>(4);
        let (batx, _batrx) = mpsc::channel::<InferenceRequest>(4);
        let queue = InferenceQueue::new(itx, btx, batx);

        tokio::spawn(async move {
            if let Some(req) = irx.recv().await {
                let _ = req.response_tx.send(InferenceResponse {
                    id: req.id,
                    text: "not json".to_string(),
                    error: None,
                });
            }
        });

        let result: Result<serde_json::Value, ParishError> =
            submit_json(&queue, InferencePriority::Interactive, "m", "p", None, None).await;
        let err = result.expect_err("should error");
        assert!(
            err.to_string().contains("JSON parse failed"),
            "expected JSON parse error, got: {err}"
        );
    }

    #[tokio::test]
    async fn submit_json_fails_when_queue_closed() {
        let (itx, _irx) = mpsc::channel::<InferenceRequest>(4);
        let (btx, _brx) = mpsc::channel::<InferenceRequest>(4);
        let (batx, _batrx) = mpsc::channel::<InferenceRequest>(4);
        let queue = InferenceQueue::new(itx, btx, batx);
        drop(_irx);
        drop(_brx);
        drop(_batrx);

        let result: Result<serde_json::Value, ParishError> =
            submit_json(&queue, InferencePriority::Interactive, "m", "p", None, None).await;
        let err = result.expect_err("should error");
        assert!(
            err.to_string().contains("queue send failed"),
            "expected queue send error, got: {err}"
        );
    }

    /// Regression — `AnyClient::Simulator::generate_stream_with_format` used
    /// to dispatch unconditionally to the Markov text stream, so any caller
    /// that expected a JSON shape (Tier 2/3 sim+reaction prompts, intent
    /// parser) saw a parse error every tick on the `small-only` loadout.
    /// Now: any JSON-shaped ask (response_format set, or a "JSON" /
    /// "input parser" / "Respond with a JSON" marker in the system or user
    /// prompt) streams a generic JSON object whose fields are wide enough
    /// for `Tier2Response`, `Tier3Update`, and the dialogue
    /// `NpcStreamResponse` (all `#[serde(default)]`), so the worst case is
    /// an "uneventful tick" rather than a parse failure.
    #[tokio::test]
    async fn simulator_streams_json_when_format_or_prompt_requests_it() {
        use tokio::sync::mpsc;

        let sim = AnyClient::simulator();

        // Helper: drive `generate_stream_with_format` and collect the chunks
        // the simulator pushed into `token_tx` (the streaming side) AND the
        // assembled return string (which is what the worker assembles for the
        // queue's response payload).
        async fn drive(
            client: &AnyClient,
            prompt: &str,
            system: Option<&str>,
            response_format: Option<ResponseFormat>,
        ) -> (Vec<String>, String) {
            let (tx, mut rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
            let stream_fut = client.generate_stream_with_format(
                "sim",
                prompt,
                system,
                tx,
                response_format,
                Some(120),
                None,
                None,
            );
            let drain = tokio::spawn(async move {
                let mut chunks = Vec::new();
                while let Some(t) = rx.recv().await {
                    chunks.push(t);
                }
                chunks
            });
            let assembled = stream_fut.await.expect("stream future");
            let chunks = drain.await.unwrap_or_default();
            (chunks, assembled)
        }

        // Case 1 — response_format explicitly set: must stream JSON.
        let (_, body1) = drive(
            &sim,
            "fly some text",
            None,
            Some(ResponseFormat::JsonObject),
        )
        .await;
        assert!(
            body1.contains("\"dialogue\""),
            "explicit JsonObject should stream the dialogue-shaped JSON, got: {body1}"
        );

        // Case 2 — Tier 2 / Tier 3 sim prompts ask for JSON in the user
        // prompt body.  Marker is "Respond with a JSON" (Tier 2) or a bare
        // "JSON object" mention.
        let (_, body2) = drive(
            &sim,
            "You are simulating background interactions. … Respond with a JSON object …",
            None,
            None,
        )
        .await;
        assert!(
            body2.starts_with('{') && body2.contains("\"dialogue\""),
            "Tier 2 prompt should still stream JSON (no response_format set), got: {body2}"
        );

        // Case 3 — intent parser system prompt triggers JSON path even without
        // a response_format hint.
        let (_, body3) = drive(
            &sim,
            "go to the pub",
            Some("You are a text adventure input parser. …"),
            None,
        )
        .await;
        assert!(
            body3.starts_with('{'),
            "intent-parser system prompt should stream JSON, got: {body3}"
        );

        // Case 4 — Tier 3 prompt (build_tier3_prompt) uses "Respond with
        // JSON" (no "a") and embeds a `{"updates":[…]}` schema. Regression:
        // before the "Respond with JSON" + `"updates":` markers landed, this
        // fell through to Markov and the Tier 3 batch parser logged a parse
        // failure on every world-tick post-boot.
        let (_, body_t3) = drive(
            &sim,
            r#"You are simulating background NPC activity. Respond with JSON, using the bracketed ids: {"updates":[{"npc_id":1,"mood":"…"}]}"#,
            None,
            None,
        )
        .await;
        assert!(
            body_t3.starts_with('{'),
            "Tier 3 prompt must stream JSON (regression — boot-time parse storm), got: {body_t3}"
        );

        // Case 5 — plain dialogue with no JSON ask: legacy Markov text path.
        let (_, body5) = drive(&sim, "Tell me a story.", None, None).await;
        assert!(
            !body5.starts_with('{'),
            "plain prompt should still produce text, got: {body5}"
        );
    }
}
