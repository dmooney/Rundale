//! Outbound rate limiting for LLM provider requests.
//!
//! Wraps the [`governor`] crate, which implements the Generic Cell Rate
//! Algorithm (a token-bucket variant). One [`InferenceRateLimiter`] gates
//! all calls flowing through a single [`crate::openai_client::OpenAiClient`],
//! so each provider endpoint gets its own quota.
//!
//! Per-category quotas are configured in `parish.toml` via
//! [`parish_config::RateLimitConfig`] and attached at client construction
//! time — call sites do not need to know about rate limiting.

use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use parish_config::CategoryRateLimit;

/// Rate limiter gating outbound LLM requests for a single provider client.
///
/// Cheap to clone (it's an `Arc` internally), so a limiter can be shared
/// across categories that target the same provider — though in practice
/// each [`crate::openai_client::OpenAiClient`] owns its own.
///
/// Built from a [`CategoryRateLimit`] via [`InferenceRateLimiter::from_config`].
/// A `per_minute` of zero (or invalid burst) disables the limiter and
/// returns `None`.
#[derive(Clone)]
pub struct InferenceRateLimiter {
    inner: Arc<DefaultDirectRateLimiter>,
}

impl InferenceRateLimiter {
    /// Constructs a limiter directly from a sustained rate and burst size.
    ///
    /// Returns `None` if `per_minute` is zero — there's no meaningful
    /// "zero requests per minute" quota and we don't want to deadlock
    /// callers. A `burst` of zero is silently promoted to 1 (the
    /// minimum cell capacity GCRA can represent).
    pub fn new(per_minute: u32, burst: u32) -> Option<Self> {
        let rate = NonZeroU32::new(per_minute)?;
        // Promote burst to at least 1 — a zero-burst quota is meaningless
        // and would prevent any progress.
        let burst = NonZeroU32::new(burst.max(1)).expect("burst.max(1) >= 1");
        let quota = Quota::per_minute(rate).allow_burst(burst);
        Some(Self {
            inner: Arc::new(RateLimiter::direct(quota)),
        })
    }

    /// Constructs a limiter from a parsed config entry.
    ///
    /// Returns `None` if the config entry is missing or has a zero rate.
    pub fn from_config(cfg: Option<CategoryRateLimit>) -> Option<Self> {
        let cfg = cfg?;
        Self::new(cfg.per_minute, cfg.burst)
    }

    /// Awaits a free slot in the limiter, blocking the current task if
    /// the bucket is empty.
    ///
    /// This is the primary integration point: every outbound LLM call
    /// goes through here before sending the HTTP request, so callers
    /// transparently throttle without needing rate-limit awareness.
    pub async fn acquire(&self) {
        self.inner.until_ready().await;
    }

    /// Non-blocking variant: returns `true` if a slot was immediately
    /// available, `false` if the call would have to wait.
    ///
    /// Useful for back-pressure decisions where you'd rather drop a
    /// non-essential request than queue it.
    pub fn try_acquire(&self) -> bool {
        self.inner.check().is_ok()
    }
}

impl std::fmt::Debug for InferenceRateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferenceRateLimiter")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn new_returns_none_for_zero_rate() {
        assert!(InferenceRateLimiter::new(0, 1).is_none());
    }

    #[test]
    fn new_promotes_zero_burst_to_one() {
        let rl = InferenceRateLimiter::new(60, 0).expect("limiter constructed");
        // First call: bucket has 1 slot, should succeed immediately.
        assert!(rl.try_acquire());
    }

    #[test]
    fn from_config_returns_none_for_unset() {
        assert!(InferenceRateLimiter::from_config(None).is_none());
    }

    #[test]
    fn from_config_returns_none_for_zero_rate() {
        let cfg = CategoryRateLimit {
            per_minute: 0,
            burst: 5,
        };
        assert!(InferenceRateLimiter::from_config(Some(cfg)).is_none());
    }

    #[test]
    fn from_config_builds_limiter_when_set() {
        let cfg = CategoryRateLimit {
            per_minute: 60,
            burst: 5,
        };
        assert!(InferenceRateLimiter::from_config(Some(cfg)).is_some());
    }

    #[test]
    fn try_acquire_blocks_after_burst_exhausted() {
        // 60/min == 1/sec; burst of 2 means 2 immediate, then wait ~1s.
        let rl = InferenceRateLimiter::new(60, 2).expect("limiter constructed");
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        // Third call within the same instant should be denied.
        assert!(!rl.try_acquire());
    }

    #[tokio::test]
    async fn acquire_returns_immediately_within_burst() {
        let rl = InferenceRateLimiter::new(600, 5).expect("limiter constructed");
        let start = Instant::now();
        for _ in 0..5 {
            rl.acquire().await;
        }
        // 5 acquires should fit within the burst with negligible delay.
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "burst acquires took too long: {:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn acquire_waits_after_burst_exhausted() {
        // 600/min == 10/sec; burst of 1 means a fresh token every 100ms.
        let rl = InferenceRateLimiter::new(600, 1).expect("limiter constructed");
        rl.acquire().await; // consume the burst
        let start = Instant::now();
        rl.acquire().await; // should wait roughly 100ms
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(50),
            "expected refill wait, got {:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "wait was too long: {:?}",
            elapsed
        );
    }

    #[test]
    fn limiter_is_clone() {
        let rl = InferenceRateLimiter::new(60, 2).expect("limiter");
        let cloned = rl.clone();
        // Both clones share the same bucket.
        assert!(rl.try_acquire());
        assert!(cloned.try_acquire());
        assert!(!rl.try_acquire());
        assert!(!cloned.try_acquire());
    }

    #[test]
    fn limiter_debug_does_not_panic() {
        let rl = InferenceRateLimiter::new(60, 1).expect("limiter");
        let _ = format!("{:?}", rl);
    }
}
