//! Shared inference-rebuild helper (#696).
//!
//! Extracts the common "abort old worker, build new client, spawn new worker,
//! install new queue" logic that was previously duplicated across
//! `limerick-server/src/routes.rs` and `limerick-tauri/src/commands.rs`.
//!
//! # Usage
//!
//! Each runtime calls [`rebuild_inference_worker`] to handle the mechanical
//! worker lifecycle, then handles the backend-specific side effects itself:
//!
//! - **`limerick-server`**: additionally emits a URL warning via the event bus.
//! - **`limerick-tauri`**: emits a URL warning via `app.emit`.
//! - **`limerick-engine`**: continues to use its own inline implementation (the
//!   headless `App` struct is not yet on `Arc<Mutex<T>>`; deferred to a future
//!   slice — see module-level comment in `game_loop/mod.rs`).
//!
//! # Architecture gate
//!
//! This module is backend-agnostic — it imports only `limerick-inference` types
//! and `InferenceConfig`.  It must not import `axum`, `tauri`, or any crate
//! in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::config::InferenceConfig;
use crate::inference::file_log::InferenceFileLog;
use crate::inference::{
    AnyClient, InferenceLog, InferenceQueue, InferenceWorkerConfig, build_client,
};

/// The three AppState mutex slots that [`rebuild_inference_worker`] needs.
///
/// Grouping them into a single struct keeps the function signature within
/// Clippy's `too-many-arguments` limit (≤ 7).
pub struct InferenceSlots<'a> {
    /// `AppState.client` — updated to the new `AnyClient` (skipped for simulator).
    pub client: &'a Mutex<Option<AnyClient>>,
    /// `AppState.worker_handle` — old task aborted, new task stored.
    pub worker_handle: &'a Mutex<Option<JoinHandle<()>>>,
    /// `AppState.inference_queue` — replaced with the new queue.
    pub inference_queue: &'a Mutex<Option<InferenceQueue>>,
}

/// Builds a fresh inference client and worker, aborting the previous worker,
/// and atomically installs both into the caller's mutex slots.
///
/// Returns `(new_client, url_warning)`.
///
/// - `new_client` is the freshly built `AnyClient` so callers can update any
///   additional slots as needed (e.g. the server's `cloud_client` slot).
/// - `url_warning` is `Some(message)` when the base URL looks malformed
///   (doesn't start with `http://` or `https://`).  Callers are responsible
///   for surfacing this to the player via their runtime's emit path.
///
/// The new worker and queue are installed into [`InferenceSlots`] before this
/// function returns.
///
/// # Lock ordering
///
/// Acquires `slots.client`, then `slots.worker_handle`, then
/// `slots.inference_queue` — callers must not hold any of these locks when
/// calling this function to avoid deadlock.
///
/// # Parameters
///
/// - `provider_name` / `base_url` / `api_key`: values read from `GameConfig`
///   (caller must drop the config lock before calling).
/// - `inference_config`: TOML-configured timeouts; not mutated.
/// - `inference_log`: shared log ring-buffer (cheap `Arc` clone).
/// - `slots`: the three AppState mutex fields used for worker lifecycle.
pub async fn rebuild_inference_worker(
    provider_name: &str,
    base_url: &str,
    api_key: Option<&str>,
    inference_config: &InferenceConfig,
    inference_log: InferenceLog,
    inference_file_log: InferenceFileLog,
    slots: InferenceSlots<'_>,
) -> (AnyClient, Option<String>) {
    // Check URL validity; callers will surface the warning.
    let url_warning = if provider_name != "simulator"
        && !(base_url.starts_with("http://") || base_url.starts_with("https://"))
    {
        Some(format!(
            "Warning: '{}' doesn't look like a valid URL — NPC conversations may fail.",
            base_url
        ))
    } else {
        None
    };

    let provider_enum = crate::config::Provider::from_str_loose(provider_name).unwrap_or_default();

    // Build the new AnyClient and update the raw client slot.
    let any_client = if provider_name == "simulator" {
        AnyClient::simulator()
    } else {
        let built = build_client(&provider_enum, base_url, api_key, inference_config);
        {
            let mut guard = slots.client.lock().await;
            *guard = Some(built.clone());
        }
        built
    };

    rebuild_inference_worker_with_client(
        any_client.clone(),
        provider_enum,
        inference_config,
        inference_log,
        inference_file_log,
        slots,
    )
    .await;

    (any_client, url_warning)
}

/// Publishes an already constructed v2 transport into the worker lifecycle.
/// This keeps adapter selection at the resolved-route seam instead of
/// reconstructing a client from legacy provider-name heuristics.
pub async fn rebuild_inference_worker_with_client(
    any_client: AnyClient,
    provider: crate::config::Provider,
    inference_config: &InferenceConfig,
    inference_log: InferenceLog,
    inference_file_log: InferenceFileLog,
    slots: InferenceSlots<'_>,
) {
    let clients = crate::inference::InferenceClients::new(
        any_client.clone(),
        String::new(),
        Default::default(),
    );
    rebuild_inference_worker_with_clients(
        clients,
        any_client,
        provider,
        inference_config,
        inference_log,
        inference_file_log,
        slots,
    )
    .await;
}

pub async fn rebuild_inference_worker_with_clients(
    clients: crate::inference::InferenceClients,
    dialogue_client: AnyClient,
    provider: crate::config::Provider,
    inference_config: &InferenceConfig,
    inference_log: InferenceLog,
    inference_file_log: InferenceFileLog,
    slots: InferenceSlots<'_>,
) {
    // Construct the complete replacement before touching the live queue.
    let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
    let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
    let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
    let worker = crate::inference::spawn_inference_worker_with_clients(
        clients,
        InferenceWorkerConfig {
            interactive_rx,
            background_rx,
            batch_rx,
            log: inference_log.clone(),
            file_log: inference_file_log.clone(),
            provider,
            timeout_config: inference_config.clone(),
        },
    );
    let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx).with_audit_sink(
        crate::inference::InferenceAuditSink::new(inference_log, inference_file_log),
    );

    // Publish admission to the new immutable client set. Dropping the old
    // queue sender lets its detached worker drain already-admitted requests
    // on their captured epoch instead of cancelling them during reload.
    *slots.inference_queue.lock().await = Some(queue);
    *slots.client.lock().await = Some(dialogue_client);
    let old_worker = slots.worker_handle.lock().await.replace(worker);
    drop(old_worker);
}

/// Desktop fulfilment of the [`crate::turn_inference::TurnInference`] seam.
///
/// Resolves each workload from the live runtime configuration at call time,
/// exactly as the game loop used to inline: Tier-1 dialogue goes through the
/// interactive [`InferenceQueue`] (tokens drained and discarded, response
/// timeout behind the `inference-response-timeout` kill switch), and intent,
/// travel encounters, and arrival reactions call their category client
/// directly with an audit record.
#[derive(Clone)]
pub struct InProcessInference<'a> {
    config: &'a Mutex<crate::ipc::GameConfig>,
    client: &'a Mutex<Option<AnyClient>>,
    inference_queue: &'a Mutex<Option<InferenceQueue>>,
    inference_config: &'a InferenceConfig,
    deferred_audit: Option<crate::inference::DeferredInferenceAudit>,
}

impl<'a> InProcessInference<'a> {
    /// Borrows the runtime slots from a game-loop context.
    pub fn from_ctx(ctx: &crate::game_loop::GameLoopContext<'a>) -> Self {
        Self {
            config: ctx.config,
            client: ctx.client,
            inference_queue: ctx.inference_queue,
            inference_config: ctx.inference_config,
            deferred_audit: None,
        }
    }

    /// Buffers every audit record in `audit` until the caller commits or
    /// discards it, so a staged turn reveals its provider calls only when it
    /// commits.
    pub fn with_deferred_audit(&self, audit: crate::inference::DeferredInferenceAudit) -> Self {
        Self {
            deferred_audit: Some(audit),
            ..self.clone()
        }
    }

    /// The live queue, scoped to this adapter's deferred audit when set.
    async fn queue(&self) -> Option<InferenceQueue> {
        let queue = self.inference_queue.lock().await.clone()?;
        Some(match &self.deferred_audit {
            Some(audit) => queue.with_deferred_audit(audit.clone()),
            None => queue,
        })
    }

    /// Resolves the direct client route for a non-dialogue workload.
    async fn direct(
        &self,
        subrole: limerick_config::InferenceSubrole,
    ) -> crate::turn_inference::DirectClientInference {
        let (client, model, profile) = {
            let config = self.config.lock().await;
            let base_client = self.client.lock().await;
            let (client, model) =
                config.resolve_category_client(subrole.category(), base_client.as_ref());
            (client, model, config.inference_profile(subrole))
        };
        let audit_sink = self
            .queue()
            .await
            .as_ref()
            .and_then(InferenceQueue::audit_sink);
        let direct =
            crate::turn_inference::DirectClientInference::new(client, model, profile, audit_sink);
        match subrole {
            limerick_config::InferenceSubrole::TravelEncounter => direct.with_timeout(
                std::time::Duration::from_secs(TRAVEL_ENCOUNTER_TIMEOUT_SECS),
            ),
            limerick_config::InferenceSubrole::ArrivalReaction => {
                direct.with_timeout(std::time::Duration::from_secs(
                    crate::config::ReactionConfig::default().llm_timeout_secs,
                ))
            }
            _ => direct,
        }
    }

    async fn complete_dialogue(
        &self,
        call: crate::turn_inference::InferenceCall,
        tokens: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> crate::turn_inference::InferenceOutcome {
        use crate::inference::{
            INFERENCE_RESPONSE_TIMEOUT_SECS, InferenceAwaitOutcome, InferencePriority,
            QueueRequest, await_inference_response,
        };
        use crate::turn_inference::{
            CallReport, GenerationSettings, InferenceFailureKind, InferenceOutcome,
        };

        let (queue, model, profile, timeout_secs) = {
            let queue = self.queue().await;
            let config = self.config.lock().await;
            let timeout_secs = if config.flags.is_disabled("inference-response-timeout") {
                None
            } else {
                Some(INFERENCE_RESPONSE_TIMEOUT_SECS)
            };
            (
                queue,
                config.model_name.clone(),
                config.inference_profile(call.subrole),
                timeout_secs,
            )
        };
        // The defaults preserve the measured Qwen2.5-14B-4bit workaround:
        // frequency_penalty=0.5 suppresses verbatim repetition loops. Keeping the
        // values in engine config lets each promoted model/backend profile carry
        // the exact sampling parameters that passed its evidence gate.
        let generation = self.inference_config.dialogue_generation.for_model(&model);
        let report = CallReport {
            model: model.clone(),
            max_tokens: Some(generation.max_tokens),
            generation: Some(GenerationSettings {
                max_tokens: generation.max_tokens,
                temperature: generation.temperature,
                frequency_penalty: generation.frequency_penalty,
                json_mode: generation.json_mode,
                enable_thinking: generation.enable_thinking,
                reasoning_effort: generation.reasoning_effort,
            }),
            metadata: None,
            partial_output_len: 0,
        };
        let failed = |kind, message: String| InferenceOutcome::Failed {
            kind,
            message,
            report: report.clone(),
        };
        let Some(queue) = queue else {
            return failed(
                InferenceFailureKind::Transport,
                "no inference queue is configured".to_string(),
            );
        };
        tracing::debug!(
            model,
            max_tokens = generation.max_tokens,
            temperature = generation.temperature,
            frequency_penalty = generation.frequency_penalty,
            json_mode = generation.json_mode,
            enable_thinking = generation.enable_thinking,
            reasoning_effort = ?generation.reasoning_effort,
            "submitting Tier-1 dialogue generation profile"
        );
        let req_id = call.correlation_id.unwrap_or_default();
        let (token_tx, token_rx) =
            tokio::sync::mpsc::channel::<String>(crate::ipc::TOKEN_CHANNEL_CAPACITY);
        let send_result = queue
            .send(QueueRequest {
                id: req_id,
                model: model.clone(),
                prompt: call.prompt,
                system: call.system,
                token_tx: Some(token_tx),
                max_tokens: Some(generation.max_tokens),
                temperature: Some(generation.temperature),
                frequency_penalty: generation.frequency_penalty,
                enable_thinking: generation.enable_thinking,
                reasoning_effort: generation.reasoning_effort,
                priority: InferencePriority::Interactive,
                role: limerick_config::InferenceCategory::Dialogue,
                subrole: call.subrole,
                profile: Some(profile),
                json_mode: generation.json_mode,
                json_schema: None,
                cancel: None,
            })
            .await;
        let response_rx = match send_result {
            Ok(rx) => rx,
            Err(e) => {
                tracing::error!("Failed to submit inference request: {}", e);
                return failed(InferenceFailureKind::Transport, e.to_string());
            }
        };

        // Drain provider tokens for transport backpressure. The engine
        // quarantines candidate text (#1834), so tokens are forwarded only
        // when the caller asked for them.
        let stream_handle = tokio::spawn(async move {
            let mut token_rx = token_rx;
            while let Some(token) = token_rx.recv().await {
                if let Some(tx) = &tokens {
                    let _ = tx.send(token).await;
                }
            }
        });
        let outcome = await_inference_response(
            response_rx,
            timeout_secs.map(std::time::Duration::from_secs),
        )
        .await;
        if matches!(&outcome, InferenceAwaitOutcome::Response(_)) {
            let _ = stream_handle.await;
        } else {
            stream_handle.abort();
        }
        match outcome {
            InferenceAwaitOutcome::Response(response) => match response.error {
                Some(error) => {
                    tracing::warn!("Inference error: {:?}", Some(&error));
                    failed(InferenceFailureKind::Transport, error)
                }
                None => InferenceOutcome::Completed {
                    text: response.text,
                    report,
                },
            },
            InferenceAwaitOutcome::Closed => {
                tracing::warn!(
                    req_id,
                    "NPC inference response channel closed without a reply"
                );
                failed(
                    InferenceFailureKind::Interrupted,
                    "response channel closed".to_string(),
                )
            }
            InferenceAwaitOutcome::TimedOut { secs } => {
                tracing::warn!(req_id, secs, "NPC inference response timed out");
                failed(
                    InferenceFailureKind::TimedOut,
                    format!("dialogue inference timed out after {secs}s"),
                )
            }
        }
    }
}

/// Time budget for LLM travel-encounter enrichment; the canned line is used
/// when it expires.
pub const TRAVEL_ENCOUNTER_TIMEOUT_SECS: u64 = 15;

impl crate::turn_inference::TurnInference for InProcessInference<'_> {
    fn route(
        &self,
        subrole: limerick_config::InferenceSubrole,
    ) -> crate::turn_inference::BoxFuture<'_, crate::turn_inference::RouteStatus> {
        Box::pin(async move {
            if subrole == limerick_config::InferenceSubrole::Dialogue {
                return if self.inference_queue.lock().await.is_some() {
                    crate::turn_inference::RouteStatus::Live
                } else {
                    crate::turn_inference::RouteStatus::Unavailable
                };
            }
            self.direct(subrole).await.route(subrole).await
        })
    }

    fn complete_streaming(
        &self,
        call: crate::turn_inference::InferenceCall,
        tokens: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> crate::turn_inference::BoxFuture<'_, crate::turn_inference::InferenceOutcome> {
        Box::pin(async move {
            if call.subrole == limerick_config::InferenceSubrole::Dialogue {
                return self.complete_dialogue(call, tokens).await;
            }
            let direct = self.direct(call.subrole).await;
            direct.complete_streaming(call, tokens).await
        })
    }
}
