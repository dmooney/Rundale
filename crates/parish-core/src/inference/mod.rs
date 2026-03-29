//! LLM inference pipeline for OpenAI-compatible providers.
//!
//! Manages a request queue (Tokio mpsc channel), routes requests
//! to the configured LLM provider (Ollama, LM Studio, OpenRouter, etc.),
//! and returns responses via oneshot channels.

pub mod client;
pub mod openai_client;
pub mod setup;

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use openai_client::OpenAiClient;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::debug_snapshot::InferenceLogEntry;

/// Maximum number of inference log entries to retain.
pub const INFERENCE_LOG_CAPACITY: usize = 50;

/// Shared ring buffer of inference call log entries.
pub type InferenceLog = Arc<Mutex<VecDeque<InferenceLogEntry>>>;

/// Creates a new empty inference log with pre-allocated capacity.
pub fn new_inference_log() -> InferenceLog {
    Arc::new(Mutex::new(VecDeque::with_capacity(INFERENCE_LOG_CAPACITY)))
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
/// Wraps a Tokio mpsc sender. Clone this to share across tasks.
#[derive(Clone)]
pub struct InferenceQueue {
    tx: mpsc::Sender<InferenceRequest>,
}

impl InferenceQueue {
    /// Creates a new inference queue with the given channel sender.
    pub fn new(tx: mpsc::Sender<InferenceRequest>) -> Self {
        Self { tx }
    }

    /// Submits an inference request to the queue.
    ///
    /// If `token_tx` is provided, the worker will stream individual tokens
    /// through it before sending the final complete response. An optional
    /// `max_tokens` cap is forwarded to the LLM provider to limit output
    /// length. Returns a oneshot receiver that will yield the complete
    /// response. Returns an error if the queue channel is closed.
    pub async fn send(
        &self,
        id: u64,
        model: String,
        prompt: String,
        system: Option<String>,
        token_tx: Option<mpsc::UnboundedSender<String>>,
        max_tokens: Option<u32>,
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
        };
        self.tx.send(request).await?;
        Ok(response_rx)
    }
}

/// Holds both local and cloud LLM clients for request routing.
///
/// The local client is used for Tier 2 background NPC simulation and intent parsing.
/// The cloud client (if configured) is used for Tier 1 player-facing dialogue.
/// Falls back to local if no cloud client is configured.
#[derive(Clone)]
pub struct InferenceClients {
    /// Local client (Ollama/LM Studio) for background tasks and intent parsing.
    pub local: OpenAiClient,
    /// Local model name (e.g. "qwen3:14b").
    pub local_model: String,
    /// Cloud client for player dialogue (None = use local for everything).
    pub cloud: Option<OpenAiClient>,
    /// Cloud model name (e.g. "anthropic/claude-sonnet-4-20250514").
    pub cloud_model: Option<String>,
}

impl InferenceClients {
    /// Returns the client and model to use for player dialogue (Tier 1).
    ///
    /// Prefers cloud if configured, falls back to local.
    pub fn dialogue_client(&self) -> (&OpenAiClient, &str) {
        match (&self.cloud, &self.cloud_model) {
            (Some(client), Some(model)) => (client, model),
            _ => (&self.local, &self.local_model),
        }
    }

    /// Returns the client and model to use for background NPC simulation (Tier 2).
    ///
    /// Always uses the local client.
    pub fn simulation_client(&self) -> (&OpenAiClient, &str) {
        (&self.local, &self.local_model)
    }

    /// Returns the client and model to use for intent parsing.
    ///
    /// Always uses the local client for low-latency structured output.
    pub fn intent_client(&self) -> (&OpenAiClient, &str) {
        (&self.local, &self.local_model)
    }

    /// Whether a cloud provider is configured for dialogue.
    pub fn has_cloud(&self) -> bool {
        self.cloud.is_some() && self.cloud_model.is_some()
    }
}

/// Spawns the inference worker task.
///
/// The worker pulls requests from the mpsc receiver, calls the LLM
/// client, and sends responses back through each request's oneshot channel.
/// Each completed call is recorded in the shared `log` ring buffer.
/// The task runs until the sender side of the channel is dropped.
pub fn spawn_inference_worker(
    client: OpenAiClient,
    mut rx: mpsc::Receiver<InferenceRequest>,
    log: InferenceLog,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
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
                    )
                    .await
            } else {
                client
                    .generate(
                        &request.model,
                        &request.prompt,
                        request.system.as_deref(),
                        request.max_tokens,
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
                if log.len() >= INFERENCE_LOG_CAPACITY {
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

    #[tokio::test]
    async fn test_inference_queue_send() {
        let (tx, mut rx) = mpsc::channel::<InferenceRequest>(10);
        let queue = InferenceQueue::new(tx);

        let response_rx = queue
            .send(
                1,
                "test-model".to_string(),
                "hello".to_string(),
                Some("system".to_string()),
                None,
                None,
            )
            .await
            .unwrap();

        // Verify the request was received
        let request = rx.recv().await.unwrap();
        assert_eq!(request.id, 1);
        assert_eq!(request.model, "test-model");
        assert_eq!(request.prompt, "hello");
        assert_eq!(request.system, Some("system".to_string()));

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
        let (tx, mut rx) = mpsc::channel::<InferenceRequest>(10);
        let queue = InferenceQueue::new(tx);

        let _response_rx = queue
            .send(
                2,
                "model".to_string(),
                "prompt".to_string(),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let request = rx.recv().await.unwrap();
        assert_eq!(request.id, 2);
        assert!(request.system.is_none());
    }

    #[tokio::test]
    async fn test_inference_queue_with_token_tx() {
        let (tx, mut rx) = mpsc::channel::<InferenceRequest>(10);
        let queue = InferenceQueue::new(tx);

        let (token_tx, _token_rx) = mpsc::unbounded_channel::<String>();

        let _response_rx = queue
            .send(
                3,
                "model".to_string(),
                "prompt".to_string(),
                None,
                Some(token_tx),
                None,
            )
            .await
            .unwrap();

        let request = rx.recv().await.unwrap();
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
    fn test_inference_clients_dialogue_uses_cloud() {
        let local = OpenAiClient::new("http://localhost:11434", None);
        let cloud = OpenAiClient::new("https://openrouter.ai/api", Some("sk-test"));
        let clients = InferenceClients {
            local,
            local_model: "qwen3:14b".to_string(),
            cloud: Some(cloud),
            cloud_model: Some("anthropic/claude-sonnet-4-20250514".to_string()),
        };
        let (_client, model) = clients.dialogue_client();
        assert_eq!(model, "anthropic/claude-sonnet-4-20250514");
        assert!(clients.has_cloud());
    }

    #[test]
    fn test_inference_clients_dialogue_falls_back_to_local() {
        let local = OpenAiClient::new("http://localhost:11434", None);
        let clients = InferenceClients {
            local,
            local_model: "qwen3:14b".to_string(),
            cloud: None,
            cloud_model: None,
        };
        let (_client, model) = clients.dialogue_client();
        assert_eq!(model, "qwen3:14b");
        assert!(!clients.has_cloud());
    }

    #[test]
    fn test_inference_clients_simulation_always_local() {
        let local = OpenAiClient::new("http://localhost:11434", None);
        let cloud = OpenAiClient::new("https://openrouter.ai/api", Some("sk-test"));
        let clients = InferenceClients {
            local,
            local_model: "qwen3:14b".to_string(),
            cloud: Some(cloud),
            cloud_model: Some("gpt-4".to_string()),
        };
        let (_client, model) = clients.simulation_client();
        assert_eq!(model, "qwen3:14b");
    }

    #[test]
    fn test_inference_clients_intent_always_local() {
        let local = OpenAiClient::new("http://localhost:11434", None);
        let cloud = OpenAiClient::new("https://openrouter.ai/api", Some("sk-test"));
        let clients = InferenceClients {
            local,
            local_model: "qwen3:14b".to_string(),
            cloud: Some(cloud),
            cloud_model: Some("gpt-4".to_string()),
        };
        let (_client, model) = clients.intent_client();
        assert_eq!(model, "qwen3:14b");
    }
}
