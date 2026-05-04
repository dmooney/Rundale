//! `InferenceClient` trait, request/response envelope, LRU response cache,
//! and structured cost metrics.
//!
//! # Design
//!
//! - [`InferenceClient`] is a lean async trait covering **non-streaming**
//!   completions.  Streaming is intentionally excluded — streamed responses
//!   are inherently single-use and cannot be cached.  Streaming stays on
//!   [`crate::AnyClient`] until a separate streaming-trait PR lands.
//!
//! - [`InferenceRequest`] is the call envelope.  It is distinct from the
//!   existing [`crate::InferenceRequest`] (the queue/worker envelope) which
//!   remains unchanged.  The new envelope carries metadata needed for caching
//!   and metrics: `request_id`, `session_id`, `account_id`, `prompt_hash`,
//!   `priority`, `params`, and `messages`.
//!
//! - [`CachingInferenceClient`] is an LRU decorator keyed by
//!   `(prompt_hash, model, params)`.  It wraps any [`InferenceClient`] impl
//!   and is disabled entirely when the `inference-response-cache` feature
//!   flag is off — no wrapper is constructed and there is no per-call
//!   overhead.
//!
//! - Structured cost metrics are emitted on every completed call via
//!   `tracing::info!` with the standard fields established by PR #888:
//!   `request_id`, `session_id`, `account_id`, `model`, `latency_ms`,
//!   `cache_hit`, plus usage counters `tokens_in` / `tokens_out`.
//!
//! # Feature flags
//!
//! | Flag                         | Default | Behaviour when off                                    |
//! |------------------------------|---------|-------------------------------------------------------|
//! | `inference-client-trait`     | on      | fall back to direct `AnyClient` call-site path        |
//! | `inference-response-cache`   | on      | `CachingInferenceClient` wrapper not constructed      |
//!
//! Both flags default-on per CLAUDE.md §6.  When `inference-response-cache`
//! is off, `cache_hit` is always `false` in the metrics event.
//!
//! # Default cache capacity
//!
//! 500 entries.  Override via the `PARISH_INFERENCE_CACHE_CAPACITY`
//! environment variable (parsed as `usize` at startup).

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use lru::LruCache;
use tokio::sync::Mutex;
use uuid::Uuid;

use parish_types::ParishError;

// ---------------------------------------------------------------------------
// Priority (re-exported from lib.rs so callers can import from one place)
// ---------------------------------------------------------------------------

pub use crate::InferencePriority as Priority;

// ---------------------------------------------------------------------------
// Params — the cacheable call parameters
// ---------------------------------------------------------------------------

/// Inference call parameters that form part of the cache key.
///
/// `f32` fields are not directly hashable/comparable, so they are
/// quantized to their IEEE 754 bit representation for `Hash` + `Eq`.
/// Two `f32` values that compare `==` always have the same bit
/// representation (for finite values), so this is semantically correct
/// for our use-case (caching identical requests).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InferenceParams {
    /// Maximum number of tokens to generate.
    pub max_tokens: Option<u32>,
    /// Temperature as raw IEEE 754 bits (`u32::from_bits(temperature)`).
    ///
    /// Callers should use [`InferenceParams::new`] which handles the
    /// quantization transparently.
    pub temperature_bits: Option<u32>,
}

impl InferenceParams {
    /// Creates `InferenceParams` from idiomatic `f32` values.
    pub fn new(max_tokens: Option<u32>, temperature: Option<f32>) -> Self {
        Self {
            max_tokens,
            temperature_bits: temperature.map(f32::to_bits),
        }
    }

    /// Returns the temperature as `f32`, or `None` if not set.
    pub fn temperature(&self) -> Option<f32> {
        self.temperature_bits.map(f32::from_bits)
    }
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self::new(None, None)
    }
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

/// A single chat-style message in the request.
#[derive(Debug, Clone)]
pub struct Message {
    /// `"user"`, `"assistant"`, or `"system"`.
    pub role: String,
    /// Text content of the message.
    pub content: String,
}

// ---------------------------------------------------------------------------
// InferenceRequest — the call envelope
// ---------------------------------------------------------------------------

/// Envelope for a single non-streaming inference call.
///
/// This is distinct from [`crate::InferenceRequest`] (the mpsc queue
/// envelope used by the worker).  This envelope is used by the
/// [`InferenceClient`] trait and the [`CachingInferenceClient`] decorator.
pub struct ClientInferenceRequest {
    /// Unique ID for this call.  Used in metrics.
    pub request_id: Uuid,
    /// Per-visitor session ID for correlation.
    pub session_id: Option<String>,
    /// Authenticated account ID for cost attribution.
    pub account_id: Option<Uuid>,
    /// Provider model name (e.g. `"claude-sonnet-4-20250514"`).
    pub model: String,
    /// FNV-1a (or equivalent) hash of the concatenated message contents.
    ///
    /// Computed by the caller so the cache key is stable across envelope
    /// re-construction.  The [`CachingInferenceClient`] trusts this hash;
    /// it is the caller's responsibility to compute it consistently.
    pub prompt_hash: u64,
    /// Priority lane for this request.
    pub priority: Priority,
    /// Cacheable call parameters.
    pub params: InferenceParams,
    /// Ordered list of messages forming the conversation.
    pub messages: Vec<Message>,
}

// ---------------------------------------------------------------------------
// InferenceResponse — the trait response
// ---------------------------------------------------------------------------

/// Response from a non-streaming inference call.
#[derive(Debug, Clone)]
pub struct ClientInferenceResponse {
    /// Generated text.
    pub text: String,
    /// Prompt tokens consumed (if reported by the provider).
    pub tokens_in: Option<u32>,
    /// Completion tokens generated (if reported by the provider).
    pub tokens_out: Option<u32>,
    /// `true` when the response was served from the LRU cache.
    ///
    /// Set by [`CachingInferenceClient`]; `false` on a real LLM call.
    /// Read by [`MeteredInferenceClient`] to emit the correct `cache_hit`
    /// value in the structured metrics event.
    pub cache_hit: bool,
}

// ---------------------------------------------------------------------------
// InferenceClient trait
// ---------------------------------------------------------------------------

/// Async trait for non-streaming LLM completions.
///
/// All concrete provider clients implement this trait.  The
/// [`CachingInferenceClient`] decorator also implements it, wrapping any
/// inner `InferenceClient` with an LRU cache.
///
/// Streaming completions are intentionally excluded — they are single-use
/// and cannot be cached.  Use [`crate::AnyClient`] for streaming.
#[async_trait]
pub trait InferenceClient: Send + Sync {
    /// Execute a non-streaming completion and return the generated text.
    async fn complete(
        &self,
        req: ClientInferenceRequest,
    ) -> Result<ClientInferenceResponse, ParishError>;
}

// ---------------------------------------------------------------------------
// Default cache capacity
// ---------------------------------------------------------------------------

/// Default LRU cache capacity (number of entries).
///
/// Override at runtime with `PARISH_INFERENCE_CACHE_CAPACITY` (parsed as
/// `usize`).  A value of `0` disables caching entirely (same as the
/// `inference-response-cache` flag being off).
pub const DEFAULT_CACHE_CAPACITY: usize = 500;

/// Reads the cache capacity from the environment, falling back to
/// [`DEFAULT_CACHE_CAPACITY`].
pub fn cache_capacity_from_env() -> usize {
    std::env::var("PARISH_INFERENCE_CACHE_CAPACITY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_CACHE_CAPACITY)
}

// ---------------------------------------------------------------------------
// Cache key
// ---------------------------------------------------------------------------

/// LRU cache key: `(prompt_hash, model, params)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    prompt_hash: u64,
    model: String,
    params: InferenceParams,
}

// ---------------------------------------------------------------------------
// CachingInferenceClient
// ---------------------------------------------------------------------------

/// LRU decorator that caches non-streaming inference responses.
///
/// Wraps any [`InferenceClient`] impl.  On a cache hit the inner client is
/// not called.  On a miss the call is forwarded, the response stored, and
/// cost metrics are emitted.
///
/// The cache is in-process and keyed by `(prompt_hash, model, params)`.
/// A future Redis-backed implementation can be dropped in by implementing
/// [`InferenceClient`] on a new struct — the decorator pattern is
/// intentionally pluggable.
///
/// # Thread safety
///
/// The inner cache is protected by a `tokio::sync::Mutex`.  The lock is
/// held only for the hash-map lookup and insert; the actual LLM call runs
/// outside the lock.
pub struct CachingInferenceClient {
    inner: Arc<dyn InferenceClient>,
    cache: Mutex<LruCache<CacheKey, ClientInferenceResponse>>,
}

impl CachingInferenceClient {
    /// Creates a new caching client with the given capacity.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is zero — use `capacity_from_env()` or
    /// supply a value from config.
    pub fn new(inner: Arc<dyn InferenceClient>, capacity: usize) -> Self {
        let cap = std::num::NonZeroUsize::new(capacity)
            .expect("CachingInferenceClient capacity must be > 0");
        Self {
            inner,
            cache: Mutex::new(LruCache::new(cap)),
        }
    }

    /// Creates a new caching client reading capacity from the environment.
    ///
    /// Falls back to [`DEFAULT_CACHE_CAPACITY`] if the environment variable
    /// is not set or cannot be parsed.
    pub fn with_default_capacity(inner: Arc<dyn InferenceClient>) -> Self {
        Self::new(inner, cache_capacity_from_env())
    }
}

#[async_trait]
impl InferenceClient for CachingInferenceClient {
    async fn complete(
        &self,
        req: ClientInferenceRequest,
    ) -> Result<ClientInferenceResponse, ParishError> {
        let key = CacheKey {
            prompt_hash: req.prompt_hash,
            model: req.model.clone(),
            params: req.params.clone(),
        };

        // Check cache under a short-held lock.
        {
            let mut cache = self.cache.lock().await;
            if let Some(cached) = cache.get(&key) {
                let mut resp = cached.clone();
                resp.cache_hit = true;
                return Ok(resp);
            }
        }

        // Miss — call the inner client outside the lock.
        let mut resp = self.inner.complete(req).await?;
        resp.cache_hit = false;

        // Store a copy without the cache_hit flag in the cache (always false
        // for stored entries; the flag is set on retrieval above).
        let mut to_cache = resp.clone();
        to_cache.cache_hit = false;
        let mut cache = self.cache.lock().await;
        cache.put(key, to_cache);

        Ok(resp)
    }
}

// ---------------------------------------------------------------------------
// MeteredInferenceClient — wraps any client to emit metrics on every call
// ---------------------------------------------------------------------------

/// Metrics decorator that emits structured cost metrics on every
/// non-streaming call, regardless of whether caching is enabled.
///
/// When the `inference-response-cache` flag is on, the stack is:
/// `MeteredInferenceClient → CachingInferenceClient → ConcreteClient`
///
/// When off:
/// `MeteredInferenceClient → ConcreteClient`
///
/// Metrics are always emitted at the outermost layer, so `cache_hit` is
/// `true` when the caching layer short-circuits.
pub struct MeteredInferenceClient {
    inner: Arc<dyn InferenceClient>,
}

impl MeteredInferenceClient {
    /// Wraps `inner` with metrics emission.
    pub fn new(inner: Arc<dyn InferenceClient>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl InferenceClient for MeteredInferenceClient {
    async fn complete(
        &self,
        req: ClientInferenceRequest,
    ) -> Result<ClientInferenceResponse, ParishError> {
        let request_id = req.request_id;
        let session_id = req.session_id.clone();
        let account_id = req.account_id;
        let model = req.model.clone();

        let start = Instant::now();
        let result = self.inner.complete(req).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match &result {
            Ok(resp) => {
                emit_metrics(
                    &request_id,
                    session_id.as_deref(),
                    account_id.as_ref(),
                    &model,
                    latency_ms,
                    resp.cache_hit,
                    resp.tokens_in,
                    resp.tokens_out,
                );
            }
            Err(e) => {
                tracing::warn!(
                    target: "parish_inference::metrics",
                    request_id = %request_id,
                    model = %model,
                    latency_ms = latency_ms,
                    error = %e,
                    "inference.call.error"
                );
            }
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Metrics helper
// ---------------------------------------------------------------------------

/// Emits a single structured tracing event with cost/latency metrics.
///
/// Uses the standardized span fields from PR #888:
/// `request_id`, `session_id`, `account_id`, `model`, `latency_ms`,
/// plus `cache_hit`, `tokens_in`, `tokens_out`.
// Each argument is a distinct structured field required by the PR #888 schema.
// Grouping them into a struct would obscure the one-to-one mapping to tracing fields.
#[allow(clippy::too_many_arguments)]
fn emit_metrics(
    request_id: &Uuid,
    session_id: Option<&str>,
    account_id: Option<&Uuid>,
    model: &str,
    latency_ms: u64,
    cache_hit: bool,
    tokens_in: Option<u32>,
    tokens_out: Option<u32>,
) {
    tracing::info!(
        target: "parish_inference::metrics",
        request_id  = %request_id,
        session_id  = ?session_id,
        account_id  = ?account_id,
        model       = %model,
        latency_ms  = latency_ms,
        cache_hit   = cache_hit,
        tokens_in   = ?tokens_in,
        tokens_out  = ?tokens_out,
        "inference.call.complete"
    );
}

// ---------------------------------------------------------------------------
// AnyClient adaptor — implements InferenceClient for the existing AnyClient
// ---------------------------------------------------------------------------

/// Adapts the existing [`crate::AnyClient`] to the new [`InferenceClient`]
/// trait.
///
/// This allows gradual migration: call sites that still hold an `AnyClient`
/// can wrap it in an `AnyClientAdapter` and use the trait interface without
/// changing the underlying network code.
pub struct AnyClientAdapter {
    inner: crate::AnyClient,
}

impl AnyClientAdapter {
    /// Wraps an `AnyClient`.
    pub fn new(inner: crate::AnyClient) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl InferenceClient for AnyClientAdapter {
    async fn complete(
        &self,
        req: ClientInferenceRequest,
    ) -> Result<ClientInferenceResponse, ParishError> {
        // Build a prompt from messages (simple concatenation with role labels).
        let system = req
            .messages
            .iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.as_str());

        let user_text: String = req
            .messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let text = self
            .inner
            .generate(
                &req.model,
                &user_text,
                system,
                req.params.max_tokens,
                req.params.temperature(),
            )
            .await?;

        // AnyClient does not report token counts.
        Ok(ClientInferenceResponse {
            text,
            tokens_in: None,
            tokens_out: None,
            cache_hit: false,
        })
    }
}

// ---------------------------------------------------------------------------
// Build helpers
// ---------------------------------------------------------------------------

/// Constructs the inference client stack for a given `AnyClient`.
///
/// When `cache_enabled` is `true`, the stack is:
/// ```text
/// MeteredInferenceClient → CachingInferenceClient → AnyClientAdapter
/// ```
///
/// When `false`:
/// ```text
/// MeteredInferenceClient → AnyClientAdapter
/// ```
///
/// The returned `Arc<dyn InferenceClient>` can be stored on `AppState`.
pub fn build_inference_client_stack(
    client: crate::AnyClient,
    cache_enabled: bool,
    cache_capacity: usize,
) -> Arc<dyn InferenceClient> {
    let adapter: Arc<dyn InferenceClient> = Arc::new(AnyClientAdapter::new(client));

    let cached: Arc<dyn InferenceClient> = if cache_enabled && cache_capacity > 0 {
        Arc::new(CachingInferenceClient::new(adapter, cache_capacity))
    } else {
        adapter
    };

    Arc::new(MeteredInferenceClient::new(cached))
}

// ---------------------------------------------------------------------------
// FNV-1a prompt hash helper
// ---------------------------------------------------------------------------

/// Computes an FNV-1a hash of all message contents for use as `prompt_hash`.
///
/// Callers are responsible for calling this consistently so the cache key
/// is stable across envelope re-construction.
pub fn hash_messages(messages: &[Message]) -> u64 {
    let mut hash: u64 = 14_695_981_039_346_656_037;
    for msg in messages {
        for byte in msg
            .role
            .bytes()
            .chain(b":".iter().copied())
            .chain(msg.content.bytes())
            .chain(b"\n".iter().copied())
        {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(1_099_511_628_211);
        }
    }
    hash
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ── Mock client ─────────────────────────────────────────────────────────

    /// A mock `InferenceClient` that counts calls and returns a fixed response.
    struct MockClient {
        call_count: Arc<AtomicU32>,
        response_text: String,
    }

    impl MockClient {
        fn new(text: &str) -> (Self, Arc<AtomicU32>) {
            let counter = Arc::new(AtomicU32::new(0));
            (
                Self {
                    call_count: Arc::clone(&counter),
                    response_text: text.to_string(),
                },
                counter,
            )
        }
    }

    #[async_trait]
    impl InferenceClient for MockClient {
        async fn complete(
            &self,
            _req: ClientInferenceRequest,
        ) -> Result<ClientInferenceResponse, ParishError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(ClientInferenceResponse {
                text: self.response_text.clone(),
                tokens_in: Some(10),
                tokens_out: Some(20),
                cache_hit: false,
            })
        }
    }

    // ── Helper ───────────────────────────────────────────────────────────────

    fn make_request(prompt_hash: u64, model: &str) -> ClientInferenceRequest {
        ClientInferenceRequest {
            request_id: Uuid::new_v4(),
            session_id: Some("test-session".to_string()),
            account_id: None,
            model: model.to_string(),
            prompt_hash,
            priority: Priority::Interactive,
            params: InferenceParams::default(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "hello".to_string(),
            }],
        }
    }

    // ── InferenceParams tests ────────────────────────────────────────────────

    #[test]
    fn inference_params_roundtrip() {
        let p = InferenceParams::new(Some(512), Some(0.7_f32));
        assert_eq!(p.max_tokens, Some(512));
        let temp = p.temperature().unwrap();
        // Allow a tiny floating-point rounding difference.
        assert!(
            (temp - 0.7_f32).abs() < 1e-6,
            "temperature roundtrip failed: {temp}"
        );
    }

    #[test]
    fn inference_params_none() {
        let p = InferenceParams::default();
        assert_eq!(p.max_tokens, None);
        assert_eq!(p.temperature(), None);
    }

    #[test]
    fn inference_params_hash_eq() {
        let a = InferenceParams::new(Some(100), Some(0.5_f32));
        let b = InferenceParams::new(Some(100), Some(0.5_f32));
        assert_eq!(a, b);
        // Use a BTreeSet to verify Hash impls are consistent.
        let mut set = std::collections::HashSet::new();
        set.insert(a.clone());
        assert!(set.contains(&b));
    }

    // ── hash_messages test ───────────────────────────────────────────────────

    #[test]
    fn hash_messages_deterministic() {
        let msgs = vec![
            Message {
                role: "system".to_string(),
                content: "You are helpful.".to_string(),
            },
            Message {
                role: "user".to_string(),
                content: "Tell me about Connacht.".to_string(),
            },
        ];
        let h1 = hash_messages(&msgs);
        let h2 = hash_messages(&msgs);
        assert_eq!(h1, h2, "hash must be deterministic");
    }

    #[test]
    fn hash_messages_differs_on_content_change() {
        let a = vec![Message {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];
        let b = vec![Message {
            role: "user".to_string(),
            content: "goodbye".to_string(),
        }];
        assert_ne!(hash_messages(&a), hash_messages(&b));
    }

    // ── Trait conformance: MockClient ────────────────────────────────────────

    #[tokio::test]
    async fn mock_client_complete_returns_response() {
        let (mock, counter) = MockClient::new("hello from mock");
        let req = make_request(42, "test-model");
        let resp = mock.complete(req).await.unwrap();
        assert_eq!(resp.text, "hello from mock");
        assert_eq!(resp.tokens_in, Some(10));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // ── Cache hit/miss test ──────────────────────────────────────────────────

    #[tokio::test]
    async fn caching_client_returns_cached_on_second_call() {
        let (mock, counter) = MockClient::new("cached response");
        let inner: Arc<dyn InferenceClient> = Arc::new(mock);
        let caching = CachingInferenceClient::new(inner, 10);

        // First call — miss, should call inner.
        let req1 = make_request(999, "model-a");
        let resp1 = caching.complete(req1).await.unwrap();
        assert_eq!(resp1.text, "cached response");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "inner called once on miss"
        );

        // Second call with identical key — hit, inner should NOT be called again.
        let req2 = make_request(999, "model-a");
        let resp2 = caching.complete(req2).await.unwrap();
        assert_eq!(resp2.text, "cached response");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "inner not called on cache hit"
        );
    }

    #[tokio::test]
    async fn caching_client_misses_on_different_model() {
        let (mock, counter) = MockClient::new("response");
        let inner: Arc<dyn InferenceClient> = Arc::new(mock);
        let caching = CachingInferenceClient::new(inner, 10);

        caching.complete(make_request(1, "model-a")).await.unwrap();
        caching.complete(make_request(1, "model-b")).await.unwrap();

        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "different model = different key"
        );
    }

    #[tokio::test]
    async fn caching_client_misses_on_different_prompt_hash() {
        let (mock, counter) = MockClient::new("response");
        let inner: Arc<dyn InferenceClient> = Arc::new(mock);
        let caching = CachingInferenceClient::new(inner, 10);

        caching.complete(make_request(1, "model-a")).await.unwrap();
        caching.complete(make_request(2, "model-a")).await.unwrap();

        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "different hash = different key"
        );
    }

    #[tokio::test]
    async fn caching_client_misses_on_different_params() {
        let (mock, counter) = MockClient::new("response");
        let inner: Arc<dyn InferenceClient> = Arc::new(mock);
        let caching = CachingInferenceClient::new(inner, 10);

        let mut req1 = make_request(1, "model-a");
        req1.params = InferenceParams::new(Some(100), None);
        let mut req2 = make_request(1, "model-a");
        req2.params = InferenceParams::new(Some(200), None);

        caching.complete(req1).await.unwrap();
        caching.complete(req2).await.unwrap();

        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "different params = different key"
        );
    }

    // ── Metrics emission test ────────────────────────────────────────────────

    #[tokio::test]
    async fn metered_client_emits_tracing_event_on_success() {
        use std::sync::{Arc as StdArc, Mutex as StdMutex};
        use tracing_subscriber::fmt::MakeWriter;

        // Capture tracing output to a thread-safe string buffer.
        #[derive(Clone)]
        struct BufWriter(StdArc<StdMutex<Vec<u8>>>);
        impl<'a> MakeWriter<'a> for BufWriter {
            type Writer = BufWriterInner;
            fn make_writer(&'a self) -> Self::Writer {
                BufWriterInner(StdArc::clone(&self.0))
            }
        }
        struct BufWriterInner(StdArc<StdMutex<Vec<u8>>>);
        impl std::io::Write for BufWriterInner {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buf: StdArc<StdMutex<Vec<u8>>> = StdArc::new(StdMutex::new(Vec::new()));
        let writer = BufWriter(StdArc::clone(&buf));

        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_max_level(tracing::Level::INFO)
            .finish();

        let (mock, _counter) = MockClient::new("metered response");
        let inner: Arc<dyn InferenceClient> = Arc::new(mock);
        let metered = Arc::new(MeteredInferenceClient::new(inner));

        // Install the subscriber for the scope of the async call.
        // `with_default` returns the closure's return value; we need a Future.
        // Because `with_default` takes a sync closure, we split: capture the
        // guard, do the async work, then drop the guard.
        let _guard = tracing::subscriber::set_default(subscriber);
        let req = make_request(42, "test-model");
        metered.complete(req).await.unwrap();
        drop(_guard);

        let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(
            output.contains("inference.call.complete"),
            "expected 'inference.call.complete' in tracing output; got:\n{output}"
        );
    }

    // ── AnyClientAdapter conformance ─────────────────────────────────────────

    #[tokio::test]
    async fn any_client_adapter_implements_inference_client() {
        let any = crate::AnyClient::simulator();
        let adapter = AnyClientAdapter::new(any);
        let req = make_request(1, "sim");
        let resp = adapter.complete(req).await.unwrap();
        assert!(
            !resp.text.is_empty(),
            "simulator should produce non-empty text"
        );
    }

    // ── build_inference_client_stack ─────────────────────────────────────────

    #[tokio::test]
    async fn build_stack_cache_enabled() {
        let any = crate::AnyClient::simulator();
        let stack = build_inference_client_stack(any, true, 50);
        let req = make_request(1, "sim");
        let resp = stack.complete(req).await.unwrap();
        assert!(!resp.text.is_empty());
    }

    #[tokio::test]
    async fn build_stack_cache_disabled() {
        let any = crate::AnyClient::simulator();
        let stack = build_inference_client_stack(any, false, 50);
        let req = make_request(1, "sim");
        let resp = stack.complete(req).await.unwrap();
        assert!(!resp.text.is_empty());
    }
}
