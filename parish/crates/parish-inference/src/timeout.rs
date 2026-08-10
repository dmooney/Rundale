//! Timeout and await helpers for inference responses.

use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{mpsc, oneshot};

use parish_types::ParishError;

use crate::any_client::TOKEN_CHANNEL_CAPACITY;
use crate::queue::{InferencePriority, InferenceQueue, InferenceResponse, QueueRequest};

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
pub static QUEUE_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

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
        .send(QueueRequest {
            id,
            model: model.to_string(),
            prompt: prompt.to_string(),
            system: system.map(String::from),
            token_tx: None,
            max_tokens,
            temperature: None,
            frequency_penalty: None,
            enable_thinking: None,
            reasoning_effort: None,
            priority,
            role: parish_config::InferenceCategory::Simulation,
            subrole: match priority {
                InferencePriority::Batch => parish_config::InferenceSubrole::Tier3Simulation,
                _ => parish_config::InferenceSubrole::Tier2Simulation,
            },
            profile: None,
            json_mode: false,
            json_schema: None,
            cancel: None,
        })
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
    cancel: Option<crate::queue::CancellationToken>,
) -> Result<T, ParishError> {
    let id = QUEUE_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    // Discard sink — we only need the streaming path enabled in the
    // worker; the assembled JSON arrives via the response channel.
    let (sink_tx, mut sink_rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    tokio::spawn(async move { while sink_rx.recv().await.is_some() {} });

    let response_rx = queue
        .send(QueueRequest {
            id,
            model: model.to_string(),
            prompt: prompt.to_string(),
            system: system.map(String::from),
            token_tx: Some(sink_tx),
            max_tokens,
            temperature: None,
            frequency_penalty: None,
            enable_thinking: None,
            reasoning_effort: None,
            priority,
            role: parish_config::InferenceCategory::Simulation,
            subrole: match priority {
                InferencePriority::Batch => parish_config::InferenceSubrole::Tier3Simulation,
                _ => parish_config::InferenceSubrole::Tier2Simulation,
            },
            profile: None,
            json_mode: false,
            json_schema: None,
            cancel,
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::{InferenceQueue, InferenceRequest, InferenceResponse};

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
}
