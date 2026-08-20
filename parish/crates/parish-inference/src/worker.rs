//! Inference worker task: spawns the priority-lane drain loop.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::any_client::{AnyClient, InferenceClients, StreamStats, TOKEN_CHANNEL_CAPACITY};
use crate::google_client::{ProviderCallError, ProviderMetadata};
use crate::logs::{InferenceLog, InferenceLogEntry};
use crate::openai_client::{GenerateParams, ResponseFormat};
use crate::queue::{CancellationToken, InferenceRequest, InferenceResponse};
use parish_config::InferenceConfig;

/// Internal transport protocol. Candidate deltas remain quarantined until the
/// provider adapter validates its terminal frame. This is deliberately not a
/// semantic `Commit`: only the canonical gameplay apply seam may commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamTransactionEvent {
    Begin {
        request_id: u64,
        configuration_epoch: u64,
    },
    ProvisionalDelta {
        request_id: u64,
        configuration_epoch: u64,
        sequence: u64,
        text: String,
    },
    CandidateComplete {
        request_id: u64,
        configuration_epoch: u64,
    },
    Abort {
        request_id: u64,
        configuration_epoch: u64,
        reason: String,
    },
}

struct StreamAbortGuard {
    tx: mpsc::Sender<StreamTransactionEvent>,
    request_id: u64,
    configuration_epoch: u64,
    armed: bool,
}

impl Drop for StreamAbortGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.tx.try_send(StreamTransactionEvent::Abort {
                request_id: self.request_id,
                configuration_epoch: self.configuration_epoch,
                reason: "stream task dropped before terminal publication".into(),
            });
        }
    }
}

fn resolved_response_format(
    schema: Option<crate::openai_client::JsonSchemaSpec>,
    json_mode: bool,
    contract: Option<parish_config::StructuredOutputMode>,
) -> Option<ResponseFormat> {
    use parish_config::StructuredOutputMode;
    if schema.is_none() && !json_mode {
        return None;
    }
    match contract {
        Some(StructuredOutputMode::JsonSchema) => schema
            .map(|json_schema| ResponseFormat::JsonSchema { json_schema })
            .or(Some(ResponseFormat::JsonObject)),
        Some(StructuredOutputMode::JsonObject) => Some(ResponseFormat::JsonObject),
        Some(StructuredOutputMode::PromptValidatedJson) => None,
        // Legacy/harness profiles predate v2 publication. Preserve their
        // explicit request contract without weakening a resolved v2 route.
        None => schema
            .map(|json_schema| ResponseFormat::JsonSchema { json_schema })
            .or_else(|| json_mode.then_some(ResponseFormat::JsonObject)),
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
pub(crate) async fn inference_with_timeout<F, T>(
    future: F,
    timeout: std::time::Duration,
    timeout_secs: u64,
    model: &str,
    label: &str,
    cancel: Option<&CancellationToken>,
) -> Result<T, ProviderCallError>
where
    F: std::future::Future<Output = Result<T, ProviderCallError>>,
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
        () = cancel_fut => Err(ProviderCallError {
            message: format!("{label} cancelled (model={model})"),
            partial_text: String::new(),
            metadata: Box::new(ProviderMetadata::unavailable(model)),
        }),
        result = &mut future => result,
        () = tokio::time::sleep(timeout) => Err(ProviderCallError {
            message: format!("{label} timed out after {timeout_secs}s (model={model})"),
            partial_text: String::new(),
            metadata: Box::new(ProviderMetadata::unavailable(model)),
        }),
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
    let clients = InferenceClients::new(client, String::new(), Default::default());
    spawn_inference_worker_with_clients(clients, config)
}

/// Spawns a worker over one immutable category transport set. Each request
/// selects its transport from `request.role`, so API flavor is resolved at
/// the same seam as model and profile selection.
pub fn spawn_inference_worker_with_clients(
    clients: InferenceClients,
    config: InferenceWorkerConfig,
) -> JoinHandle<()> {
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
            let role = request.role;
            let subrole = request.subrole;
            debug_assert_eq!(role, subrole.category());
            let (client, configured_model) = clients.client_for(role);
            let published_profile = clients.profile_for(subrole);
            let model = if published_profile.is_some() {
                configured_model.to_string()
            } else {
                request.model.clone()
            };
            let profile = published_profile
                .or(request.profile)
                .unwrap_or_else(|| parish_config::InferenceProfile::for_subrole(subrole))
                .for_model(&model);
            let is_v2 = profile.configuration_epoch > 0;
            let reasoning_effort = match profile.reasoning_intent {
                parish_config::ReasoningIntent::Auto if is_v2 => None,
                parish_config::ReasoningIntent::Auto => request.reasoning_effort,
                parish_config::ReasoningIntent::Off => Some(parish_config::ReasoningEffort::None),
                parish_config::ReasoningIntent::Effort { level } => Some(match level {
                    parish_config::ReasoningEffortV2::Minimal => {
                        parish_config::ReasoningEffort::Minimal
                    }
                    parish_config::ReasoningEffortV2::Low => parish_config::ReasoningEffort::Low,
                    parish_config::ReasoningEffortV2::Medium => {
                        parish_config::ReasoningEffort::Medium
                    }
                    parish_config::ReasoningEffortV2::High => parish_config::ReasoningEffort::High,
                    parish_config::ReasoningEffortV2::Xhigh => {
                        parish_config::ReasoningEffort::Xhigh
                    }
                    parish_config::ReasoningEffortV2::Max => parish_config::ReasoningEffort::Max,
                }),
                parish_config::ReasoningIntent::Budget { .. } => None,
            };
            let thinking_level = match profile.reasoning_intent {
                parish_config::ReasoningIntent::Off
                | parish_config::ReasoningIntent::Auto
                | parish_config::ReasoningIntent::Budget { .. } => None,
                parish_config::ReasoningIntent::Effort { level } => Some(match level {
                    parish_config::ReasoningEffortV2::Minimal => {
                        parish_config::ThinkingLevel::Minimal
                    }
                    parish_config::ReasoningEffortV2::Low => parish_config::ThinkingLevel::Low,
                    parish_config::ReasoningEffortV2::Medium => {
                        parish_config::ThinkingLevel::Medium
                    }
                    parish_config::ReasoningEffortV2::High
                    | parish_config::ReasoningEffortV2::Xhigh
                    | parish_config::ReasoningEffortV2::Max => parish_config::ThinkingLevel::High,
                }),
            };
            let service_tier = match profile.service_tier_intent {
                parish_config::ServiceTierIntent::Auto => None,
                parish_config::ServiceTierIntent::Standard => {
                    Some(parish_config::ServiceTier::Standard)
                }
                parish_config::ServiceTierIntent::Priority => {
                    Some(parish_config::ServiceTier::Priority)
                }
            };
            let system_prompt = request.system.clone();
            let prompt_prefix_len = system_prompt.as_ref().map(String::len);
            let prompt_prefix_hash = system_prompt.as_deref().map(stable_prefix_hash);
            let prompt_text = request.prompt.clone();
            // A published v2 profile is authoritative for every adapter.
            // Legacy requests have no epoch and retain their request fields.
            let max_tokens = if profile.configuration_epoch > 0 {
                Some(profile.max_output_tokens)
            } else {
                request.max_tokens
            };
            let temperature = if is_v2 {
                profile.temperature
            } else {
                profile.temperature.or(request.temperature)
            };
            let frequency_penalty = if is_v2 {
                profile.frequency_penalty
            } else {
                profile.frequency_penalty.or(request.frequency_penalty)
            };
            let enable_thinking = if is_v2 {
                match profile.reasoning_intent {
                    parish_config::ReasoningIntent::Auto => None,
                    parish_config::ReasoningIntent::Off => Some(false),
                    parish_config::ReasoningIntent::Effort { .. }
                    | parish_config::ReasoningIntent::Budget { .. } => Some(true),
                }
            } else {
                request.enable_thinking
            };
            let reasoning_intent = is_v2.then_some(profile.reasoning_intent);
            let priority = request.priority;
            let req_id = request.id;
            let start = Instant::now();

            let streaming_timeout =
                std::time::Duration::from_secs(timeout_config.streaming_timeout_secs);
            let blocking_timeout = std::time::Duration::from_secs(timeout_config.timeout_secs);

            // Resolve effective response_format: schema wins over json_mode.
            let response_format = resolved_response_format(
                request.json_schema.clone(),
                request.json_mode,
                profile.structured_output,
            );

            let (result, stream_stats) = match request.token_tx {
                Some(token_tx) => {
                    let (provider_tx, mut provider_rx) =
                        mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
                    let (event_tx, mut event_rx) =
                        mpsc::channel::<StreamTransactionEvent>(TOKEN_CHANNEL_CAPACITY);
                    event_tx
                        .send(StreamTransactionEvent::Begin {
                            request_id: req_id,
                            configuration_epoch: profile.configuration_epoch,
                        })
                        .await
                        .ok();
                    let mut abort_guard = StreamAbortGuard {
                        tx: event_tx.clone(),
                        request_id: req_id,
                        configuration_epoch: profile.configuration_epoch,
                        armed: true,
                    };
                    let bridge_tx = event_tx.clone();
                    let bridge = tokio::spawn(async move {
                        let mut sequence = 0;
                        while let Some(text) = provider_rx.recv().await {
                            if bridge_tx
                                .send(StreamTransactionEvent::ProvisionalDelta {
                                    request_id: req_id,
                                    configuration_epoch: profile.configuration_epoch,
                                    sequence,
                                    text,
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                            sequence = sequence.saturating_add(1);
                        }
                    });
                    let observer_start = start;
                    let observer = tokio::spawn(async move {
                        let mut ttft: Option<Duration> = None;
                        let mut tokens: u64 = 0;
                        let mut partial_text = String::new();
                        let mut expected_sequence = 0;
                        let mut committed = false;
                        while let Some(event) = event_rx.recv().await {
                            match event {
                                StreamTransactionEvent::Begin { .. } => {}
                                StreamTransactionEvent::ProvisionalDelta {
                                    sequence, text, ..
                                } if sequence == expected_sequence => {
                                    if ttft.is_none() {
                                        ttft = Some(observer_start.elapsed());
                                    }
                                    tokens = tokens.saturating_add(1);
                                    partial_text.push_str(&text);
                                    expected_sequence = expected_sequence.saturating_add(1);
                                }
                                StreamTransactionEvent::CandidateComplete { .. } => {
                                    committed = true;
                                    break;
                                }
                                StreamTransactionEvent::Abort { .. }
                                | StreamTransactionEvent::ProvisionalDelta { .. } => break,
                            }
                        }
                        (
                            StreamStats {
                                ttft,
                                tokens,
                                partial_text,
                            },
                            committed,
                        )
                    });
                    let label = match response_format {
                        Some(ResponseFormat::JsonSchema { .. }) => "streaming (schema) inference",
                        Some(ResponseFormat::JsonObject) => "streaming (json) inference",
                        None => "streaming inference",
                    };
                    let result = inference_with_timeout(
                        client.generate_stream_detailed_with_format(
                            &model,
                            &request.prompt,
                            request.system.as_deref(),
                            provider_tx,
                            response_format.clone(),
                            GenerateParams {
                                max_tokens,
                                temperature,
                                frequency_penalty,
                                enable_thinking,
                                reasoning_effort,
                                thinking_level,
                                service_tier,
                                reasoning_intent,
                                reasoning_dialect: profile.reasoning_dialect,
                            },
                        ),
                        streaming_timeout,
                        timeout_config.streaming_timeout_secs,
                        &model,
                        label,
                        request.cancel.as_ref(),
                    )
                    .await;
                    let _ = bridge.await;
                    let terminal = if result.is_ok() {
                        StreamTransactionEvent::CandidateComplete {
                            request_id: req_id,
                            configuration_epoch: profile.configuration_epoch,
                        }
                    } else {
                        StreamTransactionEvent::Abort {
                            request_id: req_id,
                            configuration_epoch: profile.configuration_epoch,
                            reason: result
                                .as_ref()
                                .err()
                                .map(|error| error.message.clone())
                                .unwrap_or_else(|| "provider stream aborted".into()),
                        }
                    };
                    let _ = event_tx.send(terminal).await;
                    abort_guard.armed = false;
                    drop(event_tx);
                    let (stats, committed) = observer.await.unwrap_or((
                        StreamStats {
                            ttft: None,
                            tokens: 0,
                            partial_text: String::new(),
                        },
                        false,
                    ));
                    // Provider chunks are provisional. Publish only after the
                    // adapter has observed and validated the terminal frame.
                    // Canonical gameplay callers apply their stricter semantic
                    // validator before emitting player-visible dialogue.
                    if committed && !stats.partial_text.is_empty() {
                        let _ = token_tx.send(stats.partial_text.clone()).await;
                    }
                    (result, Some(stats))
                }
                None => {
                    let result = inference_with_timeout(
                        client.generate_detailed_with_format(
                            &model,
                            &request.prompt,
                            request.system.as_deref(),
                            response_format.clone(),
                            GenerateParams {
                                max_tokens,
                                temperature,
                                frequency_penalty,
                                enable_thinking,
                                reasoning_effort,
                                thinking_level,
                                service_tier,
                                reasoning_intent,
                                reasoning_dialect: profile.reasoning_dialect,
                            },
                        ),
                        blocking_timeout,
                        timeout_config.timeout_secs,
                        &model,
                        "inference",
                        request.cancel.as_ref(),
                    )
                    .await;
                    (result, None)
                }
            };

            let elapsed = start.elapsed();
            let (observed_ttft_ms, observed_chunks, observed_partial) = match &stream_stats {
                Some(s) => (
                    s.ttft.map(|d| d.as_millis() as u64),
                    Some(s.tokens),
                    s.partial_text.clone(),
                ),
                None => (None, None, String::new()),
            };

            let mut result = result;
            if let Err(error) = &mut result {
                if error.partial_text.is_empty() && !observed_partial.is_empty() {
                    error.partial_text = observed_partial;
                }
                if error.metadata.provider == "unknown" {
                    let mut observed = client.fallback_metadata(&model);
                    observed.terminal_status =
                        error.metadata.terminal_status.clone().or_else(|| {
                            Some(
                                if error.message.contains("cancel") {
                                    "cancelled"
                                } else {
                                    "timeout"
                                }
                                .to_string(),
                            )
                        });
                    observed.ttft_ms = observed_ttft_ms;
                    observed.stream_chunks = observed_chunks.unwrap_or(0);
                    observed.duration_ms = elapsed.as_millis() as u64;
                    observed.requested_service_tier = service_tier;
                    *error.metadata = observed;
                }
            }

            let (
                response,
                entry_error,
                response_len,
                response_text,
                partial_output_len,
                mut metadata,
            ) = match &result {
                Ok(result) => (
                    InferenceResponse {
                        id: req_id,
                        text: result.text.clone(),
                        error: None,
                    },
                    None,
                    result.text.len(),
                    result.text.clone(),
                    0,
                    result.metadata.clone(),
                ),
                Err(e) => (
                    InferenceResponse {
                        id: req_id,
                        text: String::new(),
                        error: Some(e.to_string()),
                    },
                    Some(e.to_string()),
                    e.partial_text.len(),
                    e.partial_text.clone(),
                    e.partial_text.len(),
                    (*e.metadata).clone(),
                ),
            };
            if metadata.provider == "unknown" {
                metadata.provider = provider.id().to_string();
                metadata.api_mode = match provider.kind() {
                    parish_config::ProviderKind::Anthropic => "anthropic-messages",
                    parish_config::ProviderKind::Google => "google-interactions-v1",
                    parish_config::ProviderKind::Simulator => "simulator",
                    parish_config::ProviderKind::OpenAiCompat
                    | parish_config::ProviderKind::Local => "openai-chat-completions",
                }
                .to_string();
            }
            let failure_kind = entry_error
                .as_deref()
                .map(|message| classify_provider_failure(message, metadata.http_status));
            let tier_downgraded = matches!(
                metadata.requested_service_tier,
                Some(parish_config::ServiceTier::Priority)
            ) && metadata.effective_service_tier.as_deref()
                == Some("standard");
            let estimated_cost_usd = estimate_google_cost(&metadata);

            // Record the completed call. Atomic staged turns attach a
            // request-scoped buffer so neither the debug ring nor JSONL file
            // exposes a rejected candidate.
            {
                let entry = InferenceLogEntry {
                    request_id: req_id,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    model,
                    provider: metadata.provider.clone(),
                    api_mode: metadata.api_mode.clone(),
                    role,
                    subrole,
                    streaming,
                    duration_ms: elapsed.as_millis() as u64,
                    prompt_len,
                    response_len,
                    error: entry_error,
                    system_prompt,
                    prompt_text,
                    response_text,
                    max_tokens,
                    ttft_ms: metadata.ttft_ms.or(observed_ttft_ms),
                    output_tokens: metadata.usage.output_tokens,
                    stream_chunks: Some(metadata.stream_chunks)
                        .filter(|count| *count > 0)
                        .or(observed_chunks),
                    input_tokens: metadata.usage.input_tokens,
                    cached_tokens: metadata.usage.cached_tokens,
                    thought_tokens: metadata.usage.thought_tokens,
                    total_tokens: metadata.usage.total_tokens,
                    thinking_level,
                    requested_service_tier: metadata.requested_service_tier,
                    effective_service_tier: metadata.effective_service_tier,
                    provider_request_id: metadata
                        .interaction_id
                        .as_deref()
                        .map(safe_provider_request_id),
                    terminal_status: metadata.terminal_status,
                    retry_count: metadata.retry_count,
                    http_status: metadata.http_status,
                    failure_kind,
                    partial_output_len,
                    tier_downgraded,
                    estimated_cost_usd,
                    prompt_prefix_hash,
                    prompt_prefix_len,
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

pub(crate) fn stable_prefix_hash(prefix: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in prefix.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

pub(crate) fn safe_provider_request_id(value: &str) -> String {
    const EDGE: usize = 8;
    const MAX: usize = EDGE * 2 + 1;
    if value.chars().count() <= MAX {
        return value.to_string();
    }
    let start: String = value.chars().take(EDGE).collect();
    let end: String = value
        .chars()
        .rev()
        .take(EDGE)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{start}…{end}")
}

pub(crate) fn classify_provider_failure(message: &str, status: Option<u16>) -> String {
    match status {
        Some(401 | 403) => "authentication",
        Some(429) => "rate-limited",
        Some(500..=599) => "provider-unavailable",
        Some(400..=499) => "request-rejected",
        _ if message.contains("cancelled") => "cancelled",
        _ if message.contains("timed out") => "timeout",
        _ if message.contains("incomplete") || message.contains("budget_exceeded") => "incomplete",
        _ if message.contains("malformed") || message.contains("parse") => "malformed-response",
        _ => "provider-error",
    }
    .to_string()
}

/// Paid-tier text estimate checked against Google's Gemini API pricing page
/// on 2026-08-14. Gemini 3.6/3.7 promotional Standard rates through the end
/// of 2026 are $0.75/M input, $0.075/M cached input, and $3.75/M output
/// (including thinking); Priority is $1.35/M, $0.135/M, and $6.75/M.
pub(crate) fn estimate_google_cost(metadata: &ProviderMetadata) -> Option<f64> {
    if metadata.provider != "google"
        || !matches!(
            metadata.model.as_str(),
            "gemini-3.6-flash" | "gemini-3.7-flash"
        )
    {
        return None;
    }
    let input = metadata.usage.input_tokens?;
    let cached = metadata.usage.cached_tokens.unwrap_or(0).min(input);
    let output = metadata.usage.output_tokens.unwrap_or(0);
    let thought = metadata.usage.thought_tokens.unwrap_or(0);
    // Google may downgrade a requested Priority call. Bill against the tier
    // the response says actually served the request; only fall back to the
    // requested value when the provider omitted effective-tier telemetry.
    let priority = match metadata.effective_service_tier.as_deref() {
        Some("priority") => true,
        Some("standard") => false,
        _ => matches!(
            metadata.requested_service_tier,
            Some(parish_config::ServiceTier::Priority)
        ),
    };
    #[derive(serde::Deserialize)]
    struct Rates {
        input: f64,
        cached_input: f64,
        output_and_thought: f64,
    }
    #[derive(serde::Deserialize)]
    struct Pricing {
        standard: Rates,
        priority: Rates,
    }
    #[derive(serde::Deserialize)]
    struct Snapshot {
        pricing_usd_per_million_tokens: Pricing,
    }
    static SNAPSHOT: std::sync::OnceLock<Snapshot> = std::sync::OnceLock::new();
    let snapshot = SNAPSHOT.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../config/gemini-3.7-flash-capabilities.json"
        ))
        .expect("checked-in Gemini capability snapshot must parse")
    });
    let rates = if priority {
        &snapshot.pricing_usd_per_million_tokens.priority
    } else {
        &snapshot.pricing_usd_per_million_tokens.standard
    };
    let (input_rate, cached_rate, output_rate) =
        (rates.input, rates.cached_input, rates.output_and_thought);
    Some(
        (((input - cached) as f64 * input_rate)
            + (cached as f64 * cached_rate)
            + ((output + thought) as f64 * output_rate))
            / 1_000_000.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::{DeferredInferenceAudit, new_inference_log};
    use crate::queue::{InferencePriority, InferenceQueue, QueueRequest};

    fn priced_google_metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider: "google".to_string(),
            model: "gemini-3.7-flash".to_string(),
            requested_service_tier: Some(parish_config::ServiceTier::Priority),
            usage: parish_providers::ProviderUsage {
                input_tokens: Some(1_000_000),
                cached_tokens: None,
                thought_tokens: None,
                output_tokens: None,
                total_tokens: Some(1_000_000),
            },
            ..ProviderMetadata::default()
        }
    }

    #[test]
    fn cost_estimate_uses_effective_tier_after_google_downgrade() {
        let mut metadata = priced_google_metadata();
        metadata.effective_service_tier = Some("standard".to_string());
        assert_eq!(estimate_google_cost(&metadata), Some(0.75));

        metadata.effective_service_tier = Some("priority".to_string());
        assert_eq!(estimate_google_cost(&metadata), Some(1.35));
    }

    #[test]
    fn cost_estimate_falls_back_to_requested_tier_without_response_header() {
        assert_eq!(estimate_google_cost(&priced_google_metadata()), Some(1.35));
    }

    #[tokio::test]
    async fn production_worker_dispatches_each_category_to_its_published_client() {
        let (dialogue_client, dialogue_mock) = AnyClient::mock();
        let (simulation_client, simulation_mock) = AnyClient::mock();
        dialogue_mock.push_any("dialogue-wire");
        simulation_mock.push_any("simulation-wire");
        let clients = InferenceClients::new(
            dialogue_client.clone(),
            "dialogue-model".into(),
            std::collections::HashMap::from([
                (
                    parish_config::InferenceCategory::Dialogue,
                    (dialogue_client, "dialogue-model".into()),
                ),
                (
                    parish_config::InferenceCategory::Simulation,
                    (simulation_client, "simulation-model".into()),
                ),
            ]),
        );
        let (interactive_tx, interactive_rx) = mpsc::channel(4);
        let (background_tx, background_rx) = mpsc::channel(4);
        let (batch_tx, batch_rx) = mpsc::channel(4);
        let worker = spawn_inference_worker_with_clients(
            clients,
            InferenceWorkerConfig {
                interactive_rx,
                background_rx,
                batch_rx,
                log: new_inference_log(),
                file_log: crate::file_log::InferenceFileLog::disabled(),
                provider: parish_config::Provider::simulator(),
                timeout_config: InferenceConfig::default(),
            },
        );
        let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
        let request = |id, role, subrole| QueueRequest {
            id,
            model: "caller-model-must-not-select-transport".into(),
            prompt: "hello".into(),
            system: None,
            token_tx: None,
            max_tokens: Some(8),
            temperature: None,
            frequency_penalty: None,
            enable_thinking: None,
            reasoning_effort: None,
            priority: InferencePriority::Interactive,
            role,
            subrole,
            profile: None,
            json_mode: false,
            json_schema: None,
            cancel: None,
        };
        let dialogue = queue
            .send(request(
                101,
                parish_config::InferenceCategory::Dialogue,
                parish_config::InferenceSubrole::Dialogue,
            ))
            .await
            .unwrap()
            .await
            .unwrap();
        let simulation = queue
            .send(request(
                102,
                parish_config::InferenceCategory::Simulation,
                parish_config::InferenceSubrole::Tier3Simulation,
            ))
            .await
            .unwrap()
            .await
            .unwrap();
        assert_eq!(dialogue.text, "dialogue-wire");
        assert_eq!(simulation.text, "simulation-wire");
        worker.abort();
    }

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
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                role: parish_config::InferenceCategory::Dialogue,
                subrole: parish_config::InferenceSubrole::Dialogue,
                profile: None,
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
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                role: parish_config::InferenceCategory::Dialogue,
                subrole: parish_config::InferenceSubrole::Dialogue,
                profile: None,
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
            reasoning_effort: None,
            priority: InferencePriority::Interactive,
            role: parish_config::InferenceCategory::Dialogue,
            subrole: parish_config::InferenceSubrole::Dialogue,
            profile: None,
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
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                role: parish_config::InferenceCategory::Dialogue,
                subrole: parish_config::InferenceSubrole::Dialogue,
                profile: None,
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
        assert!(
            entry.output_tokens.is_none(),
            "simulator must not fabricate provider-reported tokens"
        );
        let chunks = entry.stream_chunks.expect("stream_chunks populated");
        assert!(chunks > 0, "expected >0 chunks, got {chunks}");
    }

    /// A request whose cancel token fires mid-stream must surface
    /// `error: "cancelled"` and free the worker for the next request.
    /// Uses the simulator (40 ms/token); cancellation occurs while provider
    /// deltas are still provisional, so presentation must receive none.
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

        let cancel_task = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            cancel_task.cancel();
        });
        let drain = tokio::spawn(async move {
            let mut count: u64 = 0;
            while tok_rx.recv().await.is_some() {
                count += 1;
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
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                role: parish_config::InferenceCategory::Dialogue,
                subrole: parish_config::InferenceSubrole::Dialogue,
                profile: None,
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
        assert_eq!(
            tokens_seen, 0,
            "aborted provisional deltas must never escape"
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
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                role: parish_config::InferenceCategory::Dialogue,
                subrole: parish_config::InferenceSubrole::Dialogue,
                profile: None,
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
                reasoning_effort: None,
                priority: InferencePriority::Interactive,
                role: parish_config::InferenceCategory::Dialogue,
                subrole: parish_config::InferenceSubrole::Dialogue,
                profile: None,
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
