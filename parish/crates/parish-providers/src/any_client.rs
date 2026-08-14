//! `AnyClient` — unified provider dispatch, `InferenceClients` routing,
//! and the `build_client` factory.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use parish_config::{InferenceConfig, Provider};
use parish_types::ParishError;

use crate::anthropic_client::AnthropicClient;
use crate::google_client::{GenerationResult, GoogleClient, ProviderCallError, ProviderMetadata};
use crate::mock_client::MockClient;
use crate::openai_client::{GenerateParams, OpenAiClient, ResponseFormat};
use crate::simulator::SimulatorClient;

pub use crate::openai_client::{JsonSchemaSpec, ResponseFormat as AnyResponseFormat};

/// Buffer capacity for the bounded token streaming channel.
///
/// LLM providers produce tokens far faster than terminals or websocket
/// clients consume them, so a truly unbounded channel risks OOM on long
/// responses or slow consumers. 1 024 tokens is enough headroom for any
/// realistic burst; the sender blocks (back-pressure) if the consumer
/// falls further behind, which naturally throttles HTTP reads from the
/// provider. Fixes #83.
pub const TOKEN_CHANNEL_CAPACITY: usize = 1024;

/// Per-call streaming statistics observed by the worker as tokens flow
/// through the proxied channel.
///
/// `pub` so `parish_inference::worker` (which stays in the scheduling crate)
/// can construct it; the type itself is transport-side state.
#[derive(Debug, Clone)]
pub struct StreamStats {
    pub ttft: Option<Duration>,
    pub tokens: u64,
    pub partial_text: String,
}

/// Builds the right [`AnyClient`] variant for a given [`Provider`].
///
/// Every call site that currently does `OpenAiClient::new(url, key)` should
/// route through this helper instead so that
/// [`Provider::Anthropic`] is correctly dispatched to [`AnthropicClient`]
/// rather than silently misrouted through the OpenAI-compat client (which
/// would fail with a 404 because Anthropic's endpoint is `/v1/messages`,
/// not `/v1/chat/completions`).
///
/// The returned client is always unrate-limited; attach a limiter via
/// [`AnyClient::with_rate_limit`] (not implemented — do it on the inner
/// variant before wrapping) when per-provider throttling is required.
pub fn build_client(
    provider: &Provider,
    base_url: &str,
    api_key: Option<&str>,
    inference_config: &InferenceConfig,
) -> AnyClient {
    use parish_config::ProviderKind;
    match provider.kind() {
        ProviderKind::Anthropic => AnyClient::Anthropic(AnthropicClient::new_with_config(
            base_url,
            api_key,
            inference_config,
        )),
        ProviderKind::Google => AnyClient::Google(GoogleClient::new_with_config(
            base_url,
            api_key,
            inference_config,
        )),
        ProviderKind::Simulator => AnyClient::simulator(),
        // GitHub Models and Google's OpenAI-compat base URLs already include
        // their version prefix, so appending another `/v1` produces a 404.
        _ if matches!(provider.id(), "github_models" | "google") => AnyClient::OpenAi(
            OpenAiClient::new_with_config(base_url, api_key, inference_config)
                .with_completions_path("/chat/completions"),
        ),
        _ => AnyClient::OpenAi(OpenAiClient::new_with_config(
            base_url,
            api_key,
            inference_config,
        )),
    }
}

/// Per-category LLM client routing with a base provider fallback.
///
/// Each inference category (dialogue, simulation, intent) can have its own
/// provider, model, and endpoint. Categories without explicit overrides
/// fall back to the base provider.
#[derive(Clone)]
pub struct InferenceClients {
    /// Per-category (client, model) overrides.
    overrides: std::collections::HashMap<parish_config::InferenceCategory, (AnyClient, String)>,
    /// Base client used when no per-category override exists.
    pub base: AnyClient,
    /// Base model name (e.g. "gemma4:e4b").
    pub base_model: String,
}

impl InferenceClients {
    /// Creates a new `InferenceClients` with the given base client and per-category overrides.
    pub fn new(
        base: AnyClient,
        base_model: String,
        overrides: std::collections::HashMap<parish_config::InferenceCategory, (AnyClient, String)>,
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
    pub fn client_for(&self, category: parish_config::InferenceCategory) -> (&AnyClient, &str) {
        match self.overrides.get(&category) {
            Some((client, model)) => (client, model),
            None => (&self.base, &self.base_model),
        }
    }

    /// Returns the client and model to use for player dialogue (Tier 1).
    pub fn dialogue_client(&self) -> (&AnyClient, &str) {
        self.client_for(parish_config::InferenceCategory::Dialogue)
    }

    /// Returns the client and model to use for background NPC simulation (Tier 2).
    pub fn simulation_client(&self) -> (&AnyClient, &str) {
        self.client_for(parish_config::InferenceCategory::Simulation)
    }

    /// Returns the client and model to use for intent parsing.
    pub fn intent_client(&self) -> (&AnyClient, &str) {
        self.client_for(parish_config::InferenceCategory::Intent)
    }

    /// Returns the client and model to use for NPC arrival reactions.
    pub fn reaction_client(&self) -> (&AnyClient, &str) {
        self.client_for(parish_config::InferenceCategory::Reaction)
    }

    /// Whether the dialogue category uses a different provider than the base.
    pub fn has_custom_dialogue(&self) -> bool {
        self.overrides
            .contains_key(&parish_config::InferenceCategory::Dialogue)
    }
}

/// A unified client handle covering every supported provider transport.
///
/// - [`AnyClient::OpenAi`] wraps the OpenAI-compatible HTTP client used by
///   Ollama, LM Studio, OpenRouter, OpenAI, Google, Groq, xAI, Mistral,
///   DeepSeek, Together, NVIDIA NIM, vLLM, and any custom OpenAI-compatible
///   endpoint.
/// - [`AnyClient::Anthropic`] wraps [`AnthropicClient`], the native client
///   for Anthropic's Messages API (distinct schema, auth, and SSE events).
/// - [`AnyClient::Simulator`] is the built-in offline mock.
/// - [`AnyClient::Mock`] is a scriptable, deterministic test stand-in.
#[derive(Clone)]
pub enum AnyClient {
    /// A real OpenAI-compatible HTTP client.
    OpenAi(OpenAiClient),
    /// Anthropic's native Messages API client (see [`AnthropicClient`]).
    Anthropic(AnthropicClient),
    /// Google's native Gemini Interactions API client.
    Google(GoogleClient),
    /// The built-in offline simulator (generates funny nonsense locally).
    Simulator(Arc<SimulatorClient>),
    /// A scriptable mock returning caller-supplied completions verbatim.
    /// Test-only; see [`MockClient`]. Never opens a socket.
    Mock(Arc<MockClient>),
}

impl AnyClient {
    /// Wraps a real `OpenAiClient`.
    pub fn open_ai(client: OpenAiClient) -> Self {
        Self::OpenAi(client)
    }

    /// Wraps a real `AnthropicClient`.
    pub fn anthropic(client: AnthropicClient) -> Self {
        Self::Anthropic(client)
    }

    /// Wraps a native Google Interactions client.
    pub fn google(client: GoogleClient) -> Self {
        Self::Google(client)
    }

    /// Creates a new simulator client.
    pub fn simulator() -> Self {
        Self::Simulator(Arc::new(SimulatorClient::new()))
    }

    /// Creates an empty scriptable mock client, returning the shared
    /// [`MockClient`] handle so the test can enqueue completions.
    pub fn mock() -> (Self, Arc<MockClient>) {
        let mock = Arc::new(MockClient::new());
        (Self::Mock(mock.clone()), mock)
    }

    /// Generates plain text (non-streaming).
    ///
    /// `params.frequency_penalty` is the OpenAI-compat sampling knob; on
    /// `Anthropic` / `Simulator` it is accepted and ignored (no
    /// equivalent on the Messages API; the simulator is deterministic).
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => c.generate(model, prompt, system, params).await,
            Self::Google(c) => c.generate(model, prompt, system, params).await,
            Self::Anthropic(c) => {
                c.generate(model, prompt, system, params.max_tokens, params.temperature)
                    .await
            }
            Self::Simulator(c) => {
                c.generate(model, prompt, system, params.max_tokens, params.temperature)
                    .await
            }
            Self::Mock(c) => {
                c.generate(model, prompt, system, params.max_tokens, params.temperature)
                    .await
            }
        }
    }

    /// Generates text with token streaming.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_stream(model, prompt, system, token_tx, params)
                    .await
            }
            Self::Google(c) => {
                c.generate_stream(model, prompt, system, token_tx, params)
                    .await
            }
            Self::Anthropic(c) => {
                c.generate_stream(
                    model,
                    prompt,
                    system,
                    token_tx,
                    params.max_tokens,
                    params.temperature,
                )
                .await
            }
            Self::Simulator(c) => {
                c.generate_stream(
                    model,
                    prompt,
                    system,
                    token_tx,
                    params.max_tokens,
                    params.temperature,
                )
                .await
            }
            Self::Mock(c) => {
                c.generate_stream(
                    model,
                    prompt,
                    system,
                    token_tx,
                    params.max_tokens,
                    params.temperature,
                )
                .await
            }
        }
    }

    /// Streams text with JSON mode enabled.
    ///
    /// Like [`generate_stream`] but constrains the provider to emit valid JSON.
    /// Used for Tier 1 NPC responses where dialogue is embedded in a JSON
    /// structure and extracted incrementally during streaming.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_stream_json(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_stream_json(model, prompt, system, token_tx, params)
                    .await
            }
            Self::Google(c) => c
                .generate_stream_detailed_with_format(
                    model,
                    prompt,
                    system,
                    token_tx,
                    Some(ResponseFormat::JsonObject),
                    params,
                )
                .await
                .map(|result| result.text)
                .map_err(Into::into),
            Self::Anthropic(c) => {
                c.generate_stream_json(
                    model,
                    prompt,
                    system,
                    token_tx,
                    params.max_tokens,
                    params.temperature,
                )
                .await
            }
            Self::Simulator(c) => {
                c.generate_stream_json(
                    model,
                    prompt,
                    system,
                    token_tx,
                    params.max_tokens,
                    params.temperature,
                )
                .await
            }
            Self::Mock(c) => {
                c.generate_stream_json(
                    model,
                    prompt,
                    system,
                    token_tx,
                    params.max_tokens,
                    params.temperature,
                )
                .await
            }
        }
    }

    /// Generates a structured JSON response and deserializes it into `T`.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_json<T: serde::de::DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        params: GenerateParams,
    ) -> Result<T, ParishError> {
        match self {
            Self::OpenAi(c) => c.generate_json::<T>(model, prompt, system, params).await,
            Self::Google(c) => {
                let raw = c
                    .generate_detailed_with_format(
                        model,
                        prompt,
                        system,
                        Some(ResponseFormat::JsonObject),
                        params,
                    )
                    .await
                    .map_err(ParishError::from)?;
                serde_json::from_str(crate::strip_json_fence(&raw.text)).map_err(|error| {
                    ParishError::Inference(format!("invalid Google JSON: {error}"))
                })
            }
            Self::Anthropic(c) => {
                c.generate_json::<T>(model, prompt, system, params.max_tokens, params.temperature)
                    .await
            }
            Self::Simulator(c) => {
                c.generate_json::<T>(model, prompt, system, params.max_tokens, params.temperature)
                    .await
            }
            Self::Mock(c) => {
                c.generate_json::<T>(model, prompt, system, params.max_tokens, params.temperature)
                    .await
            }
        }
    }

    /// Non-streaming generate with an explicit OpenAI `response_format`.
    ///
    /// Returns the raw response text; callers parse JSON themselves. Used
    /// by the inference queue worker so [`InferenceRequest::json_schema`]
    /// can flow end-to-end without the worker having to know the target
    /// `T`. Anthropic and Simulator backends ignore `response_format`
    /// (they don't speak OpenAI's structured-outputs wire shape) and fall
    /// back to plain `generate`.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_text_with_format(model, prompt, system, response_format, params)
                    .await
            }
            Self::Google(c) => c
                .generate_detailed_with_format(model, prompt, system, response_format, params)
                .await
                .map(|result| result.text)
                .map_err(Into::into),
            Self::Anthropic(c) => {
                c.generate(model, prompt, system, params.max_tokens, params.temperature)
                    .await
            }
            Self::Simulator(c) => {
                if response_format.is_some() {
                    c.generate_json::<serde_json::Value>(
                        model,
                        prompt,
                        system,
                        params.max_tokens,
                        params.temperature,
                    )
                    .await
                    .and_then(|value| {
                        serde_json::to_string(&value).map_err(|error| {
                            ParishError::Inference(format!("simulator JSON encode failed: {error}"))
                        })
                    })
                } else {
                    c.generate(model, prompt, system, params.max_tokens, params.temperature)
                        .await
                }
            }
            Self::Mock(c) => {
                if response_format.is_some() {
                    // Preserve MockClient's typed-JSON semantics. In
                    // particular, input-parser calls synthesize deterministic
                    // intent JSON without consuming the next scripted NPC
                    // completion.
                    c.generate_json::<serde_json::Value>(
                        model,
                        prompt,
                        system,
                        params.max_tokens,
                        params.temperature,
                    )
                    .await
                    .and_then(|value| {
                        serde_json::to_string(&value).map_err(|error| {
                            ParishError::Inference(format!("mock JSON encode failed: {error}"))
                        })
                    })
                } else {
                    c.generate(model, prompt, system, params.max_tokens, params.temperature)
                        .await
                }
            }
        }
    }

    /// Streaming counterpart of [`generate_with_format`]. Used by the
    /// worker when a request carries a token sender *and* a response
    /// format (e.g. Tier 1 dialogue with a strict schema). Anthropic /
    /// Simulator ignore `response_format` and fall back to their plain
    /// streaming methods.
    /// Streaming generate with an explicit OpenAI `response_format`.
    ///
    /// Used by the inference queue worker when a request carries both a token
    /// sender and a response format (e.g. Tier 1 dialogue with a strict schema).
    /// Anthropic / Simulator ignore `response_format` and fall back to their
    /// plain streaming methods.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_stream_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        match self {
            Self::OpenAi(c) => {
                c.generate_stream_with_format(
                    model,
                    prompt,
                    system,
                    token_tx,
                    response_format,
                    params,
                )
                .await
            }
            Self::Google(c) => c
                .generate_stream_detailed_with_format(
                    model,
                    prompt,
                    system,
                    token_tx,
                    response_format,
                    params,
                )
                .await
                .map(|result| result.text)
                .map_err(Into::into),
            Self::Anthropic(c) => {
                c.generate_stream(
                    model,
                    prompt,
                    system,
                    token_tx,
                    params.max_tokens,
                    params.temperature,
                )
                .await
            }
            Self::Simulator(c) => {
                // When the caller expects JSON (either schema or json_mode),
                // the Markov stream the simulator would otherwise produce
                // never parses and the caller logs a JSON-parse error every
                // tick. Route those calls through the simulator's existing
                // JSON-stream path, which emits a generic JSON object whose
                // fields are wide enough for the Tier 2 / Tier 3 / reaction
                // structs (all use `#[serde(default)]`), so the worst case
                // is an "uneventful tick" rather than a hard parse failure.
                let wants_json = response_format.is_some()
                    || prompt.contains("Respond with a JSON")
                    || prompt.contains("Respond with JSON")
                    || prompt.contains("JSON object")
                    || prompt.contains("\"updates\":")
                    || prompt.contains("\"npc_id\":")
                    || system.is_some_and(|s| s.contains("JSON") || s.contains("input parser"));
                if wants_json {
                    c.generate_stream_json(
                        model,
                        prompt,
                        system,
                        token_tx,
                        params.max_tokens,
                        params.temperature,
                    )
                    .await
                } else {
                    c.generate_stream(
                        model,
                        prompt,
                        system,
                        token_tx,
                        params.max_tokens,
                        params.temperature,
                    )
                    .await
                }
            }
            Self::Mock(c) => {
                // Record whether this call asked the provider for a JSON
                // response format, so tests can assert JSON-mode activation per
                // attempt (e.g. parish-npc TD-033's Tier 2 retry).
                c.record_response_format(response_format.is_some());
                // Same JSON-vs-plain routing as the simulator arm: a scripted
                // dialogue completion is wrapped in the JSON envelope only when
                // the caller expects JSON, otherwise streamed as plain text.
                let wants_json = response_format.is_some()
                    || prompt.contains("Respond with a JSON")
                    || prompt.contains("Respond with JSON")
                    || prompt.contains("JSON object")
                    || prompt.contains("\"updates\":")
                    || prompt.contains("\"npc_id\":")
                    || system.is_some_and(|s| s.contains("JSON") || s.contains("input parser"));
                if wants_json {
                    c.generate_stream_json(
                        model,
                        prompt,
                        system,
                        token_tx,
                        params.max_tokens,
                        params.temperature,
                    )
                    .await
                } else {
                    c.generate_stream(
                        model,
                        prompt,
                        system,
                        token_tx,
                        params.max_tokens,
                        params.temperature,
                    )
                    .await
                }
            }
        }
    }

    /// Returns a reference to the inner `OpenAiClient`, if this is a real client.
    pub fn as_open_ai(&self) -> Option<&OpenAiClient> {
        match self {
            Self::OpenAi(c) => Some(c),
            Self::Anthropic(_) | Self::Google(_) | Self::Simulator(_) | Self::Mock(_) => None,
        }
    }

    /// Returns a reference to the inner `AnthropicClient`, if this is an Anthropic client.
    pub fn as_anthropic(&self) -> Option<&AnthropicClient> {
        match self {
            Self::Anthropic(c) => Some(c),
            Self::OpenAi(_) | Self::Google(_) | Self::Simulator(_) | Self::Mock(_) => None,
        }
    }

    /// Returns `true` if this is the offline simulator.
    pub fn is_simulator(&self) -> bool {
        matches!(self, Self::Simulator(_))
    }

    /// Whether this client uses Google's native Interactions transport.
    pub fn is_google(&self) -> bool {
        matches!(self, Self::Google(_))
    }

    /// Returns `true` if this is the scriptable test mock.
    pub fn is_mock(&self) -> bool {
        matches!(self, Self::Mock(_))
    }

    /// Returns `true` if the underlying client has a rate limiter attached.
    ///
    /// The simulator is always unlimited (no network calls), so this is
    /// `false` for `Self::Simulator`.
    pub fn has_rate_limiter(&self) -> bool {
        match self {
            Self::OpenAi(c) => c.has_rate_limiter(),
            Self::Anthropic(c) => c.has_rate_limiter(),
            Self::Google(c) => c.has_rate_limiter(),
            Self::Simulator(_) | Self::Mock(_) => false,
        }
    }

    /// Provider-neutral detailed non-streaming call used by inference audit.
    pub async fn generate_detailed_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<GenerationResult, ProviderCallError> {
        if let Self::Google(client) = self {
            return client
                .generate_detailed_with_format(model, prompt, system, response_format, params)
                .await;
        }
        let metadata = self.fallback_metadata(model);
        self.generate_with_format(model, prompt, system, response_format, params)
            .await
            .map(|text| GenerationResult {
                text,
                metadata: metadata.clone(),
            })
            .map_err(|error| ProviderCallError {
                message: error.to_string(),
                partial_text: String::new(),
                metadata: Box::new(metadata),
            })
    }

    /// Provider-neutral detailed streaming call used by inference audit.
    pub async fn generate_stream_detailed_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<GenerationResult, ProviderCallError> {
        if let Self::Google(client) = self {
            return client
                .generate_stream_detailed_with_format(
                    model,
                    prompt,
                    system,
                    token_tx,
                    response_format,
                    params,
                )
                .await;
        }
        let metadata = self.fallback_metadata(model);
        self.generate_stream_with_format(model, prompt, system, token_tx, response_format, params)
            .await
            .map(|text| GenerationResult {
                text,
                metadata: metadata.clone(),
            })
            .map_err(|error| ProviderCallError {
                message: error.to_string(),
                partial_text: String::new(),
                metadata: Box::new(metadata),
            })
    }

    /// Provider identity available before a request reaches a terminal event.
    /// Used to retain useful cancellation/timeout telemetry when a future is
    /// dropped before the wire response can return final usage.
    pub fn fallback_metadata(&self, model: &str) -> ProviderMetadata {
        let (provider, api_mode) = match self {
            Self::OpenAi(_) => ("openai-compatible", "openai-chat-completions"),
            Self::Anthropic(_) => ("anthropic", "anthropic-messages"),
            Self::Google(_) => ("google", "google-interactions-v1"),
            Self::Simulator(_) => ("simulator", "simulator"),
            Self::Mock(_) => ("mock", "mock"),
        };
        ProviderMetadata {
            provider: provider.to_string(),
            api_mode: api_mode.to_string(),
            model: model.to_string(),
            terminal_status: Some("completed".to_string()),
            ..ProviderMetadata::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_structured_intent_does_not_consume_scripted_dialogue() {
        let (client, mock) = AnyClient::mock();
        mock.push_any("Aye, I know Fr. Tierney well enough.");

        let intent = client
            .generate_with_format(
                "mock",
                "Do you know Father Declan Tierney?",
                Some("You are an input parser."),
                Some(ResponseFormat::JsonObject),
                GenerateParams::default(),
            )
            .await
            .expect("structured intent response");

        assert!(intent.contains("\"intent\""), "got: {intent}");
        assert_eq!(mock.pending(), 1, "intent must not consume NPC dialogue");
        let dialogue = client
            .generate(
                "mock",
                "answer the player",
                Some("You are Seamus Gallagher"),
                GenerateParams::default(),
            )
            .await
            .expect("scripted dialogue");
        assert!(dialogue.contains("know Fr. Tierney"));
    }

    #[test]
    fn test_inference_clients_dialogue_uses_override() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let cloud = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-test"),
        ));
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

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), HashMap::new());
        let (_client, model) = clients.dialogue_client();
        assert_eq!(model, "qwen3:14b");
        assert!(!clients.has_custom_dialogue());
    }

    #[test]
    fn test_inference_clients_simulation_falls_back_to_base() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let cloud = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-test"),
        ));
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

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let dial = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-dial"),
        ));
        let sim = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let intent = AnyClient::open_ai(OpenAiClient::new("http://localhost:1234", None));
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

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), HashMap::new());
        let (_client, model) = clients.intent_client();
        assert_eq!(model, "qwen3:14b");
    }

    #[test]
    fn test_inference_clients_reaction_falls_back_to_base() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let cloud = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-test"),
        ));
        let mut overrides = HashMap::new();
        overrides.insert(InferenceCategory::Dialogue, (cloud, "gpt-4".to_string()));
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), overrides);
        let (_client, model) = clients.reaction_client();
        assert_eq!(model, "qwen3:14b");
    }

    #[test]
    fn test_inference_clients_reaction_uses_override() {
        use parish_config::InferenceCategory;
        use std::collections::HashMap;

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let reaction = AnyClient::open_ai(OpenAiClient::new(
            "https://openrouter.ai/api",
            Some("sk-reaction"),
        ));
        let mut overrides = HashMap::new();
        overrides.insert(
            InferenceCategory::Reaction,
            (reaction, "claude-sonnet-4".to_string()),
        );
        let clients = InferenceClients::new(base, "qwen3:14b".to_string(), overrides);
        let (_client, model) = clients.reaction_client();
        assert_eq!(model, "claude-sonnet-4");
    }

    /// Regression — `AnyClient::Simulator::generate_stream_with_format` used
    /// to dispatch unconditionally to the Markov text stream, so any caller
    /// that expected a JSON shape (Tier 2/3 sim+reaction prompts, intent
    /// parser) saw a parse error every tick on the `small-only` loadout.
    /// Now: any JSON-shaped ask (response_format set, or a "JSON" /
    /// "input parser" / "Respond with a JSON" marker in the system or user
    /// prompt) streams a generic JSON object whose fields are wide enough
    /// for `Tier2Response`, `Tier3Update`, and the dialogue
    /// `NpcStreamResponse` (all `#[serde(default)]`), so the worst case is
    /// an "uneventful tick" rather than a parse failure.
    #[tokio::test]
    async fn simulator_streams_json_when_format_or_prompt_requests_it() {
        use crate::any_client::TOKEN_CHANNEL_CAPACITY;
        use tokio::sync::mpsc;

        let sim = AnyClient::simulator();

        // Helper: drive `generate_stream_with_format` and collect the chunks
        // the simulator pushed into `token_tx` (the streaming side) AND the
        // assembled return string (which is what the worker assembles for the
        // queue's response payload).
        async fn drive(
            client: &AnyClient,
            prompt: &str,
            system: Option<&str>,
            response_format: Option<ResponseFormat>,
        ) -> (Vec<String>, String) {
            let (tx, mut rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
            let stream_fut = client.generate_stream_with_format(
                "sim",
                prompt,
                system,
                tx,
                response_format,
                GenerateParams {
                    max_tokens: Some(120),
                    ..Default::default()
                },
            );
            let drain = tokio::spawn(async move {
                let mut chunks = Vec::new();
                while let Some(t) = rx.recv().await {
                    chunks.push(t);
                }
                chunks
            });
            let assembled = stream_fut.await.expect("stream future");
            let chunks = drain.await.unwrap_or_default();
            (chunks, assembled)
        }

        // Case 1 — response_format explicitly set: must stream JSON.
        let (_, body1) = drive(
            &sim,
            "fly some text",
            None,
            Some(ResponseFormat::JsonObject),
        )
        .await;
        assert!(
            body1.contains("\"dialogue\""),
            "explicit JsonObject should stream the dialogue-shaped JSON, got: {body1}"
        );

        // Case 2 — Tier 2 / Tier 3 sim prompts ask for JSON in the user
        // prompt body.  Marker is "Respond with a JSON" (Tier 2) or a bare
        // "JSON object" mention.
        let (_, body2) = drive(
            &sim,
            "You are simulating background interactions. … Respond with a JSON object …",
            None,
            None,
        )
        .await;
        assert!(
            body2.starts_with('{') && body2.contains("\"dialogue\""),
            "Tier 2 prompt should still stream JSON (no response_format set), got: {body2}"
        );

        // Case 3 — intent parser system prompt triggers JSON path even without
        // a response_format hint.
        let (_, body3) = drive(
            &sim,
            "go to the pub",
            Some("You are a text adventure input parser. …"),
            None,
        )
        .await;
        assert!(
            body3.starts_with('{'),
            "intent-parser system prompt should stream JSON, got: {body3}"
        );

        // Case 4 — Tier 3 prompt (build_tier3_prompt) uses "Respond with
        // JSON" (no "a") and embeds a `{"updates":[…]}` schema. Regression:
        // before the "Respond with JSON" + `"updates":` markers landed, this
        // fell through to Markov and the Tier 3 batch parser logged a parse
        // failure on every world-tick post-boot.
        let (_, body_t3) = drive(
            &sim,
            r#"You are simulating background NPC activity. Respond with JSON, using the bracketed ids: {"updates":[{"npc_id":1,"mood":"…"}]}"#,
            None,
            None,
        )
        .await;
        assert!(
            body_t3.starts_with('{'),
            "Tier 3 prompt must stream JSON (regression — boot-time parse storm), got: {body_t3}"
        );

        // Case 5 — plain dialogue with no JSON ask: legacy Markov text path.
        let (_, body5) = drive(&sim, "Tell me a story.", None, None).await;
        assert!(
            !body5.starts_with('{'),
            "plain prompt should still produce text, got: {body5}"
        );
    }
}
