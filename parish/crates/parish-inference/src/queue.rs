//! Inference queue types: priority lanes, request/response, and the queue handle.

use tokio::sync::{mpsc, oneshot};
pub use tokio_util::sync::CancellationToken;

use crate::logs::DeferredInferenceAudit;
pub use crate::openai_client::JsonSchemaSpec;

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
    /// Optional OpenAI-compatible reasoning-mode control. Only measured
    /// provider/model profiles should set this; `None` omits the wire field.
    pub enable_thinking: Option<bool>,
    /// Optional provider reasoning effort for measured dialogue profiles.
    pub reasoning_effort: Option<parish_config::ReasoningEffort>,
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
    /// Request-scoped audit buffer used by atomic staged turns.
    pub deferred_audit: Option<DeferredInferenceAudit>,
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

/// All fields needed to submit one request through [`InferenceQueue::send`].
///
/// Replaces the former positional-argument family (`send`, `send_with_penalty`,
/// `send_with_schema`, `send_full`) that required `#[allow(clippy::too_many_arguments)]`.
/// Construct with struct literal syntax and leave optional fields as `None`
/// unless needed.
pub struct QueueRequest {
    /// Unique request identifier for correlation.
    pub id: u64,
    /// Model to use (e.g. `"gemma4:e4b"`).
    pub model: String,
    /// User-turn prompt text.
    pub prompt: String,
    /// Optional system prompt for context.
    pub system: Option<String>,
    /// Optional channel for streaming tokens before the final response.
    pub token_tx: Option<mpsc::Sender<String>>,
    /// Optional maximum number of tokens to generate.
    pub max_tokens: Option<u32>,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
    /// Optional OpenAI-compat `frequency_penalty`.  Forwarded to vllm-mlx,
    /// LM Studio, OpenAI, and OpenRouter; ignored by Anthropic/Simulator.
    pub frequency_penalty: Option<f32>,
    /// Optional OpenAI-compatible reasoning-mode control.
    pub enable_thinking: Option<bool>,
    /// Optional provider reasoning effort.
    pub reasoning_effort: Option<parish_config::ReasoningEffort>,
    /// Priority lane for this request.
    pub priority: InferencePriority,
    /// When `true`, the worker uses JSON mode streaming.
    pub json_mode: bool,
    /// Optional strict structured-output schema (takes precedence over
    /// `json_mode` when both are set).
    pub json_schema: Option<JsonSchemaSpec>,
    /// Optional cancellation token.
    pub cancel: Option<CancellationToken>,
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
    deferred_audit: Option<DeferredInferenceAudit>,
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
            deferred_audit: None,
        }
    }

    /// Returns a queue handle whose requests buffer inference audit records
    /// until the supplied scope is committed.
    pub fn with_deferred_audit(&self, audit: DeferredInferenceAudit) -> Self {
        Self {
            interactive_tx: self.interactive_tx.clone(),
            background_tx: self.background_tx.clone(),
            batch_tx: self.batch_tx.clone(),
            deferred_audit: Some(audit),
        }
    }

    /// Submits a request to the appropriate priority lane.
    ///
    /// All request fields are carried in [`QueueRequest`] to keep the function
    /// signature short.  Returns a oneshot receiver that will yield the complete
    /// response, or an error if the queue channel is closed.
    pub async fn send(
        &self,
        req: QueueRequest,
    ) -> Result<oneshot::Receiver<InferenceResponse>, mpsc::error::SendError<InferenceRequest>>
    {
        let priority = req.priority;
        let (response_tx, response_rx) = oneshot::channel();
        let request = InferenceRequest {
            id: req.id,
            model: req.model,
            prompt: req.prompt,
            system: req.system,
            response_tx,
            token_tx: req.token_tx,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            frequency_penalty: req.frequency_penalty,
            enable_thinking: req.enable_thinking,
            reasoning_effort: req.reasoning_effort,
            priority,
            json_mode: req.json_mode,
            json_schema: req.json_schema,
            cancel: req.cancel,
            deferred_audit: self.deferred_audit.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::any_client::TOKEN_CHANNEL_CAPACITY;

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
            .send(QueueRequest {
                id: 1,
                model: "test-model".to_string(),
                prompt: "hello".to_string(),
                system: Some("system".to_string()),
                token_tx: None,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
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

    /// TODO #10 / #23 / #34 — `QueueRequest::frequency_penalty` must be
    /// carried onto the `InferenceRequest` so the worker can forward it to
    /// the underlying client. Regressing this field to `None` would silently
    /// disable the Tier 1 repetition-loop fix.
    #[tokio::test]
    async fn test_inference_queue_send_with_penalty_carries_field() {
        let (queue, mut irx, _brx, _batrx) = make_queue();
        let _rx = queue
            .send(QueueRequest {
                id: 42,
                model: "dialogue-model".to_string(),
                prompt: "prompt".to_string(),
                system: Some("system".to_string()),
                token_tx: None,
                max_tokens: Some(256),
                temperature: Some(0.7),
                frequency_penalty: Some(0.5),
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                json_mode: true,
                json_schema: None,
                cancel: None,
            })
            .await
            .unwrap();

        let request = irx.recv().await.unwrap();
        assert_eq!(request.id, 42);
        assert_eq!(request.frequency_penalty, Some(0.5));
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(256));
        assert!(request.json_mode);
    }

    /// Callers that leave `frequency_penalty` unset must get `None`
    /// on the request so non-dialogue tiers don't accidentally pick up
    /// the Tier 1 sampling override.
    #[tokio::test]
    async fn test_inference_queue_send_default_omits_frequency_penalty() {
        // Send into the Interactive lane and receive on `irx` so the
        // request actually surfaces. The original Background-into-irx
        // mismatch hung this test forever and stalled CI's quality
        // gate at the 30-minute timeout (#1127 follow-up).
        let (queue, mut irx, _brx, _batrx) = make_queue();
        let _rx = queue
            .send(QueueRequest {
                id: 43,
                model: "m".to_string(),
                prompt: "p".to_string(),
                system: None,
                token_tx: None,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
            .await
            .unwrap();
        let request = irx.recv().await.unwrap();
        assert_eq!(request.frequency_penalty, None);
    }

    #[tokio::test]
    async fn test_inference_queue_no_system() {
        let (queue, mut irx, _brx, _batrx) = make_queue();

        let _response_rx = queue
            .send(QueueRequest {
                id: 2,
                model: "model".to_string(),
                prompt: "prompt".to_string(),
                system: None,
                token_tx: None,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
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
            .send(QueueRequest {
                id: 3,
                model: "model".to_string(),
                prompt: "prompt".to_string(),
                system: None,
                token_tx: Some(token_tx),
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
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
            .send(QueueRequest {
                id: 10,
                model: "m".to_string(),
                prompt: "p".to_string(),
                system: None,
                token_tx: None,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
            .await
            .unwrap();
        let _rx2 = queue
            .send(QueueRequest {
                id: 11,
                model: "m".to_string(),
                prompt: "p".to_string(),
                system: None,
                token_tx: None,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Background,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
            .await
            .unwrap();
        let _rx3 = queue
            .send(QueueRequest {
                id: 12,
                model: "m".to_string(),
                prompt: "p".to_string(),
                system: None,
                token_tx: None,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Batch,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
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
            .send(QueueRequest {
                id: 20,
                model: "m".to_string(),
                prompt: "batch".to_string(),
                system: None,
                token_tx: None,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Batch,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
            .await
            .unwrap();
        let _rx_interactive = queue
            .send(QueueRequest {
                id: 21,
                model: "m".to_string(),
                prompt: "interactive".to_string(),
                system: None,
                token_tx: None,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
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
}
