//! LLM inference pipeline via Ollama.
//!
//! Manages a request queue (Tokio mpsc channel), routes requests
//! to the Ollama API, and returns responses via oneshot channels.

pub mod client;
pub mod setup;

use client::OllamaClient;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

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
    /// Returns a oneshot receiver that will yield the response.
    /// Returns an error if the queue channel is closed.
    pub async fn send(
        &self,
        id: u64,
        model: String,
        prompt: String,
        system: Option<String>,
    ) -> Result<oneshot::Receiver<InferenceResponse>, mpsc::error::SendError<InferenceRequest>>
    {
        let (response_tx, response_rx) = oneshot::channel();
        let request = InferenceRequest {
            id,
            model,
            prompt,
            system,
            response_tx,
        };
        self.tx.send(request).await?;
        Ok(response_rx)
    }
}

/// Spawns the inference worker task.
///
/// The worker pulls requests from the mpsc receiver, calls the Ollama
/// client, and sends responses back through each request's oneshot channel.
/// The task runs until the sender side of the channel is dropped.
pub fn spawn_inference_worker(
    client: OllamaClient,
    mut rx: mpsc::Receiver<InferenceRequest>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            let result = client
                .generate(&request.model, &request.prompt, request.system.as_deref())
                .await;

            let response = match result {
                Ok(text) => InferenceResponse {
                    id: request.id,
                    text,
                    error: None,
                },
                Err(e) => InferenceResponse {
                    id: request.id,
                    text: String::new(),
                    error: Some(e.to_string()),
                },
            };

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
            .send(2, "model".to_string(), "prompt".to_string(), None)
            .await
            .unwrap();

        let request = rx.recv().await.unwrap();
        assert_eq!(request.id, 2);
        assert!(request.system.is_none());
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
}
