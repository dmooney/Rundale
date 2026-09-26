//! The turn inference seam.
//!
//! Every model call made while a player turn runs (intent classification,
//! Tier-1 dialogue, travel-encounter enrichment, arrival reactions) goes
//! through [`TurnInference`]. The shared game loop builds an
//! [`InferenceCall`] and interprets the [`InferenceOutcome`]; the runtime
//! decides how the call is fulfilled. Desktop fulfils it in-process with the
//! configured provider clients (`game_loop::inference::InProcessInference`);
//! an embedded host can fulfil it through its own transport.
//!
//! Responsibility split (Rules 33 and 37): the host owns model selection,
//! generation settings, transport, timeouts, termination checks, and audit
//! records. The engine owns prompts, semantic validation, and every state
//! change. A host never applies semantic guards or publishes candidate text.
//!
//! See `docs/design/portable-turn-api.md` §4.3.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::config::{InferenceProfile, InferenceSubrole, ReasoningEffort};
use crate::inference::{
    AnyClient, DirectInferenceAudit, GenerateParams, InferenceAuditSink, InferencePriority,
    ProviderCallError, ProviderMetadata, ResponseFormat,
};

/// Boxed future returned by [`TurnInference`] methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The reply shape the engine expects from a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseShape {
    /// Free text (encounters, reactions).
    Text,
    /// A JSON Intent classification. Hosts check that it parses before
    /// reporting success.
    IntentJson,
    /// The Tier-1 NPC dialogue envelope. Parsed leniently by the canonical
    /// apply seam, so hosts do not reject it structurally.
    NpcDialogue,
}

/// One model request from the engine.
#[derive(Debug, Clone)]
pub struct InferenceCall {
    /// Which workload this is; the host maps it to a route and settings.
    pub subrole: InferenceSubrole,
    /// System instructions, when the workload has them.
    pub system: Option<String>,
    /// The rendered prompt.
    pub prompt: String,
    /// Expected reply shape.
    pub response: ResponseShape,
    /// Engine correlation id (dialogue turn id), used for queue and audit
    /// correlation.
    pub correlation_id: Option<u64>,
}

/// Why a call produced no usable reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceFailureKind {
    /// The request could not be delivered or the provider returned an error.
    Transport,
    /// The reply did not have the expected structure.
    Protocol,
    /// The call exceeded its time budget.
    TimedOut,
    /// The reply channel closed before a terminal reply.
    Interrupted,
}

/// Generation settings the host used, reported back for telemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationSettings {
    /// Output token cap.
    pub max_tokens: u32,
    /// Sampling temperature.
    pub temperature: f32,
    /// Frequency penalty, when set.
    pub frequency_penalty: Option<f32>,
    /// Whether JSON mode was requested from the provider.
    pub json_mode: bool,
    /// Thinking toggle, when set.
    pub enable_thinking: Option<bool>,
    /// Reasoning effort, when set.
    pub reasoning_effort: Option<ReasoningEffort>,
}

/// What the host resolved and observed for one call.
#[derive(Debug, Clone, Default)]
pub struct CallReport {
    /// Model identifier the host invoked.
    pub model: String,
    /// Output token cap the host requested.
    pub max_tokens: Option<u32>,
    /// Generation settings, for workloads that report them (dialogue).
    pub generation: Option<GenerationSettings>,
    /// Provider transport metadata, when the host has it.
    pub metadata: Option<ProviderMetadata>,
    /// Length of any partial output discarded with a failed call.
    pub partial_output_len: usize,
}

/// The host's answer to one [`InferenceCall`].
#[derive(Debug, Clone)]
pub enum InferenceOutcome {
    /// A terminal reply with a success finish reason.
    Completed {
        /// The reply text.
        text: String,
        /// Host report.
        report: CallReport,
    },
    /// No usable reply.
    Failed {
        /// Failure category.
        kind: InferenceFailureKind,
        /// Diagnostic message (never shown to the player).
        message: String,
        /// Report, including any partial output length in metadata.
        report: CallReport,
    },
}

impl InferenceOutcome {
    /// The reply text when the call completed.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Completed { text, .. } => Some(text),
            Self::Failed { .. } => None,
        }
    }

    /// The host report, for either outcome.
    pub fn report(&self) -> &CallReport {
        match self {
            Self::Completed { report, .. } | Self::Failed { report, .. } => report,
        }
    }
}

/// Whether a workload has a route, before any call is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteStatus {
    /// No route is configured; the engine uses its offline fallback.
    Unavailable,
    /// A route exists but produces placeholder text (the simulator); free-text
    /// workloads use canned lines instead.
    Simulated,
    /// A real model route.
    Live,
}

/// Fulfils the engine's model calls for one runtime.
pub trait TurnInference: Send + Sync {
    /// Reports whether `subrole` has a route.
    fn route(&self, subrole: InferenceSubrole) -> BoxFuture<'_, RouteStatus>;

    /// Performs one call. When `tokens` is set, the host forwards reply text
    /// as it arrives (a host without streaming sends the whole reply once);
    /// the channel is closed when the call ends.
    fn complete_streaming(
        &self,
        call: InferenceCall,
        tokens: Option<mpsc::Sender<String>>,
    ) -> BoxFuture<'_, InferenceOutcome>;

    /// Performs one call without streaming.
    fn complete(&self, call: InferenceCall) -> BoxFuture<'_, InferenceOutcome> {
        self.complete_streaming(call, None)
    }
}

/// Fulfils calls with one directly held provider client.
///
/// This is the in-process path for intent, travel encounters, and arrival
/// reactions: one provider request, an optional time budget, and one audit
/// record, exactly as those workloads have always been called.
pub struct DirectClientInference {
    client: Option<AnyClient>,
    model: String,
    profile: InferenceProfile,
    audit_sink: Option<InferenceAuditSink>,
    timeout: Option<Duration>,
}

impl DirectClientInference {
    /// Wraps `client` (or no client) with the model and profile to use.
    pub fn new(
        client: Option<AnyClient>,
        model: impl Into<String>,
        profile: InferenceProfile,
        audit_sink: Option<InferenceAuditSink>,
    ) -> Self {
        Self {
            client,
            model: model.into(),
            profile,
            audit_sink,
            timeout: None,
        }
    }

    /// Bounds each call; on expiry the call fails with
    /// `"<workload> inference timed out after <n>s"`.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn params(&self, subrole: InferenceSubrole) -> GenerateParams {
        let profile = &self.profile;
        match subrole {
            InferenceSubrole::Intent => GenerateParams {
                max_tokens: Some(profile.max_output_tokens),
                thinking_level: Some(profile.thinking_level),
                service_tier: Some(profile.service_tier),
                ..GenerateParams::default()
            },
            _ => GenerateParams {
                max_tokens: Some(profile.max_output_tokens),
                temperature: None,
                frequency_penalty: None,
                enable_thinking: None,
                reasoning_effort: None,
                thinking_level: Some(profile.thinking_level),
                service_tier: Some(profile.service_tier),
                reasoning_intent: (profile.configuration_epoch > 0)
                    .then_some(profile.reasoning_intent),
                reasoning_dialect: profile.reasoning_dialect,
            },
        }
    }
}

fn timeout_label(subrole: InferenceSubrole) -> &'static str {
    match subrole {
        InferenceSubrole::TravelEncounter => "travel encounter",
        InferenceSubrole::ArrivalReaction | InferenceSubrole::MessageReaction => "reaction",
        _ => "provider",
    }
}

fn failure_kind(message: &str, response: ResponseShape) -> InferenceFailureKind {
    if message.contains("timed out") {
        InferenceFailureKind::TimedOut
    } else if response == ResponseShape::IntentJson && message.contains("JSON parse failed") {
        InferenceFailureKind::Protocol
    } else {
        InferenceFailureKind::Transport
    }
}

impl TurnInference for DirectClientInference {
    fn route(&self, _subrole: InferenceSubrole) -> BoxFuture<'_, RouteStatus> {
        let status = match &self.client {
            None => RouteStatus::Unavailable,
            Some(client) if client.is_simulator() => RouteStatus::Simulated,
            Some(_) => RouteStatus::Live,
        };
        Box::pin(async move { status })
    }

    fn complete_streaming(
        &self,
        call: InferenceCall,
        tokens: Option<mpsc::Sender<String>>,
    ) -> BoxFuture<'_, InferenceOutcome> {
        Box::pin(async move {
            let params = self.params(call.subrole);
            let mut report = CallReport {
                model: self.model.clone(),
                max_tokens: params.max_tokens,
                generation: None,
                metadata: None,
                partial_output_len: 0,
            };
            let Some(client) = self.client.as_ref() else {
                return InferenceOutcome::Failed {
                    kind: InferenceFailureKind::Transport,
                    message: "no inference route is configured".to_string(),
                    report,
                };
            };
            let response_format =
                (call.response == ResponseShape::IntentJson).then_some(ResponseFormat::JsonObject);
            let audit = DirectInferenceAudit::new(
                self.audit_sink.clone(),
                &self.model,
                &call.prompt,
                call.system.as_deref(),
                call.subrole,
                tokens.is_some(),
                params.max_tokens,
                params.thinking_level,
                params.service_tier,
                params.temperature,
                InferencePriority::Interactive,
            );
            let request = async {
                match tokens {
                    Some(tx) => {
                        client
                            .generate_stream_detailed_with_format(
                                &self.model,
                                &call.prompt,
                                call.system.as_deref(),
                                tx,
                                response_format,
                                params,
                            )
                            .await
                    }
                    None => {
                        client
                            .generate_detailed_with_format(
                                &self.model,
                                &call.prompt,
                                call.system.as_deref(),
                                response_format,
                                params,
                            )
                            .await
                    }
                }
            };
            let detailed = match self.timeout {
                Some(limit) => match tokio::time::timeout(limit, request).await {
                    Ok(result) => result,
                    Err(_) => Err(ProviderCallError {
                        message: format!(
                            "{} inference timed out after {}s",
                            timeout_label(call.subrole),
                            limit.as_secs()
                        ),
                        partial_text: String::new(),
                        metadata: Box::new(ProviderMetadata::unavailable(&self.model)),
                    }),
                },
                None => request.await,
            };
            let checked = match (call.response, detailed) {
                (ResponseShape::IntentJson, Ok(result)) => {
                    match limerick_input::intent_reply_parse_error(&result.text) {
                        None => Ok(result),
                        Some(message) => Err(ProviderCallError {
                            message,
                            partial_text: result.text,
                            metadata: Box::new(result.metadata),
                        }),
                    }
                }
                (_, other) => other,
            };
            match audit.record(checked).await {
                Ok(result) => {
                    report.metadata = Some(result.metadata);
                    InferenceOutcome::Completed {
                        text: result.text,
                        report,
                    }
                }
                Err(error) => {
                    report.metadata = Some(*error.metadata);
                    report.partial_output_len = error.partial_text.len();
                    InferenceOutcome::Failed {
                        kind: failure_kind(&error.message, call.response),
                        message: error.message,
                        report,
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InferenceProfile;

    fn intent_call(prompt: &str) -> InferenceCall {
        InferenceCall {
            subrole: InferenceSubrole::Intent,
            system: Some(limerick_input::intent_system_prompt().to_string()),
            prompt: prompt.to_string(),
            response: ResponseShape::IntentJson,
            correlation_id: None,
        }
    }

    /// An OpenAI-compatible client pointed at a wiremock server that returns
    /// `reply` for every chat completion.
    async fn http_client(reply: &str) -> (AnyClient, wiremock::MockServer) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "c",
                "object": "chat.completion",
                "model": "test-model",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": reply},
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;
        let provider = crate::config::Provider::from_str_loose("lmstudio").expect("lmstudio");
        let client = crate::inference::build_client(
            &provider,
            &format!("{}/v1", server.uri()),
            None,
            &crate::config::InferenceConfig::default(),
        );
        (client, server)
    }

    fn direct(client: Option<AnyClient>) -> DirectClientInference {
        DirectClientInference::new(
            client,
            "test-model",
            InferenceProfile::for_subrole(InferenceSubrole::Intent),
            None,
        )
    }

    #[tokio::test]
    async fn route_reports_unavailable_simulated_and_live() {
        assert_eq!(
            direct(None).route(InferenceSubrole::Intent).await,
            RouteStatus::Unavailable
        );
        assert_eq!(
            direct(Some(AnyClient::simulator()))
                .route(InferenceSubrole::ArrivalReaction)
                .await,
            RouteStatus::Simulated
        );
        let (mock, _) = AnyClient::mock();
        assert_eq!(
            direct(Some(mock)).route(InferenceSubrole::Intent).await,
            RouteStatus::Live
        );
    }

    #[tokio::test]
    async fn malformed_intent_reply_is_a_protocol_failure_with_the_desktop_message() {
        let (client, _server) = http_client("this is not json").await;
        let outcome = direct(Some(client))
            .complete(intent_call("stroll yonder"))
            .await;
        match outcome {
            InferenceOutcome::Failed {
                kind,
                message,
                report,
            } => {
                assert_eq!(kind, InferenceFailureKind::Protocol);
                assert!(
                    message.starts_with("intent JSON parse failed:"),
                    "{message}"
                );
                assert_eq!(report.model, "test-model");
            }
            other => panic!("expected a protocol failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn well_formed_intent_reply_completes_with_the_raw_text() {
        let reply = r#"{"intent":"move","target":"the pub","dialogue":null}"#;
        let (client, _server) = http_client(reply).await;
        let outcome = direct(Some(client))
            .complete(intent_call("stroll yonder to the pub"))
            .await;
        assert_eq!(outcome.text(), Some(reply));
        assert_eq!(
            outcome.report().max_tokens,
            Some(InferenceProfile::for_subrole(InferenceSubrole::Intent).max_output_tokens)
        );
    }

    #[tokio::test]
    async fn missing_client_fails_without_a_provider_call() {
        let outcome = direct(None).complete(intent_call("hello")).await;
        assert!(matches!(
            outcome,
            InferenceOutcome::Failed {
                kind: InferenceFailureKind::Transport,
                ..
            }
        ));
    }
}
