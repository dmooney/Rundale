//! WebGPU inference client — delegates LLM calls to the browser via a channel bridge.
//!
//! When the user selects `/provider webgpu` in the web client, all inference
//! requests are forwarded to the connected browser instead of hitting an HTTP
//! endpoint. The browser runs the model locally using WebLLM (WebGPU API) and
//! streams tokens back over the WebSocket.
//!
//! # Architecture
//!
//! ```text
//! InferenceWorker
//!   → WebGpuClient::generate_stream()
//!     → sends WebGpuRequest on request_tx
//!       → WebGpuBridge task (parish-server) receives it
//!         → emits "inference-request" event over WebSocket to browser
//!           → browser runs WebLLM, streams tokens back as WS messages
//!             → ws.rs routes tokens to WebGpuPending in AppState
//!               → WebGpuClient::generate_stream() receives tokens / done signal
//! ```
//!
//! The `WebGpuClient` itself has no dependency on the event bus or WebSocket —
//! it only talks through `mpsc`/`oneshot` channels. The `parish-server` crate
//! owns the bridge between those channels and the WebSocket transport.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;
use tokio::sync::{mpsc, oneshot};

use parish_types::ParishError;

// ── Wire types ────────────────────────────────────────────────────────────────

// Global monotonic ID counter shared across all WebGpuClient instances.
// Using a global (rather than per-instance) counter ensures request IDs are
// never reused across rebuild_inference calls, so stale timeout cleanup tasks
// from old bridge generations cannot accidentally remove new requests that
// happened to receive the same ID.
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// An outbound inference request forwarded from [`WebGpuClient`] to the bridge task.
pub struct WebGpuRequest {
    /// Unique request identifier used to correlate browser responses.
    pub id: u64,
    /// Model ID to use for inference (e.g. `"gemma-3-1b-it-q4f32_1-MLC"`).
    pub model: String,
    /// User prompt text.
    pub prompt: String,
    /// Optional system prompt.
    pub system: Option<String>,
    /// Token limit sent to the model.
    pub max_tokens: Option<u32>,
    /// Sampling temperature.
    pub temperature: Option<f32>,
    /// Streaming token sink. `None` for non-streaming requests.
    pub token_tx: Option<mpsc::UnboundedSender<String>>,
    /// Resolved when the browser signals completion (`inference-done`) or error.
    /// `Ok(text)` = full response; `Err(msg)` = error from browser.
    pub done_tx: oneshot::Sender<Result<String, String>>,
}

// ── Client ────────────────────────────────────────────────────────────────────

/// LLM client that delegates inference to the browser via WebGPU.
///
/// Created by `rebuild_inference` in `parish-server` and passed to
/// [`spawn_inference_worker`](crate::spawn_inference_worker) as any other client.
#[derive(Clone)]
pub struct WebGpuClient {
    request_tx: mpsc::Sender<WebGpuRequest>,
}

impl WebGpuClient {
    /// Creates a new client that forwards requests over `request_tx`.
    pub fn new(request_tx: mpsc::Sender<WebGpuRequest>) -> Self {
        Self { request_tx }
    }

    fn next_id(&self) -> u64 {
        NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    }

    /// Generates text (non-streaming). Blocks until the browser responds.
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        let id = self.next_id();
        let (done_tx, done_rx) = oneshot::channel();

        self.request_tx
            .send(WebGpuRequest {
                id,
                model: model.to_string(),
                prompt: prompt.to_string(),
                system: system.map(str::to_string),
                max_tokens,
                temperature,
                token_tx: None,
                done_tx,
            })
            .await
            .map_err(|_| ParishError::Inference("webgpu bridge closed".into()))?;

        done_rx
            .await
            .map_err(|_| ParishError::Inference("webgpu bridge dropped done_tx".into()))?
            .map_err(|e| ParishError::Inference(format!("webgpu error: {e}")))
    }

    /// Generates text with token streaming. Streams each token through `token_tx`.
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::UnboundedSender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        let id = self.next_id();
        let (done_tx, done_rx) = oneshot::channel();

        self.request_tx
            .send(WebGpuRequest {
                id,
                model: model.to_string(),
                prompt: prompt.to_string(),
                system: system.map(str::to_string),
                max_tokens,
                temperature,
                token_tx: Some(token_tx),
                done_tx,
            })
            .await
            .map_err(|_| ParishError::Inference("webgpu bridge closed".into()))?;

        done_rx
            .await
            .map_err(|_| ParishError::Inference("webgpu bridge dropped done_tx".into()))?
            .map_err(|e| ParishError::Inference(format!("webgpu error: {e}")))
    }

    /// Generates a JSON-structured response and deserialises it into `T`.
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<T, ParishError> {
        let text = self
            .generate(model, prompt, system, max_tokens, temperature)
            .await?;
        serde_json::from_str(&text).map_err(|e| {
            ParishError::Inference(format!(
                "webgpu json parse error: {e}\nresponse was: {text}"
            ))
        })
    }
}
