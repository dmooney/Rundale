//! LLM inference pipeline for OpenAI-compatible providers.
//!
//! Manages a request queue (Tokio mpsc channel), routes requests
//! to the configured LLM provider (Ollama, LM Studio, OpenRouter, etc.),
//! and returns responses via oneshot channels.

pub mod client;
pub mod openai_client;
pub mod rate_limit;
pub mod setup;
pub mod simulator;
pub(crate) mod utf8_stream;

pub use rate_limit::InferenceRateLimiter;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use openai_client::OpenAiClient;
use simulator::SimulatorClient;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

use parish_config::InferenceConfig;
use parish_types::ParishError;

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
}

/// Priority lane for inference requests. Higher priority lanes are drained first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InferencePriority {
    /// Player-facing dialogue (Tier 1). Highest priority.
    Interactive = 0,
    /// NPC background simulation (Tier 2). Medium priority.
    Background = 1,
    /// Distant NPC batch simulation (Tier 3). Lowest priority.
    Batch = 2,
}

/// Shared ring buffer of inference call log entries.
pub type InferenceLog = Arc<Mutex<VecDeque<InferenceLogEntry>>>;

/// Creates a new empty inference log with pre-allocated capacity from config.
pub fn new_inference_log_with_config(config: &InferenceConfig) -> InferenceLog {
    Arc::new(Mutex::new(VecDeque::with_capacity(config.log_capacity)))
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
    /// The Ollama model to use (e.g. "qwen3:14b").
    pub model: String,
    /// The prompt text to send to the model.
    pub prompt: String,
    /// Optional system prompt for context.
    pub system: Option<String>,
    /// Channel to send the response back to the caller.
    pub response_tx: oneshot::Sender<InferenceResponse>,
    /// Optional channel for streaming tokens. If present, the worker streams
    /// individual tokens through this before sending the final response.
    pub token_tx: Option<mpsc::UnboundedSender<String>>,
    /// Optional maximum number of tokens to generate.
    pub max_tokens: Option<u32>,
    /// Optional temperature for sampling (0.0 = deterministic, 1.0+ = creative).
    pub temperature: Option<f32>,
    /// Priority lane for this request.
    pub priority: InferencePriority,
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
        token_tx: Option<mpsc::UnboundedSender<String>>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        priority: InferencePriority,
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
            priority,
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

/// Monotonically increasing request ID counter for queue-submitted JSON requests.
static QUEUE_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Submit a request that expects a JSON response, then deserialize it.
///
/// Used by Tier 3 batch inference and (in future Track B) Tier 2 background simulation.
/// Requests are non-streaming and routed to the given priority lane.
pub async fn submit_json<T: serde::de::DeserializeOwned>(
    queue: &InferenceQueue,
    priority: InferencePriority,
    model: &str,
    prompt: &str,
    system: Option<&str>,
) -> Result<T, ParishError> {
    let id = QUEUE_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let response_rx = queue
        .send(
            id,
            model.to_string(),
            prompt.to_string(),
            system.map(String::from),
            None,
            None,
            None,
            priority,
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
    overrides: std::collections::HashMap<parish_config::InferenceCategory, (OpenAiClient, String)>,
    /// Base client used when no per-category override exists.
    pub base: OpenAiClient,
    /// Base model name (e.g. "qwen3:14b").
    pub base_model: String,
}

impl InferenceClients {
    /// Creates a new `InferenceClients` with the given base client and per-category overrides.
    pub fn new(
        base: OpenAiClient,
        base_model: String,
        overrides: std::collections::HashMap<
            parish_config::InferenceCategory,
            (OpenAiClient, String),
        >,
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
    pub fn client_for(&self, category: parish_config::InferenceCategory) -> (&OpenAiClient, &str) {
        match self.overrides.get(&category) {
            Some((client, model)) => (client, model),
            None => (&self.base, &self.base_model),
        }
    }

    /// Returns the client and model to use for player dialogue (Tier 1).
    pub fn dialogue_client(&self) -> (&OpenAiClient, &str) {
        self.client_for(parish_config::InferenceCategory::Dialogue)
    }

    /// Returns the client and model to use for background NPC simulation (Tier 2).
    pub fn simulation_client(&self) -> (&OpenAiClient, &str) {
        self.client_for(parish_config::InferenceCategory::Simulation)
    }

    /// Returns the client and model to use for intent parsing.
    pub fn intent_client(&self) -> (&OpenAiClient, &str) {
        self.client_for(parish_config::InferenceCategory::Intent)
    }

    /// Returns the client and model to use for NPC arrival reactions.
    pub fn reaction_client(&self) -> (&OpenAiClient, &str) {
        self.client_for(parish_config::InferenceCategory::Reaction)
    }

    /// Whether the dialogue category uses a different provider than the base.
    pub fn has_custom_dialogue(&self) -> bool {
        self.overrides
            .contains_key(&parish_config::InferenceCategory::Dialogue)
    }
}

/// A unified client handle that can be either a real OpenAI-compatible
/// HTTP client or the built-in offline simulator.
///
/// Use `AnyClient::OpenAi` for all real providers (Ollama, OpenRouter, etc.)
/// and `AnyClient::Simulator` to run without any LLM for testing.
#[derive(Clone)]
pub enum AnyClient {
    /// A real OpenAI-compatible HTTP client.
    OpenAi(OpenAiClient),
    /// The built-in offline simulator (generates funny nonsense locally).
    Simulator(Arc<SimulatorClient>),
}

impl AnyClient {
    /// Wraps a real `OpenAiClient`.
    pub fn open_ai(client: OpenAiClient) -> Self {
        Self::OpenAi(client)
    }

    /// Creates a new simulator client.
    pub fn simulator() -> Self {
        Self::Simulator(Arc::new(SimulatorClient::new()))
    }

    /// Generates plain text (non-streaming).
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate(model, prompt, system, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                c.generate(model, prompt, system, max_tokens, temperature)
                    .await
            }
        }
    }

    /// Generates text with token streaming.
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::UnboundedSender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_stream(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                c.generate_stream(model, prompt, system, token_tx, max_tokens, temperature)
                    .await
            }
        }
    }

    /// Generates a structured JSON response and deserializes it into `T`.
    pub async fn generate_json<T: serde::de::DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<T, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_json::<T>(model, prompt, system, max_tokens, temperature)
                    .await
            }
            Self::Simulator(c) => {
                c.generate_json::<T>(model, prompt, system, max_tokens, temperature)
                    .await
            }
        }
    }

    /// Returns a reference to the inner `OpenAiClient`, if this is a real client.
    pub fn as_open_ai(&self) -> Option<&OpenAiClient> {
        match self {
            Self::OpenAi(c) => Some(c),
            Self::Simulator(_) => None,
        }
    }

    /// Returns `true` if this is the offline simulator.
    pub fn is_simulator(&self) -> bool {
        matches!(self, Self::Simulator(_))
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
pub fn spawn_inference_worker(
    client: AnyClient,
    mut interactive_rx: mpsc::Receiver<InferenceRequest>,
    mut background_rx: mpsc::Receiver<InferenceRequest>,
    mut batch_rx: mpsc::Receiver<InferenceRequest>,
    log: InferenceLog,
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
            let req_id = request.id;
            let start = Instant::now();

            let result = if let Some(token_tx) = request.token_tx {
                client
                    .generate_stream(
                        &request.model,
                        &request.prompt,
                        request.system.as_deref(),
                        token_tx,
                        request.max_tokens,
                        request.temperature,
                    )
                    .await
            } else {
                client
                    .generate(
                        &request.model,
                        &request.prompt,
                        request.system.as_deref(),
                        request.max_tokens,
                        request.temperature,
                    )
                    .await
            };

            let elapsed = start.elapsed();

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
                };
                let mut log = log.lock().await;
                if log.len() >= log.capacity().max(1) {
                    log.pop_front();
                }
                log.push_back(entry);
            }

            // Ignore send error — the caller may have dropped the receiver
            let _ = request.response_tx.send(response);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let (token_tx, _token_rx) = mpsc::unbounded_channel::<String>();

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

    #[test]
    fn test_inference_clients_dialogue_uses_override() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = OpenAiClient::new("http://localhost:11434", None);
        let cloud = OpenAiClient::new("https://openrouter.ai/api", Some("sk-test"));
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

        let base = OpenAiClient::new("http://localhost:11434", None);
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), HashMap::new());
        let (_client, model) = clients.dialogue_client();
        assert_eq!(model, "qwen3:14b");
        assert!(!clients.has_custom_dialogue());
    }

    #[test]
    fn test_inference_clients_simulation_falls_back_to_base() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = OpenAiClient::new("http://localhost:11434", None);
        let cloud = OpenAiClient::new("https://openrouter.ai/api", Some("sk-test"));
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

        let base = OpenAiClient::new("http://localhost:11434", None);
        let dial = OpenAiClient::new("https://openrouter.ai/api", Some("sk-dial"));
        let sim = OpenAiClient::new("http://localhost:11434", None);
        let intent = OpenAiClient::new("http://localhost:1234", None);
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

        let base = OpenAiClient::new("http://localhost:11434", None);
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), HashMap::new());
        let (_client, model) = clients.intent_client();
        assert_eq!(model, "qwen3:14b");
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
            priority: InferencePriority::Interactive,
        };
        // send returns Err when the receiver has been dropped by the aborted task.
        let send_result = interactive_tx.send(req).await;
        assert!(
            send_result.is_err(),
            "expected send to fail after worker abort"
        );
    }
}
