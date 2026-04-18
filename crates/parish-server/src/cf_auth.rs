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
    /// Cached JWKS (refreshed every 10 min).
    jwks: std::sync::Mutex<Option<Arc<Jwks>>>,
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
            jwks: std::sync::Mutex::new(None),
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

        if !stale
            && let Ok(guard) = self.jwks.lock()
            && let Some(ref cached) = *guard
        {
            return Ok(Arc::clone(cached));
        }

        self.refresh_jwks().await
    }

    /// Unconditionally fetches a fresh JWKS, updating the cache.
    ///
    /// Called on the TTL-miss path above and on a `kid` miss inside
    /// [`Self::validate`] so Cloudflare key rotations don't 401 every
    /// request until the 10-minute TTL expires.
    async fn refresh_jwks(&self) -> Result<Arc<Jwks>, String> {
        let resp = reqwest::get(&self.jwks_url)
            .await
            .map_err(|e| format!("JWKS fetch failed: {e}"))?;
        let jwks: Jwks = resp
            .json()
            .await
            .map_err(|e| format!("JWKS parse failed: {e}"))?;
        let arc = Arc::new(jwks);
        if let Ok(mut guard) = self.jwks.lock() {
            *guard = Some(Arc::clone(&arc));
        }
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_refresh.store(now_secs, Ordering::Relaxed);
        Ok(arc)
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
            key_arc = self.refresh_jwks().await?;
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
        let expected = hex::encode(mac.finalize().into_bytes());

        if sig_hex != expected {
            return Err("invalid signature");
        }

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
}
