//! Shared HTTP client fields and builder methods for provider-specific clients.
//!
//! Both [`crate::OpenAiClient`] and [`crate::AnthropicClient`] carry the
//! same set of fields (two `reqwest::Client`s, base URL, optional API key,
//! optional rate limiter) and implement identical builder-chain helpers.
//! This module extracts that commonality so each client struct composes a
//! `ClientBase` instead of duplicating the declarations.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use parish_config::InferenceConfig;

use crate::rate_limit::InferenceRateLimiter;

/// Shared HTTP client state for API provider clients.
#[derive(Clone)]
pub(crate) struct ClientBase {
    /// HTTP client with default timeout for non-streaming requests.
    pub(crate) client: reqwest::Client,
    /// HTTP client with longer timeout for streaming requests.
    /// Reused across calls to preserve connection pooling.
    pub(crate) streaming_client: reqwest::Client,
    /// Base URL (e.g. `"http://localhost:11434"` or `"https://api.anthropic.com"`).
    pub(crate) base_url: String,
    /// Optional API key sent as auth header.
    pub(crate) api_key: Option<String>,
    /// Optional outbound request rate limiter. `None` means unlimited.
    pub(crate) rate_limiter: Option<InferenceRateLimiter>,
}

impl ClientBase {
    /// Creates a new `ClientBase` with timeouts sourced from `config`.
    ///
    /// The `base_url` is normalised: trailing slashes and an optional `/v1`
    /// suffix are stripped (the provider-specific endpoint paths append their
    /// own `/v1/...` segment).
    pub(crate) fn new(
        base_url: &str,
        api_key: Option<&str>,
        label: &'static str,
        streaming_label: &'static str,
        config: &InferenceConfig,
    ) -> Self {
        Self::new_inner(base_url, api_key, label, streaming_label, config, false)
    }

    /// Creates a client whose base URL is an exact, versioned API prefix.
    /// V2 endpoint contracts own their prefix, so `/v1`, `/api/v1`, and
    /// provider-specific equivalents must never be normalised away.
    pub(crate) fn new_preserving_path(
        base_url: &str,
        api_key: Option<&str>,
        label: &'static str,
        streaming_label: &'static str,
        config: &InferenceConfig,
    ) -> Self {
        Self::new_inner(base_url, api_key, label, streaming_label, config, true)
    }

    fn new_inner(
        base_url: &str,
        api_key: Option<&str>,
        label: &'static str,
        streaming_label: &'static str,
        config: &InferenceConfig,
        preserve_path: bool,
    ) -> Self {
        let normalized = if preserve_path {
            base_url.trim_end_matches('/').to_string()
        } else {
            let trimmed = base_url.trim_end_matches('/');
            trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
        };

        // Cloning reqwest::Client reuses its connection pool. Category
        // overrides commonly resolve to the same Google host, so cache the
        // pair by endpoint and timeout policy rather than opening four pools.
        type ClientPair = (reqwest::Client, reqwest::Client);
        static TRANSPORTS: OnceLock<Mutex<HashMap<String, ClientPair>>> = OnceLock::new();
        // Include a non-reversible key fingerprint so separate credentials
        // never share a transport/auth pool, without storing the secret in a
        // process-global map key or exposing it through debug formatting.
        let mut key_hasher = DefaultHasher::new();
        api_key.unwrap_or_default().hash(&mut key_hasher);
        let pool_key = format!(
            "{}|{:016x}|{}|{}",
            normalized,
            key_hasher.finish(),
            config.timeout_secs,
            config.streaming_timeout_secs
        );
        let transports = TRANSPORTS.get_or_init(|| Mutex::new(HashMap::new()));
        let (client, streaming_client) = transports
            .lock()
            .ok()
            .and_then(|pool| pool.get(&pool_key).cloned())
            .unwrap_or_else(|| {
                let pair = (
                    crate::openai_client::build_client_or_fallback(
                        Duration::from_secs(config.timeout_secs),
                        label,
                    ),
                    crate::openai_client::build_client_or_fallback(
                        Duration::from_secs(config.streaming_timeout_secs),
                        streaming_label,
                    ),
                );
                if let Ok(mut pool) = transports.lock() {
                    pool.insert(pool_key, pair.clone());
                }
                pair
            });

        Self {
            client,
            streaming_client,
            base_url: normalized,
            api_key: api_key.map(|s| s.to_string()),
            rate_limiter: None,
        }
    }

    /// Attaches an outbound rate limiter, returning the modified base.
    pub(crate) fn with_rate_limit(mut self, limiter: InferenceRateLimiter) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    /// Convenience: attach a rate limiter only if `limiter` is `Some`.
    pub(crate) fn maybe_with_rate_limit(self, limiter: Option<InferenceRateLimiter>) -> Self {
        match limiter {
            Some(l) => self.with_rate_limit(l),
            None => self,
        }
    }

    /// Returns whether the rate limiter is attached.
    pub(crate) fn has_rate_limiter(&self) -> bool {
        self.rate_limiter.is_some()
    }

    /// Returns the base URL.
    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Awaits a free slot in the rate limiter (no-op if unlimited).
    pub(crate) async fn acquire_slot(&self) {
        if let Some(rl) = &self.rate_limiter {
            rl.acquire().await;
        }
    }
}
