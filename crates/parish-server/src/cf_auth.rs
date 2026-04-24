//! Cloudflare Access JWT validation and WS session-token helpers.
//!
//! # CF Access verification (#276)
//! `CfAccessVerifier` fetches the JWKS from the Cloudflare Access team domain,
//! caches it for 10 minutes, and validates the `Cf-Access-Jwt-Assertion` header
//! on every request.
//!
//! # WS session tokens (#377)
//! `SessionToken` mints short-lived HMAC-SHA256 tokens used to authenticate
//! WebSocket upgrade requests via `?token=`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::OnceCell;

// ── Auth context (injected as Axum extension) ─────────────────────────────────

/// Identity extracted from a validated Cloudflare Access JWT.
///
/// Injected into request extensions by `cf_access_guard`; downstream handlers
/// can retrieve it with `req.extensions().get::<AuthContext>()`.
#[derive(Clone, Debug)]
pub struct AuthContext {
    pub email: String,
}

// ── JWKS cache ────────────────────────────────────────────────────────────────

/// A single JWK key (RSA public key fields we need for `jsonwebtoken`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct JwkKey {
    pub kid: String,
    pub kty: String,
    #[serde(default)]
    pub n: String,
    #[serde(default)]
    pub e: String,
    #[serde(default)]
    pub x: String,
    #[serde(default)]
    pub y: String,
    #[serde(default)]
    pub crv: String,
    #[serde(rename = "use", default)]
    pub use_: String,
    #[serde(default)]
    pub alg: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Jwks {
    pub keys: Vec<JwkKey>,
}

impl Jwks {
    pub fn find_key(&self, kid: &str) -> Option<&JwkKey> {
        self.keys.iter().find(|k| k.kid == kid)
    }
}

/// Minimum RSA modulus size (in bits) accepted from the JWKS endpoint.
///
/// 2048 is the NIST 800-131A floor for RSA signing keys through 2030; weaker
/// keys (e.g. 512- or 1024-bit moduli served by a compromised or
/// misconfigured JWKS endpoint) are trivially factorable and must be rejected
/// regardless of what `alg` the JWT header claims (#457).
const MIN_RSA_BITS: usize = 2048;

/// Validates that a single JWK meets the security policy enforced by
/// [`CfAccessVerifier`] (#457):
///
/// - If the `use` field is present it must be `"sig"` — keys published for
///   encryption-only must not be trusted for signature verification.
/// - RSA keys must have a modulus of at least [`MIN_RSA_BITS`] bits.
/// - EC keys must use one of the NIST curves we have verified support for in
///   `jsonwebtoken` (P-256, P-384, P-521).
/// - All other `kty` values are rejected — we do not want to quietly trust
///   new key types before deliberately vetting them.
fn validate_jwk(key: &JwkKey) -> Result<(), String> {
    if !key.use_.is_empty() && key.use_ != "sig" {
        return Err(format!("non-signing `use`: {}", key.use_));
    }
    match key.kty.as_str() {
        "RSA" => {
            use base64::Engine;
            // JWK RSA `n` is base64url-encoded big-endian modulus. Strip a
            // leading zero byte if present (present when the high bit is
            // set to keep the value unambiguously positive).
            let n_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(key.n.trim_end_matches('='))
                .map_err(|e| format!("invalid RSA modulus (n): {e}"))?;
            let effective_bits = n_bytes
                .iter()
                .position(|b| *b != 0)
                .map(|first_nonzero| {
                    let stripped = &n_bytes[first_nonzero..];
                    let leading_zeros = stripped[0].leading_zeros() as usize;
                    stripped.len() * 8 - leading_zeros
                })
                .unwrap_or(0);
            if effective_bits < MIN_RSA_BITS {
                return Err(format!(
                    "RSA modulus too small: {effective_bits} bits (min {MIN_RSA_BITS})"
                ));
            }
            Ok(())
        }
        "EC" => match key.crv.as_str() {
            "P-256" | "P-384" | "P-521" => Ok(()),
            "" => Err("EC key missing `crv`".to_string()),
            other => Err(format!("unsupported EC curve: {other}")),
        },
        other => Err(format!("unsupported key type: {other}")),
    }
}

// ── Claims ─────────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct CfClaims {
    pub email: String,
    pub aud: serde_json::Value,
    pub iss: Option<String>,
    pub exp: u64,
}

// ── Verifier ──────────────────────────────────────────────────────────────────

/// Validates `Cf-Access-Jwt-Assertion` JWTs against the Cloudflare Access JWKS.
pub struct CfAccessVerifier {
    pub jwks_url: String,
    pub audience: String,
    pub team_domain: String,
    /// Cached JWKS (refreshed every 10 min). Held briefly for clone + install;
    /// **never** across an HTTP fetch — that's what `refresh_lock` is for.
    jwks: tokio::sync::Mutex<Option<Arc<Jwks>>>,
    /// Coordinates concurrent refreshes so only one outbound HTTPS fetch is
    /// in flight at a time (#501). Held across the full fetch + validate +
    /// install so that peers wait for the first fetch to publish its result,
    /// but `jwks` stays free for happy-path readers in `get_jwks`
    /// (codex-#550 followup: don't block unrelated valid-token requests on
    /// upstream JWKS latency).
    refresh_lock: tokio::sync::Mutex<()>,
    /// Unix timestamp (seconds) of the last JWKS refresh.
    last_refresh: AtomicU64,
}

impl CfAccessVerifier {
    /// Creates a verifier from environment variables.
    ///
    /// - `CF_ACCESS_TEAM_DOMAIN` — e.g. `"myteam.cloudflareaccess.com"`
    /// - `CF_ACCESS_AUD`         — application audience tag
    pub fn from_env() -> Option<Arc<Self>> {
        let team_domain = std::env::var("CF_ACCESS_TEAM_DOMAIN").ok()?;
        let audience = std::env::var("CF_ACCESS_AUD").ok()?;
        let jwks_url = format!(
            "https://{}/cdn-cgi/access/certs",
            team_domain.trim_matches('/')
        );
        Some(Arc::new(Self {
            jwks_url,
            audience,
            team_domain,
            jwks: tokio::sync::Mutex::new(None),
            refresh_lock: tokio::sync::Mutex::new(()),
            last_refresh: AtomicU64::new(0),
        }))
    }

    /// Returns the cached JWKS, refreshing if older than 10 minutes.
    async fn get_jwks(&self) -> Result<Arc<Jwks>, String> {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let last = self.last_refresh.load(Ordering::Relaxed);
        let stale = now_secs.saturating_sub(last) > 600;

        if !stale {
            let guard = self.jwks.lock().await;
            if let Some(ref cached) = *guard {
                return Ok(Arc::clone(cached));
            }
        }

        self.refresh_jwks().await
    }

    /// Fetches a fresh JWKS (TTL-miss path), updating the cache.
    async fn refresh_jwks(&self) -> Result<Arc<Jwks>, String> {
        self.coordinated_refresh(None).await
    }

    /// Fetches a fresh JWKS because we observed a `kid` not present in the
    /// current cache (key-rotation path from [`Self::validate`]). Unlike
    /// [`Self::refresh_jwks`], the peer-recency shortcut only applies if
    /// the peer's cache actually contains the kid we need (addresses codex
    /// P1 on #550).
    async fn refresh_jwks_for_kid(&self, kid: &str) -> Result<Arc<Jwks>, String> {
        self.coordinated_refresh(Some(kid)).await
    }

    /// Core refresh path. Acquires `refresh_lock` (not `jwks`) across the
    /// HTTP fetch, so concurrent happy-path readers of `jwks` are never
    /// blocked on upstream Cloudflare latency (codex followup on #550).
    ///
    /// Sequence under the refresh coordinator:
    /// 1. Re-check peer recency inside the coordinator — if a sibling
    ///    refresh just populated a cache we can use, return it without
    ///    fetching again (#501 thundering-herd collapse).
    /// 2. Perform the HTTP fetch + JSON parse + key-policy filter (#457).
    ///    The `jwks` mutex is not held during any of this.
    /// 3. Briefly take `jwks` to install the fresh `Arc<Jwks>`, then
    ///    record `last_refresh` after install (codex P2 on #550) so slow
    ///    fetches don't leave the timestamp pointing to a moment before
    ///    the cache existed.
    async fn coordinated_refresh(&self, required_kid: Option<&str>) -> Result<Arc<Jwks>, String> {
        let _refresh_guard = self.refresh_lock.lock().await;

        // Step 1 — peer-recency shortcut. Briefly consult the cache.
        {
            let cache = self.jwks.lock().await;
            if let Some(cached) = Self::take_recent_cache(
                &cache,
                self.last_refresh.load(Ordering::Relaxed),
                required_kid,
            ) {
                return Ok(cached);
            }
        }

        // Step 2 — fetch + validate. No cache lock held across `await`.
        let resp = reqwest::get(&self.jwks_url)
            .await
            .map_err(|e| format!("JWKS fetch failed: {e}"))?;
        let mut jwks: Jwks = resp
            .json()
            .await
            .map_err(|e| format!("JWKS parse failed: {e}"))?;

        // #457 — drop keys that fail security-policy checks. A malformed
        // or partially-weak JWKS still yields a usable verifier as long
        // as at least one key survives.
        let before = jwks.keys.len();
        jwks.keys.retain(|k| match validate_jwk(k) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(kid = %k.kid, error = %e, "JWKS: dropping invalid key");
                false
            }
        });
        if jwks.keys.is_empty() && before > 0 {
            return Err(format!(
                "JWKS validation: all {before} keys rejected by policy"
            ));
        }

        // Step 3 — install + record timestamp post-install.
        let arc = Arc::new(jwks);
        {
            let mut guard = self.jwks.lock().await;
            *guard = Some(Arc::clone(&arc));
        }
        let installed_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_refresh.store(installed_secs, Ordering::Relaxed);
        Ok(arc)
    }

    /// Shared recency shortcut used by both refresh entry points. Returns
    /// `Some(cached)` when the current cache was installed recently enough
    /// that a follower should reuse it instead of fetching again. When
    /// `required_kid` is `Some`, the shortcut additionally requires that
    /// the cached JWKS actually contains that kid — otherwise the caller
    /// genuinely needs a new fetch.
    fn take_recent_cache(
        guard: &tokio::sync::MutexGuard<'_, Option<Arc<Jwks>>>,
        last_refresh: u64,
        required_kid: Option<&str>,
    ) -> Option<Arc<Jwks>> {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let recent = now_secs.saturating_sub(last_refresh) < 10;
        let cached = guard.as_ref()?;
        if !recent {
            return None;
        }
        if let Some(kid) = required_kid
            && cached.find_key(kid).is_none()
        {
            return None;
        }
        Some(Arc::clone(cached))
    }

    /// Validates a raw JWT string and returns the extracted [`AuthContext`].
    pub async fn validate(&self, token: &str) -> Result<AuthContext, String> {
        use jsonwebtoken::{Algorithm, DecodingKey, Header, Validation, decode, decode_header};

        // Decode the header to get `kid`.
        let header: Header = decode_header(token).map_err(|e| format!("JWT header parse: {e}"))?;
        let kid = header.kid.ok_or("JWT missing kid")?;

        // Try cached JWKS first; on a kid miss, force-refresh once before rejecting.
        // Cloudflare rotates keys outside our 10-minute TTL; without this retry a
        // rotation would 401 every request until the cache expires naturally.
        let jwks = self.get_jwks().await?;
        let key_arc: Arc<Jwks>;
        let key = if let Some(k) = jwks.find_key(&kid) {
            k
        } else {
            key_arc = self.refresh_jwks_for_kid(&kid).await?;
            key_arc
                .find_key(&kid)
                .ok_or_else(|| format!("Unknown JWKS kid: {kid}"))?
        };

        // Build decoding key.
        let decoding_key = match key.kty.as_str() {
            "RSA" => DecodingKey::from_rsa_components(&key.n, &key.e)
                .map_err(|e| format!("RSA key error: {e}"))?,
            "EC" => DecodingKey::from_ec_components(&key.x, &key.y)
                .map_err(|e| format!("EC key error: {e}"))?,
            other => return Err(format!("Unsupported key type: {other}")),
        };

        let algorithm = match header.alg {
            Algorithm::RS256 => Algorithm::RS256,
            Algorithm::RS384 => Algorithm::RS384,
            Algorithm::RS512 => Algorithm::RS512,
            Algorithm::ES256 => Algorithm::ES256,
            Algorithm::ES384 => Algorithm::ES384,
            other => return Err(format!("Unsupported algorithm: {other:?}")),
        };

        let mut validation = Validation::new(algorithm);
        validation.set_audience(&[&self.audience]);
        validation.set_issuer(&[format!("https://{}", self.team_domain)]);

        let token_data = decode::<CfClaims>(token, &decoding_key, &validation)
            .map_err(|e| format!("JWT validation failed: {e}"))?;

        Ok(AuthContext {
            email: token_data.claims.email,
        })
    }
}

// ── Global verifier singleton ──────────────────────────────────────────────────

static CF_VERIFIER: OnceCell<Option<Arc<CfAccessVerifier>>> = OnceCell::new();

/// Returns the global `CfAccessVerifier`, initialising it once from env.
pub fn global_verifier() -> Option<Arc<CfAccessVerifier>> {
    CF_VERIFIER.get_or_init(CfAccessVerifier::from_env).clone()
}

// ── WS session token (#377) ───────────────────────────────────────────────────

/// A short-lived HMAC-SHA256 session token for WebSocket authentication.
///
/// Format (URL-safe): `<expiry_unix_ts_hex>.<hmac_hex>`
pub struct SessionToken;

fn signing_key() -> Vec<u8> {
    // Initialised once.
    //
    // Release builds **require** `PARISH_WS_SIGNING_KEY` to be set. A random
    // ephemeral key would make tokens minted by one replica unverifiable by
    // any other replica — any non-sticky LB path between `/api/session-init`
    // and `/api/ws` would 401 valid tokens and trigger reconnect loops.
    // Debug builds still auto-generate a key for local dev convenience.
    static KEY: OnceCell<Vec<u8>> = OnceCell::new();
    KEY.get_or_init(|| {
        if let Some(k) = std::env::var("PARISH_WS_SIGNING_KEY")
            .ok()
            .filter(|s| !s.is_empty())
        {
            return k.into_bytes();
        }
        #[cfg(not(debug_assertions))]
        {
            panic!(
                "PARISH_WS_SIGNING_KEY must be set in release builds \
                 (shared across replicas so WS session tokens are verifiable \
                 regardless of which instance served /api/session-init)"
            );
        }
        #[cfg(debug_assertions)]
        {
            use rand::RngCore;
            let mut key = vec![0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            key
        }
    })
    .clone()
}

impl SessionToken {
    const TTL_SECS: u64 = 300; // 5 minutes

    /// Mints a new token for `email`.
    pub fn mint(email: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + Self::TTL_SECS;

        let msg = format!("{email}:{expiry}");
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&signing_key()).expect("HMAC accepts any key length");
        mac.update(msg.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        format!("{expiry:016x}.{sig}")
    }

    /// Validates a token.  Returns the email on success.
    ///
    /// This format (`<expiry_hex>.<sig>`) carries no email; use [`Self::validate_full`]
    /// for the format that embeds the email in the payload.
    pub fn validate(token: &str) -> Result<String, &'static str> {
        let (expiry_hex, sig) = token.split_once('.').ok_or("malformed token")?;
        let expiry = u64::from_str_radix(expiry_hex, 16).map_err(|_| "bad expiry")?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now > expiry {
            return Err("token expired");
        }

        // This format embeds no email in the token — use validate_full instead.
        let _ = sig;
        Err("use validate_full")
    }

    /// Validates a token of the form `<expiry_hex>:<email>.<sig>`.
    ///
    /// Returns the email on success.
    pub fn validate_full(token: &str) -> Result<String, &'static str> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        // token = "<expiry_hex>:<email>.<sig>"
        let dot_pos = token.rfind('.').ok_or("malformed token")?;
        let payload = &token[..dot_pos];
        let sig_hex = &token[dot_pos + 1..];

        let colon_pos = payload.find(':').ok_or("malformed token")?;
        let expiry_hex = &payload[..colon_pos];
        let email = &payload[colon_pos + 1..];

        let expiry = u64::from_str_radix(expiry_hex, 16).map_err(|_| "bad expiry")?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now > expiry {
            return Err("token expired");
        }

        let msg = format!("{email}:{expiry}");
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&signing_key()).expect("HMAC accepts any key length");
        mac.update(msg.as_bytes());
        let sig_bytes = hex::decode(sig_hex).map_err(|_| "invalid signature")?;
        mac.verify_slice(&sig_bytes)
            .map_err(|_| "invalid signature")?;

        Ok(email.to_string())
    }

    /// Mints a token embedding the email so `validate_full` can recover it.
    pub fn mint_full(email: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + Self::TTL_SECS;

        let msg = format!("{email}:{expiry}");
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&signing_key()).expect("HMAC accepts any key length");
        mac.update(msg.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        // payload = "<expiry_hex>:<email>", appended with ".<sig>"
        format!("{expiry:016x}:{email}.{sig}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_token_round_trip() {
        let email = "alice@example.com";
        let token = SessionToken::mint_full(email);
        let recovered = SessionToken::validate_full(&token).unwrap();
        assert_eq!(recovered, email);
    }

    #[test]
    fn session_token_bad_sig_rejected() {
        let token = SessionToken::mint_full("bob@example.com");
        // Corrupt the signature
        let bad = format!("{token}X");
        assert!(SessionToken::validate_full(&bad).is_err());
    }

    // ── #457 JWK policy tests ─────────────────────────────────────────────

    /// Builds a JWK with a synthetic RSA modulus of `bits` bits (all 0xFF,
    /// which gives the maximum possible bit length for the byte count).
    fn rsa_jwk(kid: &str, bits: usize, use_: &str) -> JwkKey {
        use base64::Engine;
        let bytes = vec![0xFFu8; bits.div_ceil(8)];
        let n = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes);
        JwkKey {
            kid: kid.to_string(),
            kty: "RSA".to_string(),
            n,
            e: "AQAB".to_string(),
            x: String::new(),
            y: String::new(),
            crv: String::new(),
            use_: use_.to_string(),
            alg: String::new(),
        }
    }

    fn ec_jwk(kid: &str, crv: &str) -> JwkKey {
        JwkKey {
            kid: kid.to_string(),
            kty: "EC".to_string(),
            n: String::new(),
            e: String::new(),
            x: "dummy-x".to_string(),
            y: "dummy-y".to_string(),
            crv: crv.to_string(),
            use_: String::new(),
            alg: String::new(),
        }
    }

    #[test]
    fn validate_jwk_accepts_2048_bit_rsa() {
        assert!(validate_jwk(&rsa_jwk("k1", 2048, "")).is_ok());
    }

    #[test]
    fn validate_jwk_accepts_4096_bit_rsa() {
        assert!(validate_jwk(&rsa_jwk("k1", 4096, "sig")).is_ok());
    }

    #[test]
    fn validate_jwk_rejects_1024_bit_rsa() {
        let err = validate_jwk(&rsa_jwk("weak", 1024, "")).unwrap_err();
        assert!(err.contains("too small"), "unexpected error: {err}");
    }

    #[test]
    fn validate_jwk_rejects_512_bit_rsa() {
        let err = validate_jwk(&rsa_jwk("tiny", 512, "")).unwrap_err();
        assert!(err.contains("too small"), "unexpected error: {err}");
    }

    #[test]
    fn validate_jwk_rejects_rsa_with_enc_use() {
        let err = validate_jwk(&rsa_jwk("e1", 2048, "enc")).unwrap_err();
        assert!(
            err.contains("non-signing"),
            "expected non-signing rejection, got: {err}"
        );
    }

    #[test]
    fn validate_jwk_accepts_rsa_with_sig_use() {
        assert!(validate_jwk(&rsa_jwk("s1", 2048, "sig")).is_ok());
    }

    #[test]
    fn validate_jwk_rejects_malformed_rsa_modulus() {
        let mut k = rsa_jwk("bad", 2048, "");
        k.n = "not-base64-!!!".to_string();
        let err = validate_jwk(&k).unwrap_err();
        assert!(
            err.contains("invalid RSA modulus"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_jwk_accepts_supported_ec_curves() {
        for crv in ["P-256", "P-384", "P-521"] {
            assert!(
                validate_jwk(&ec_jwk("ec", crv)).is_ok(),
                "curve {crv} should be accepted"
            );
        }
    }

    #[test]
    fn validate_jwk_rejects_unsupported_ec_curve() {
        let err = validate_jwk(&ec_jwk("weird", "secp256k1")).unwrap_err();
        assert!(
            err.contains("unsupported EC curve"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_jwk_rejects_unknown_kty() {
        let k = JwkKey {
            kid: "oct1".to_string(),
            kty: "oct".to_string(),
            n: String::new(),
            e: String::new(),
            x: String::new(),
            y: String::new(),
            crv: String::new(),
            use_: String::new(),
            alg: String::new(),
        };
        let err = validate_jwk(&k).unwrap_err();
        assert!(
            err.contains("unsupported key type"),
            "unexpected error: {err}"
        );
    }

    // ── #501 JWKS refresh concurrency ─────────────────────────────────────

    /// Test-only helper: install a pre-built Jwks into the verifier's cache
    /// under the same lock + recency bookkeeping that `refresh_jwks` uses,
    /// so we can exercise the concurrent-refresh collapse path without
    /// making real HTTP calls.
    impl CfAccessVerifier {
        #[cfg(test)]
        async fn install_jwks_for_test(&self, jwks: Jwks) -> Arc<Jwks> {
            let mut guard = self.jwks.lock().await;
            let arc = Arc::new(jwks);
            *guard = Some(Arc::clone(&arc));
            let now_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            self.last_refresh.store(now_secs, Ordering::Relaxed);
            arc
        }
    }

    fn make_test_verifier() -> Arc<CfAccessVerifier> {
        Arc::new(CfAccessVerifier {
            jwks_url: "https://example.invalid/cdn-cgi/access/certs".to_string(),
            audience: "aud".to_string(),
            team_domain: "team.example".to_string(),
            jwks: tokio::sync::Mutex::new(None),
            refresh_lock: tokio::sync::Mutex::new(()),
            last_refresh: AtomicU64::new(0),
        })
    }

    #[tokio::test]
    async fn get_jwks_returns_cached_when_fresh() {
        let v = make_test_verifier();
        let installed = v
            .install_jwks_for_test(Jwks {
                keys: vec![rsa_jwk("k1", 2048, "sig")],
            })
            .await;
        // get_jwks must not attempt a network call when cache is fresh.
        let fetched = v.get_jwks().await.unwrap();
        assert!(
            Arc::ptr_eq(&installed, &fetched),
            "cached JWKS should be returned by reference"
        );
    }

    #[tokio::test]
    async fn get_jwks_cache_hit_survives_concurrent_callers() {
        let v = make_test_verifier();
        v.install_jwks_for_test(Jwks {
            keys: vec![rsa_jwk("k1", 2048, "sig")],
        })
        .await;
        // Fire many concurrent callers. None should trigger an HTTP refresh
        // (which would fail against the `.invalid` URL) because the cache
        // is fresh.
        let mut handles = Vec::new();
        for _ in 0..32 {
            let v = Arc::clone(&v);
            handles.push(tokio::spawn(async move { v.get_jwks().await }));
        }
        for h in handles {
            assert!(h.await.unwrap().is_ok());
        }
    }

    // ── codex-#550 P1: kid-miss must bypass peer-recency shortcut unless
    //    the peer's cache actually contains the rotated kid.

    #[tokio::test]
    async fn refresh_for_kid_reuses_recent_cache_when_kid_present() {
        let v = make_test_verifier();
        let installed = v
            .install_jwks_for_test(Jwks {
                keys: vec![rsa_jwk("rotated", 2048, "sig")],
            })
            .await;
        // The cache is <10s old AND contains the required kid — the
        // shortcut applies and we return the cached Arc without fetching.
        let got = v.refresh_jwks_for_kid("rotated").await.unwrap();
        assert!(
            Arc::ptr_eq(&installed, &got),
            "peer-refreshed cache containing the kid should be reused"
        );
    }

    #[tokio::test]
    async fn refresh_for_kid_bypasses_shortcut_when_kid_absent() {
        let v = make_test_verifier();
        // Install a fresh cache *without* the kid we're about to ask for.
        // Under the old code, refresh_jwks would have honored the recency
        // shortcut and returned this cache, so validate() would then fail
        // with "Unknown JWKS kid". The kid-miss path must instead force a
        // real fetch — which here hits the `.invalid` URL and errors,
        // proving the fetch was attempted.
        v.install_jwks_for_test(Jwks {
            keys: vec![rsa_jwk("old", 2048, "sig")],
        })
        .await;
        let err = v
            .refresh_jwks_for_kid("newly-rotated")
            .await
            .expect_err("should attempt fetch when required kid is missing");
        assert!(
            err.contains("JWKS fetch failed"),
            "expected real fetch attempt, got: {err}"
        );
    }

    // ── codex-#550 P2: last_refresh must reflect install time, not the
    //    pre-fetch timestamp, so slow fetches don't defeat the recency
    //    shortcut for queued callers.

    #[tokio::test]
    async fn install_updates_last_refresh_timestamp() {
        let v = make_test_verifier();
        // Verifier starts with last_refresh = 0 (epoch).
        assert_eq!(v.last_refresh.load(Ordering::Relaxed), 0);
        v.install_jwks_for_test(Jwks {
            keys: vec![rsa_jwk("k1", 2048, "sig")],
        })
        .await;
        // After install, last_refresh must be close to "now".
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let last = v.last_refresh.load(Ordering::Relaxed);
        assert!(
            now_secs.saturating_sub(last) < 5,
            "last_refresh={last} too far behind now={now_secs}"
        );
    }

    // ── codex-#550 followup: reads must not block on in-flight refresh ─────

    #[tokio::test]
    async fn get_jwks_does_not_block_on_refresh_lock() {
        // Proves that a happy-path reader (fresh cache, no TTL miss) does
        // not contend with an in-flight refresh. We acquire the
        // refresh_lock directly to simulate a refresh stuck on a slow
        // upstream, then observe that get_jwks still resolves without the
        // lock being released.
        let v = make_test_verifier();
        v.install_jwks_for_test(Jwks {
            keys: vec![rsa_jwk("k1", 2048, "sig")],
        })
        .await;

        let refresh_held = v.refresh_lock.lock().await;
        // get_jwks should complete immediately — bounded by a short
        // timeout proves we aren't blocked on the held lock.
        let fetched = tokio::time::timeout(std::time::Duration::from_millis(100), v.get_jwks())
            .await
            .expect("get_jwks must not wait on refresh_lock")
            .expect("get_jwks must succeed with a fresh cache");
        assert_eq!(fetched.keys.len(), 1);
        drop(refresh_held);
    }
}
