//! Bounded retry with exponential backoff for cloud-provider HTTP requests.
//!
//! Cloud providers shed load with HTTP 429 (rate limited) and transient 5xx
//! errors — including Anthropic's 529 "overloaded". Previously those statuses
//! were logged and surfaced to the caller immediately (#1366 §3.4); this
//! module retries them with capped exponential backoff plus jitter, honouring
//! a sane `Retry-After` header when the server supplies one.
//!
//! Scope is deliberately narrow:
//!
//! - **HTTP status failures only.** Transport errors (connection refused,
//!   timeout, TLS) are not retried — they indicate a different failure class
//!   and are out of scope here.
//! - **Never mid-stream.** Streaming callers route only the initial
//!   request/response-status phase through [`send_with_retry`]; once the
//!   first SSE byte has been consumed the request is not retryable.
//! - **Pacing is additive to the GCRA limiter.** Callers acquire their
//!   [`crate::rate_limit::InferenceRateLimiter`] slot once before the first
//!   attempt; the backoff itself paces the retries, so the slot is *not*
//!   re-acquired per attempt.

use std::future::Future;
use std::time::{Duration, SystemTime};

use parish_types::ParishError;
use rand::RngExt;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;

/// Maximum number of retries after the initial request (4 attempts total).
const MAX_RETRIES: u32 = 3;
/// Backoff before the first retry; doubles on each subsequent retry.
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
/// Upper bound on any computed backoff delay.
const MAX_BACKOFF: Duration = Duration::from_secs(8);
/// Upper bound on a server-supplied `Retry-After` value. Anything larger is
/// capped at this value, so a misbehaving server cannot park the game loop
/// for minutes while still honouring the intent to back off.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Sends a request via `send`, retrying rate-limited / transient-error
/// responses with exponential backoff.
///
/// `send` is invoked once per attempt and must build the request from
/// scratch each time — a `reqwest` request body is not reusable across
/// sends. Retryable statuses are 429 and 5xx (see [`is_retryable_status`]);
/// after [`MAX_RETRIES`] retries the last response is returned as-is so the
/// caller's existing status handling (`error_for_status`, body extraction)
/// produces the same errors it always has. Transport errors are mapped to
/// [`ParishError::Network`] immediately, without retry.
pub(crate) async fn send_with_retry<F, Fut>(
    provider: &'static str,
    mut send: F,
) -> Result<reqwest::Response, ParishError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let mut retries = 0u32;
    loop {
        let response = send()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?;
        let status = response.status();
        if !is_retryable_status(status) || retries >= MAX_RETRIES {
            return Ok(response);
        }

        // A sane Retry-After header overrides the computed backoff — the
        // server knows its own load-shedding window better than we do.
        let delay = retry_after_delay(response.headers(), SystemTime::now())
            .unwrap_or_else(|| jittered(base_backoff(retries)));
        retries += 1;
        tracing::warn!(
            provider,
            status = status.as_u16(),
            attempt = retries,
            max_retries = MAX_RETRIES,
            delay_ms = delay.as_millis(),
            "retryable provider response; backing off before retry"
        );
        tokio::time::sleep(delay).await;
    }
}

/// Returns whether `status` warrants a retry.
///
/// Retryable: 429 (rate limited) and all 5xx — which includes Anthropic's
/// non-standard 529 "overloaded". Every other status (notably other 4xx
/// like 400/401/404, which indicate a caller bug, not transient load) is
/// returned to the caller unchanged.
fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Deterministic backoff base for the given retry index (0-based):
/// 500ms, 1s, 2s, 4s, … capped at [`MAX_BACKOFF`].
pub(crate) fn base_backoff(retry: u32) -> Duration {
    // `min(5)` bounds the shift so the multiplier cannot overflow; the cap
    // below dominates from retry 4 onwards anyway.
    INITIAL_BACKOFF
        .saturating_mul(1u32 << retry.min(5))
        .min(MAX_BACKOFF)
}

/// Applies "equal jitter" to a backoff base: a uniform factor in
/// `[0.5, 1.0]` keeps at least half the deterministic delay (so retries
/// still spread out over time) while desynchronising concurrent clients
/// hammering the same provider.
pub(crate) fn jittered(base: Duration) -> Duration {
    let factor: f64 = rand::rng().random_range(0.5..=1.0);
    base.mul_f64(factor)
}

/// Extracts a usable `Retry-After` delay from response headers, if any.
///
/// Returns `None` when the header is absent or unparseable — callers then
/// fall back to the computed backoff. Values exceeding [`MAX_RETRY_AFTER`]
/// are capped rather than rejected, so the caller still honours the intent
/// to back off without being parked for an arbitrarily long time.
pub(crate) fn retry_after_delay(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    parse_retry_after(value, now)
}

/// Parses a `Retry-After` header value (RFC 9110 §10.2.3).
///
/// Accepts both wire forms: delta-seconds (`"120"`) and HTTP-date
/// (`"Wed, 21 Oct 2015 07:28:00 GMT"`, parsed via RFC 2822 rules which
/// cover the IMF-fixdate format). A date in the past yields a zero delay;
/// a value beyond [`MAX_RETRY_AFTER`] is capped at [`MAX_RETRY_AFTER`] so
/// the caller still backs off without being parked for an arbitrary duration.
/// Returns `None` only when the value cannot be parsed at all.
fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let trimmed = value.trim();
    let duration = match trimmed.parse::<u64>() {
        Ok(secs) => Duration::from_secs(secs),
        Err(_) => {
            let date = chrono::DateTime::parse_from_rfc2822(trimmed).ok()?;
            let now: chrono::DateTime<chrono::Utc> = now.into();
            date.signed_duration_since(now)
                .to_std()
                .unwrap_or(Duration::ZERO)
        }
    };
    Some(duration.min(MAX_RETRY_AFTER))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderValue, RETRY_AFTER};

    /// Fixed "now" for deterministic HTTP-date tests (2001-09-09T01:46:40Z).
    fn fixed_now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000)
    }

    // ── backoff schedule ─────────────────────────────────────────────────

    #[test]
    fn base_backoff_doubles_from_500ms() {
        assert_eq!(base_backoff(0), Duration::from_millis(500));
        assert_eq!(base_backoff(1), Duration::from_secs(1));
        assert_eq!(base_backoff(2), Duration::from_secs(2));
        assert_eq!(base_backoff(3), Duration::from_secs(4));
    }

    #[test]
    fn base_backoff_caps_at_eight_seconds() {
        assert_eq!(base_backoff(4), Duration::from_secs(8));
        assert_eq!(base_backoff(5), Duration::from_secs(8));
        // Far beyond the shift bound — must not overflow or exceed the cap.
        assert_eq!(base_backoff(100), Duration::from_secs(8));
    }

    #[test]
    fn jitter_stays_within_half_to_full_base() {
        let base = Duration::from_secs(2);
        for _ in 0..200 {
            let d = jittered(base);
            assert!(d >= base / 2, "jittered delay {d:?} below half of base");
            assert!(d <= base, "jittered delay {d:?} above base");
        }
    }

    // ── status retryability matrix ───────────────────────────────────────

    #[test]
    fn retryable_statuses_are_429_and_5xx() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        // Anthropic's non-standard 529 "overloaded" falls in the 5xx range.
        assert!(is_retryable_status(StatusCode::from_u16(529).unwrap()));
    }

    #[test]
    fn non_retryable_statuses_pass_through() {
        assert!(!is_retryable_status(StatusCode::OK));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::FORBIDDEN));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(StatusCode::UNPROCESSABLE_ENTITY));
    }

    // ── Retry-After parsing ──────────────────────────────────────────────

    #[test]
    fn parse_retry_after_delta_seconds() {
        assert_eq!(parse_retry_after("0", fixed_now()), Some(Duration::ZERO));
        assert_eq!(
            parse_retry_after("5", fixed_now()),
            Some(Duration::from_secs(5))
        );
        // Surrounding whitespace is tolerated.
        assert_eq!(
            parse_retry_after(" 7 ", fixed_now()),
            Some(Duration::from_secs(7))
        );
    }

    #[test]
    fn parse_retry_after_delta_at_cap_is_accepted() {
        assert_eq!(
            parse_retry_after("30", fixed_now()),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn parse_retry_after_delta_beyond_cap_is_capped() {
        assert_eq!(
            parse_retry_after("31", fixed_now()),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            parse_retry_after("86400", fixed_now()),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn parse_retry_after_http_date_future() {
        let now = fixed_now();
        let target: chrono::DateTime<chrono::Utc> = (now + Duration::from_secs(10)).into();
        let parsed = parse_retry_after(&target.to_rfc2822(), now).expect("should parse");
        assert_eq!(parsed, Duration::from_secs(10));
    }

    #[test]
    fn parse_retry_after_imf_fixdate_with_gmt_zone() {
        // The canonical RFC 9110 IMF-fixdate form, with a literal GMT zone.
        let now: SystemTime = chrono::DateTime::parse_from_rfc2822("Wed, 21 Oct 2015 07:27:55 GMT")
            .unwrap()
            .into();
        let parsed = parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT", now).expect("should parse");
        assert_eq!(parsed, Duration::from_secs(5));
    }

    #[test]
    fn parse_retry_after_http_date_in_past_yields_zero() {
        let now = fixed_now();
        let target: chrono::DateTime<chrono::Utc> = (now - Duration::from_secs(60)).into();
        assert_eq!(
            parse_retry_after(&target.to_rfc2822(), now),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn parse_retry_after_http_date_beyond_cap_is_capped() {
        let now = fixed_now();
        let target: chrono::DateTime<chrono::Utc> = (now + Duration::from_secs(3600)).into();
        assert_eq!(
            parse_retry_after(&target.to_rfc2822(), now),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn parse_retry_after_garbage_is_ignored() {
        assert_eq!(parse_retry_after("soon", fixed_now()), None);
        assert_eq!(parse_retry_after("-5", fixed_now()), None);
        assert_eq!(parse_retry_after("1.5", fixed_now()), None);
        assert_eq!(parse_retry_after("", fixed_now()), None);
    }

    #[test]
    fn retry_after_delay_reads_header() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        assert_eq!(
            retry_after_delay(&headers, fixed_now()),
            Some(Duration::from_secs(2))
        );
    }

    #[test]
    fn retry_after_delay_absent_or_invalid_header_is_none() {
        let empty = HeaderMap::new();
        assert_eq!(retry_after_delay(&empty, fixed_now()), None);

        let mut invalid = HeaderMap::new();
        invalid.insert(RETRY_AFTER, HeaderValue::from_static("whenever"));
        assert_eq!(retry_after_delay(&invalid, fixed_now()), None);
    }
}
