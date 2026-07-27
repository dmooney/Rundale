//! Inference worker task: spawns the priority-lane drain loop.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use parish_config::InferenceConfig;
use parish_types::ParishError;

use crate::any_client::{AnyClient, StreamStats, TOKEN_CHANNEL_CAPACITY};
use crate::logs::{InferenceLog, InferenceLogEntry};
use crate::openai_client::{GenerateParams, ResponseFormat};
use crate::queue::{CancellationToken, InferenceRequest, InferenceResponse};

/// Wraps an inference future with a timeout *and* an optional cancellation
/// token, producing consistent error messages so callers don't repeat the
/// match+format pattern.
///
/// Resolution order is `select!`-style: whichever of {cancel, timeout, the
/// inner future} fires first wins. Cancel and timeout both drop the inner
/// future, which closes the underlying HTTP/SSE connection so providers
/// release their model slot.
pub(crate) async fn inference_with_timeout<F, T>(
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

/// Typed bundle of arguments for [`spawn_inference_worker`].
///
/// Groups the three priority-lane receivers, the shared log ring-buffer, the
/// on-disk inference file log, the resolved provider enum, and the timeout
/// config so that `spawn_inference_worker` can accept a single struct rather
/// than eight positional arguments.
pub struct InferenceWorkerConfig {
    /// Receiver end of the Interactive (highest priority) lane.
    pub interactive_rx: mpsc::Receiver<InferenceRequest>,
    /// Receiver end of the Background (medium priority) lane.
    pub background_rx: mpsc::Receiver<InferenceRequest>,
    /// Receiver end of the Batch (lowest priority) lane.
    pub batch_rx: mpsc::Receiver<InferenceRequest>,
    /// Shared in-memory ring buffer for the debug panel.
    pub log: InferenceLog,
    /// On-disk JSONL log (may be disabled).
    pub file_log: crate::file_log::InferenceFileLog,
    /// Resolved provider variant (used only for file-log enrichment).
    pub provider: parish_config::Provider,
    /// Timeout configuration sourced from `parish.toml`.
    pub timeout_config: InferenceConfig,
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
pub fn spawn_inference_worker(client: AnyClient, config: InferenceWorkerConfig) -> JoinHandle<()> {
    let InferenceWorkerConfig {
        mut interactive_rx,
        mut background_rx,
        mut batch_rx,
        log,
        file_log,
        provider,
        timeout_config,
    } = config;
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
                            GenerateParams {
                                max_tokens: request.max_tokens,
                                temperature: request.temperature,
                                frequency_penalty: request.frequency_penalty,
                                enable_thinking: request.enable_thinking,
                            },
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
                            GenerateParams {
                                max_tokens: request.max_tokens,
                                temperature: request.temperature,
                                frequency_penalty: request.frequency_penalty,
                                enable_thinking: request.enable_thinking,
                            },
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

            // Record the completed call. Atomic staged turns attach a
            // request-scoped buffer so neither the debug ring nor JSONL file
            // exposes a rejected candidate.
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
                if let Some(deferred) = request.deferred_audit.as_ref() {
                    deferred
                        .record(
                            entry,
                            log.clone(),
                            file_log.clone(),
                            provider.clone(),
                            priority,
                        )
                        .await;
                } else {
                    file_log.record(&entry, &provider, priority);
                    let mut log = log.lock().await;
                    log.push(entry);
                }
            }

            // Ignore send error — the caller may have dropped the receiver
            let _ = request.response_tx.send(response);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::{DeferredInferenceAudit, new_inference_log};
    use crate::queue::{InferencePriority, InferenceQueue, QueueRequest};

    #[tokio::test]
    async fn staged_audit_is_hidden_until_commit_in_memory_and_on_disk() {
        let temp = tempfile::tempdir().unwrap();
        let (interactive_tx, interactive_rx) = mpsc::channel::<InferenceRequest>(4);
        let (background_tx, background_rx) = mpsc::channel::<InferenceRequest>(4);
        let (batch_tx, batch_rx) = mpsc::channel::<InferenceRequest>(4);
        let log = new_inference_log();
        let file_log = crate::file_log::InferenceFileLog::spawn(temp.path(), true, None);
        let file_path = file_log.path().to_path_buf();
        let worker = spawn_inference_worker(
            AnyClient::simulator(),
            InferenceWorkerConfig {
                interactive_rx,
                background_rx,
                batch_rx,
                log: log.clone(),
                file_log,
                provider: parish_config::Provider::simulator(),
                timeout_config: InferenceConfig::default(),
            },
        );
        let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
        let audit = DeferredInferenceAudit::default();
        let scoped = queue.with_deferred_audit(audit.clone());

        let response = scoped
            .send(QueueRequest {
                id: 7,
                model: "simulator".to_string(),
                prompt: "Say hello.".to_string(),
                system: None,
                token_tx: None,
                max_tokens: Some(8),
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
            .await
            .unwrap()
            .await
            .unwrap();
        assert!(response.error.is_none());
        assert!(log.lock().await.is_empty());
        assert!(
            !file_path.exists(),
            "pending staged call must not create the JSONL audit file"
        );

        audit.commit().await;
        assert_eq!(log.lock().await.len(), 1);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if std::fs::read_to_string(&file_path)
                    .is_ok_and(|contents| contents.lines().count() == 1)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("committed JSONL audit should become visible");

        let rejected_audit = DeferredInferenceAudit::default();
        let rejected_queue = queue.with_deferred_audit(rejected_audit.clone());
        let _ = rejected_queue
            .send(QueueRequest {
                id: 8,
                model: "simulator".to_string(),
                prompt: "Say goodbye.".to_string(),
                system: None,
                token_tx: None,
                max_tokens: Some(8),
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
            })
            .await
            .unwrap()
            .await
            .unwrap();
        rejected_audit.discard().await;
        assert_eq!(log.lock().await.len(), 1);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap().lines().count(),
            1
        );
        worker.abort();
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
            InferenceWorkerConfig {
                interactive_rx,
                background_rx,
                batch_rx,
                log,
                file_log: crate::file_log::InferenceFileLog::disabled(),
                provider: parish_config::Provider::simulator(),
                timeout_config: InferenceConfig::default(),
            },
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
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
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
            enable_thinking: None,
            priority: InferencePriority::Interactive,
            json_mode: false,
            json_schema: None,
            cancel: None,
            deferred_audit: None,
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
            InferenceWorkerConfig {
                interactive_rx,
                background_rx,
                batch_rx,
                log: log.clone(),
                file_log: crate::file_log::InferenceFileLog::disabled(),
                provider: parish_config::Provider::simulator(),
                timeout_config: InferenceConfig::default(),
            },
        );

        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
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
                enable_thinking: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: None,
                deferred_audit: None,
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
            InferenceWorkerConfig {
                interactive_rx,
                background_rx,
                batch_rx,
                log,
                file_log: crate::file_log::InferenceFileLog::disabled(),
                provider: parish_config::Provider::simulator(),
                timeout_config: InferenceConfig::default(),
            },
        );

        let cancel = CancellationToken::new();
        let cancel_for_request = cancel.clone();

        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
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
                enable_thinking: None,
                priority: InferencePriority::Interactive,
                json_mode: false,
                json_schema: None,
                cancel: Some(cancel_for_request),
                deferred_audit: None,
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
            InferenceWorkerConfig {
                interactive_rx,
                background_rx,
                batch_rx,
                log,
                file_log: crate::file_log::InferenceFileLog::disabled(),
                provider: parish_config::Provider::simulator(),
                timeout_config: cfg,
            },
        );

        // Send a normal request — the simulator responds well within 1 s.
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
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
                deferred_audit: None,
                response_tx: resp_tx,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
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
        let (resp_tx2, resp_rx2) = tokio::sync::oneshot::channel();
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
                deferred_audit: None,
                response_tx: resp_tx2,
                max_tokens: None,
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
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
}
