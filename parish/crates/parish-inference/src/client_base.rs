//! Shared HTTP client fields and builder methods for provider-specific clients.
//!
//! Both [`crate::OpenAiClient`] and [`crate::AnthropicClient`] carry the
//! same set of fields (two `reqwest::Client`s, base URL, optional API key,
//! optional rate limiter) and implement identical builder-chain helpers.
//! This module extracts that commonality so each client struct composes a
//! `ClientBase` instead of duplicating the declarations.

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
        let client = crate::openai_client::build_client_or_fallback(
            Duration::from_secs(config.timeout_secs),
            label,
        );

        let streaming_client = crate::openai_client::build_client_or_fallback(
            Duration::from_secs(config.streaming_timeout_secs),
            streaming_label,
        );

        let normalized = {
            let trimmed = base_url.trim_end_matches('/');
            trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
        };

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
