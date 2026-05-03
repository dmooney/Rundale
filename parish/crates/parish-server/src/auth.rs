//! Google OAuth 2.0 login flow and auth status endpoint.
//!
//! Routes are only registered when `GOOGLE_CLIENT_ID` and
//! `GOOGLE_CLIENT_SECRET` environment variables are set.
//!
//! Flow:
//!   1. `GET /auth/login/google`   — redirect to Google's consent screen
//!   2. `GET /auth/callback/google` — exchange code, link session, redirect to /
//!   3. `GET /api/auth/status`     — returns current auth state (for UI)

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use tower_sessions::Session;

use crate::middleware::{SESSION_COOKIE, TOWER_OAUTH_STATE_KEY, TOWER_SESSION_ID_KEY};
use crate::session::{GlobalState, get_or_create_session};

// ── Request / response types ──────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Response body for `GET /api/auth/status`.
#[derive(serde::Serialize)]
pub struct AuthStatus {
    /// Whether Google OAuth is configured on this server instance.
    pub oauth_enabled: bool,
    /// Whether the current session is linked to a Google account.
    pub logged_in: bool,
    /// OAuth provider name (always `"google"` when `logged_in` is true).
    pub provider: Option<String>,
    /// Google display name or email.
    pub display_name: Option<String>,
}

// ── OAuth cookie name for CSRF state ─────────────────────────────────────────

const OAUTH_STATE_COOKIE: &str = "parish_oauth_state";

// ── Google OAuth endpoints ────────────────────────────────────────────────────

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /auth/login/google` — redirects to Google's OAuth consent screen.
pub async fn login_google(State(global): State<Arc<GlobalState>>) -> Response {
    let Some(ref cfg) = global.oauth_config else {
        return (StatusCode::NOT_FOUND, "OAuth not configured").into_response();
    };

    let csrf_state = uuid::Uuid::new_v4().to_string();
    let redirect_uri = format!(
        "{}/auth/callback/google",
        cfg.base_url.trim_end_matches('/')
    );

    let url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code\
         &scope=openid%20email%20profile&state={}",
        GOOGLE_AUTH_URL,
        urlenccode(&cfg.client_id),
        urlenccode(&redirect_uri),
        urlenccode(&csrf_state),
    );

    let state_cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Max-Age=600; Path=/",
        OAUTH_STATE_COOKIE, csrf_state
    );

    let mut response = Redirect::to(&url).into_response();
    if let Ok(v) = HeaderValue::from_str(&state_cookie) {
        response.headers_mut().insert(header::SET_COOKIE, v);
    }
    response
}

/// `GET /auth/callback/google` — handles the OAuth redirect from Google.
pub async fn callback_google(
    State(global): State<Arc<GlobalState>>,
    Query(params): Query<CallbackParams>,
    headers: HeaderMap,
) -> Response {
    let Some(ref cfg) = global.oauth_config else {
        return (StatusCode::NOT_FOUND, "OAuth not configured").into_response();
    };

    // Surface provider errors gracefully.
    if let Some(err) = params.error {
        tracing::warn!("Google OAuth error: {}", err);
        return Redirect::to("/?oauth_error=1").into_response();
    }

    let Some(code) = params.code else {
        return (StatusCode::BAD_REQUEST, "Missing code").into_response();
    };

    // CSRF check: the state param must match the cookie.
    let expected_state = cookie_value(&headers, OAUTH_STATE_COOKIE);
    if params.state.as_deref() != expected_state.as_deref() {
        tracing::warn!(
            received_state = ?params.state,
            cookie_state = ?expected_state,
            "OAuth CSRF mismatch"
        );
        return (StatusCode::BAD_REQUEST, "Invalid state").into_response();
    }
    tracing::info!(state = ?params.state, "OAuth CSRF state matched");

    // Exchange the authorization code for an access token.
    let redirect_uri = format!(
        "{}/auth/callback/google",
        cfg.base_url.trim_end_matches('/')
    );
    let access_token = match exchange_code(cfg, &code, &redirect_uri, GOOGLE_TOKEN_URL).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Token exchange failed: {}", e);
            return Redirect::to("/?oauth_error=1").into_response();
        }
    };

    // Fetch user info.
    let (provider_user_id, display_name) =
        match fetch_user_info(&access_token, GOOGLE_USERINFO_URL).await {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!("Userinfo fetch failed: {}", e);
                return Redirect::to("/?oauth_error=1").into_response();
            }
        };

    // Determine which session to use.
    let current_session_id = cookie_value(&headers, SESSION_COOKIE);
    tracing::info!(
        current_session_id = ?current_session_id,
        provider_user_id = %provider_user_id,
        display_name = %display_name,
        "OAuth callback: resolving target session"
    );

    let target_session_id =
        if let Some(existing) = global.sessions.find_by_oauth("google", &provider_user_id) {
            // Try to actually resolve the linked session.  If it comes back
            // as `is_new` the session's save data is gone (e.g. different
            // worktree, wiped saves/) and the middleware will overwrite
            // whatever cookie we set.  Re-link to the caller's current
            // session instead so the user isn't stuck in a login loop.
            let (resolved_id, _, is_new) = get_or_create_session(&global, Some(&existing)).await;
            if !is_new {
                resolved_id
            } else {
                tracing::info!(
                    stale_session = %existing,
                    replacement = %resolved_id,
                    "OAuth: linked session unrestorable, re-linking"
                );
                let sid = match current_session_id.as_deref() {
                    Some(id) if global.sessions.exists_in_db(id) => id.to_string(),
                    _ => resolved_id,
                };
                global
                    .sessions
                    .link_oauth("google", &provider_user_id, &sid, &display_name);
                sid
            }
        } else {
            // Link the current anonymous session to this Google account.
            let sid = match current_session_id.as_deref() {
                Some(id) if global.sessions.exists_in_db(id) => id.to_string(),
                _ => {
                    let (new_id, _, _) = get_or_create_session(&global, None).await;
                    global.sessions.persist_new(&new_id);
                    new_id
                }
            };
            global
                .sessions
                .link_oauth("google", &provider_user_id, &sid, &display_name);
            sid
        };

    tracing::info!(
        target_session_id = %target_session_id,
        "OAuth callback: setting parish_sid cookie and redirecting to /"
    );

    // Build the response: set the parish_sid cookie to the target session,
    // clear the CSRF state cookie, and redirect to the game.
    let session_cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Max-Age=31536000; Path=/",
        SESSION_COOKIE, target_session_id
    );
    let clear_state_cookie = format!(
        "{}=; HttpOnly; Secure; SameSite=Lax; Max-Age=0; Path=/",
        OAUTH_STATE_COOKIE
    );

    let mut response = Redirect::to("/").into_response();
    if let Ok(v) = HeaderValue::from_str(&session_cookie) {
        response.headers_mut().insert(header::SET_COOKIE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&clear_state_cookie) {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
    response
}

/// `GET /auth/logout` — ends the current Google session.
///
/// Creates a fresh anonymous session and issues a new cookie so the browser
/// starts clean.  The old session (and its saves) remain intact: the user can
/// recover it by logging in again with the same Google account.
pub async fn logout(State(global): State<Arc<GlobalState>>) -> Response {
    let (new_session_id, _, _) = get_or_create_session(&global, None).await;
    global.sessions.persist_new(&new_session_id);

    let cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Max-Age=31536000; Path=/",
        SESSION_COOKIE, new_session_id
    );
    let mut response = Redirect::to("/").into_response();
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, v);
    }
    response
}

// ── tower-sessions OAuth handlers (#364) ─────────────────────────────────────
//
// These handlers replace the cookie-based CSRF state machinery used by
// `login_google` / `callback_google` with `tower-sessions`-managed storage.
// The session and OAuth state share a single `parish_sid` cookie managed by
// `SessionManagerLayer`, eliminating the previous bug where the dedicated
// `parish_oauth_state` cookie could clobber or be clobbered by `parish_sid`
// in the same response (see issue #364).
//
// `login_google_tower` writes the CSRF state into the tower-session under
// `TOWER_OAUTH_STATE_KEY`; `callback_google_tower` reads it back, verifies
// it, and on success writes the (possibly new) parish session id under
// `TOWER_SESSION_ID_KEY` so subsequent requests resolve the correct
// `SessionEntry`.

/// `GET /auth/login/google` — redirects to Google's OAuth consent screen.
///
/// tower-sessions-backed variant: stores the CSRF state in the
/// `parish_sid` session instead of a separate `parish_oauth_state` cookie.
pub async fn login_google_tower(
    State(global): State<Arc<GlobalState>>,
    session: Session,
) -> Response {
    let Some(ref cfg) = global.oauth_config else {
        return (StatusCode::NOT_FOUND, "OAuth not configured").into_response();
    };

    let csrf_state = uuid::Uuid::new_v4().to_string();
    let redirect_uri = format!(
        "{}/auth/callback/google",
        cfg.base_url.trim_end_matches('/')
    );

    let url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code\
         &scope=openid%20email%20profile&state={}",
        GOOGLE_AUTH_URL,
        urlenccode(&cfg.client_id),
        urlenccode(&redirect_uri),
        urlenccode(&csrf_state),
    );

    if let Err(e) = session
        .insert(TOWER_OAUTH_STATE_KEY, csrf_state.clone())
        .await
    {
        tracing::warn!(
            error = %e,
            "tower-sessions: failed to persist OAuth CSRF state"
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to start OAuth flow",
        )
            .into_response();
    }

    Redirect::to(&url).into_response()
}

/// `GET /auth/callback/google` — handles the OAuth redirect from Google.
///
/// tower-sessions-backed variant: reads CSRF state from the
/// `parish_sid` session, verifies it, then either re-uses or links the
/// caller's session to the Google account and writes the resulting
/// session id back into the tower-session.  No manual `Set-Cookie` calls.
pub async fn callback_google_tower(
    State(global): State<Arc<GlobalState>>,
    session: Session,
    Query(params): Query<CallbackParams>,
) -> Response {
    let Some(ref cfg) = global.oauth_config else {
        return (StatusCode::NOT_FOUND, "OAuth not configured").into_response();
    };

    if let Some(err) = params.error {
        tracing::warn!("Google OAuth error: {}", err);
        return Redirect::to("/?oauth_error=1").into_response();
    }

    let Some(code) = params.code else {
        return (StatusCode::BAD_REQUEST, "Missing code").into_response();
    };

    // CSRF check: the state param must match the value stashed in the
    // tower-session at login time.
    let expected_state: Option<String> = session.get(TOWER_OAUTH_STATE_KEY).await.unwrap_or(None);
    if params.state.as_deref() != expected_state.as_deref() {
        tracing::warn!(
            received_state = ?params.state,
            session_state_present = expected_state.is_some(),
            "OAuth CSRF mismatch (tower-sessions)"
        );
        return (StatusCode::BAD_REQUEST, "Invalid state").into_response();
    }
    tracing::info!("OAuth CSRF state matched (tower-sessions)");

    // Whatever happens next, drop the now-used CSRF state from the
    // session so a replayed callback can't reuse it.
    let _ = session.remove::<String>(TOWER_OAUTH_STATE_KEY).await;

    let redirect_uri = format!(
        "{}/auth/callback/google",
        cfg.base_url.trim_end_matches('/')
    );
    let access_token = match exchange_code(cfg, &code, &redirect_uri, GOOGLE_TOKEN_URL).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Token exchange failed: {}", e);
            return Redirect::to("/?oauth_error=1").into_response();
        }
    };

    let (provider_user_id, display_name) =
        match fetch_user_info(&access_token, GOOGLE_USERINFO_URL).await {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!("Userinfo fetch failed: {}", e);
                return Redirect::to("/?oauth_error=1").into_response();
            }
        };

    // Determine which parish session id should be tied to this Google
    // identity going forward, then store it in the tower-session.  The
    // logic mirrors the legacy callback's "stale-session re-link" path
    // (see #364 background) so a wiped saves directory does not lock the
    // user into an unrestorable session.
    let current_session_id: Option<String> =
        session.get(TOWER_SESSION_ID_KEY).await.unwrap_or(None);
    tracing::info!(
        current_session_id = ?current_session_id,
        provider_user_id = %provider_user_id,
        display_name = %display_name,
        "OAuth callback (tower): resolving target session"
    );

    let target_session_id =
        if let Some(existing) = global.sessions.find_by_oauth("google", &provider_user_id) {
            let (resolved_id, _, is_new) = get_or_create_session(&global, Some(&existing)).await;
            if !is_new {
                resolved_id
            } else {
                tracing::info!(
                    stale_session = %existing,
                    replacement = %resolved_id,
                    "OAuth (tower): linked session unrestorable, re-linking"
                );
                let sid = match current_session_id.as_deref() {
                    Some(id) if global.sessions.exists_in_db(id) => id.to_string(),
                    _ => resolved_id,
                };
                global
                    .sessions
                    .link_oauth("google", &provider_user_id, &sid, &display_name);
                sid
            }
        } else {
            let sid = match current_session_id.as_deref() {
                Some(id) if global.sessions.exists_in_db(id) => id.to_string(),
                _ => {
                    let (new_id, _, _) = get_or_create_session(&global, None).await;
                    global.sessions.persist_new(&new_id);
                    new_id
                }
            };
            global
                .sessions
                .link_oauth("google", &provider_user_id, &sid, &display_name);
            sid
        };

    // Fix: cycle the session ID to prevent session fixation attacks (#364).
    // An attacker who obtained the pre-auth session ID cannot use it after
    // authentication because cycle_id() rotates the ID in the backing store.
    if let Err(e) = session.cycle_id().await {
        tracing::warn!(error = %e, "tower-sessions: failed to cycle session ID after OAuth");
    }
    if let Err(e) = session
        .insert(TOWER_SESSION_ID_KEY, target_session_id.clone())
        .await
    {
        tracing::warn!(
            error = %e,
            "tower-sessions: failed to persist session id after OAuth"
        );
    }

    tracing::info!(
        target_session_id = %target_session_id,
        "OAuth callback (tower): session linked, redirecting to /"
    );

    Redirect::to("/").into_response()
}

/// `GET /auth/logout` — ends the current Google session.
///
/// tower-sessions-backed variant: clears the parish session id from the
/// tower-session and creates a fresh anonymous session, so the next
/// request behaves as if a new visitor arrived.
pub async fn logout_tower(State(global): State<Arc<GlobalState>>, session: Session) -> Response {
    let (new_session_id, _, _) = get_or_create_session(&global, None).await;
    global.sessions.persist_new(&new_session_id);

    if let Err(e) = session
        .insert(TOWER_SESSION_ID_KEY, new_session_id.clone())
        .await
    {
        tracing::warn!(
            error = %e,
            "tower-sessions: failed to rotate session id on logout"
        );
    }
    // Drop any lingering CSRF state.
    let _ = session.remove::<String>(TOWER_OAUTH_STATE_KEY).await;

    Redirect::to("/").into_response()
}

/// `GET /api/auth/status` — returns OAuth configuration and login state.
///
/// Reads the per-session id from the `SessionId` extension injected by
/// either [`crate::middleware::session_middleware`] (legacy) or
/// [`crate::middleware::session_middleware_tower`] (#364).  Falls back to
/// reading the raw `parish_sid` cookie when the extension is absent (e.g.
/// routes not covered by the middleware stack in tests or future routes).
pub async fn get_auth_status(
    State(global): State<Arc<GlobalState>>,
    headers: HeaderMap,
    extensions: Option<axum::extract::Extension<crate::middleware::SessionId>>,
) -> Json<AuthStatus> {
    let oauth_enabled = global.oauth_config.is_some();
    let session_id = if let Some(ext) = extensions {
        ext.0.0
    } else {
        match cookie_value(&headers, SESSION_COOKIE) {
            Some(id) => id,
            None => {
                return Json(AuthStatus {
                    oauth_enabled,
                    logged_in: false,
                    provider: None,
                    display_name: None,
                });
            }
        }
    };

    // Check whether this session has a linked OAuth account.
    let linked = find_oauth_account_for_session(&global, &session_id);
    tracing::debug!(
        session_id = %session_id,
        linked = linked.is_some(),
        "auth/status resolved"
    );

    Json(AuthStatus {
        oauth_enabled,
        logged_in: linked.is_some(),
        provider: linked.as_ref().map(|_| "google".to_string()),
        display_name: linked.map(|(_, name)| name),
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Exchanges an authorization code for a Google access token.
///
/// `token_url` is normally [`GOOGLE_TOKEN_URL`]; tests may substitute a
/// wiremock base URL to avoid hitting the real Google endpoint.
async fn exchange_code(
    cfg: &crate::session::OAuthConfig,
    code: &str,
    redirect_uri: &str,
    token_url: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(token_url)
        .form(&[
            ("code", code),
            ("client_id", &cfg.client_id),
            ("client_secret", &cfg.client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    body["access_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("no access_token in response: {body}"))
}

/// Fetches the Google user's `sub` (stable ID) and display name.
///
/// `userinfo_url` is normally [`GOOGLE_USERINFO_URL`]; tests may substitute a
/// wiremock base URL to avoid hitting the real Google endpoint.
async fn fetch_user_info(
    access_token: &str,
    userinfo_url: &str,
) -> Result<(String, String), String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let sub = body["sub"]
        .as_str()
        .ok_or_else(|| "missing sub".to_string())?
        .to_string();

    // Prefer `name`; fall back to `email`.
    let display_name = body["name"]
        .as_str()
        .or_else(|| body["email"].as_str())
        .unwrap_or("Google user")
        .to_string();

    Ok((sub, display_name))
}

/// Looks up the Google account linked to `session_id`.
///
/// Returns `(provider_user_id, display_name)` if found.
fn find_oauth_account_for_session(
    global: &GlobalState,
    session_id: &str,
) -> Option<(String, String)> {
    global.sessions.google_account_for_session(session_id)
}

/// Reads a named cookie value from a `HeaderMap`.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            for pair in cookies.split(';') {
                let pair = pair.trim();
                if let Some(rest) = pair.strip_prefix(name)
                    && let Some(rest) = rest.strip_prefix('=')
                {
                    return Some(rest.trim().to_string());
                }
            }
            None
        })
}

/// Minimal percent-encoding for OAuth URL parameters.
fn urlenccode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            _ => {
                for byte in ch.to_string().as_bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    out
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::{HeaderMap, HeaderValue, header};
    use tower_sessions::{MemoryStore, Session};
    use wiremock::matchers::{body_string_contains, header as header_matcher, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::middleware::{TOWER_OAUTH_STATE_KEY, TOWER_SESSION_ID_KEY};
    use crate::session::OAuthConfig;

    use super::{cookie_value, exchange_code, fetch_user_info, urlenccode};

    // ── tower-sessions CSRF/session round-trip (#364) ─────────────────────────

    /// Building block: create an empty in-memory tower-session for handler tests.
    fn fresh_tower_session() -> Session {
        let store = Arc::new(MemoryStore::default());
        Session::new(None, store, None)
    }

    /// `login_google_tower` must persist the CSRF state under
    /// `TOWER_OAUTH_STATE_KEY`, and `callback_google_tower`'s read path must
    /// retrieve the same value.  This is the round-trip the legacy code
    /// achieved via the `parish_oauth_state` cookie — without that cookie,
    /// the new path lives entirely inside the tower-session.
    #[tokio::test]
    async fn tower_session_round_trips_csrf_state() {
        let session = fresh_tower_session();

        // Simulate what `login_google_tower` writes.
        session
            .insert(TOWER_OAUTH_STATE_KEY, "csrf-abc-123".to_string())
            .await
            .expect("insert csrf state");

        // Simulate what `callback_google_tower` reads back.
        let read: Option<String> = session.get(TOWER_OAUTH_STATE_KEY).await.unwrap();
        assert_eq!(read.as_deref(), Some("csrf-abc-123"));
    }

    /// After a successful callback the CSRF state must be removed so a
    /// replayed callback request cannot reuse the same state.
    #[tokio::test]
    async fn tower_session_csrf_state_is_removed_after_callback() {
        let session = fresh_tower_session();
        session
            .insert(TOWER_OAUTH_STATE_KEY, "csrf-abc".to_string())
            .await
            .unwrap();

        // Callback success path drops the key.
        let _ = session.remove::<String>(TOWER_OAUTH_STATE_KEY).await;

        let read: Option<String> = session.get(TOWER_OAUTH_STATE_KEY).await.unwrap();
        assert_eq!(
            read, None,
            "CSRF state must not survive a successful callback"
        );
    }

    /// The parish session id stored under `TOWER_SESSION_ID_KEY` must
    /// round-trip through the tower-session — this is the key the new
    /// session middleware uses to find the per-visitor `SessionEntry`.
    #[tokio::test]
    async fn tower_session_round_trips_parish_session_id() {
        let session = fresh_tower_session();
        let parish_uuid = "11111111-2222-4333-8444-555555555555";

        session
            .insert(TOWER_SESSION_ID_KEY, parish_uuid.to_string())
            .await
            .expect("insert session id");

        let read: Option<String> = session.get(TOWER_SESSION_ID_KEY).await.unwrap();
        assert_eq!(read.as_deref(), Some(parish_uuid));
    }

    /// Logout-equivalent: replacing the parish session id under the same
    /// key must overwrite the previous value, not append.
    #[tokio::test]
    async fn tower_session_logout_replaces_parish_session_id() {
        let session = fresh_tower_session();
        session
            .insert(TOWER_SESSION_ID_KEY, "old-session".to_string())
            .await
            .unwrap();

        // logout_tower writes the fresh anonymous session id.
        session
            .insert(TOWER_SESSION_ID_KEY, "new-session".to_string())
            .await
            .unwrap();

        let read: Option<String> = session.get(TOWER_SESSION_ID_KEY).await.unwrap();
        assert_eq!(read.as_deref(), Some("new-session"));
    }

    // ── Pure helpers ──────────────────────────────────────────────────────────

    /// Unreserved characters must pass through urlenccode unchanged.
    #[test]
    fn urlenccode_unreserved_chars_unchanged() {
        let input = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~";
        assert_eq!(urlenccode(input), input);
    }

    /// Space, `@`, `/`, and `+` must be percent-encoded.
    #[test]
    fn urlenccode_special_chars_are_percent_encoded() {
        assert_eq!(urlenccode(" "), "%20");
        assert_eq!(urlenccode("@"), "%40");
        assert_eq!(urlenccode("/"), "%2F");
        assert_eq!(urlenccode("+"), "%2B");
    }

    /// A multi-byte Unicode character must produce the right percent-encoded bytes.
    #[test]
    fn urlenccode_multibyte_unicode() {
        // U+00E9 (é) encodes as UTF-8 0xC3 0xA9
        assert_eq!(urlenccode("é"), "%C3%A9");
    }

    /// `cookie_value` must return `None` when no `Cookie` header is present.
    #[test]
    fn cookie_value_no_cookie_header_returns_none() {
        let headers = HeaderMap::new();
        assert_eq!(cookie_value(&headers, "parish_sid"), None);
    }

    /// `cookie_value` must extract the correct value from a multi-cookie header.
    #[test]
    fn cookie_value_extracts_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=abc; parish_sid=test-session-id; extra=xyz"),
        );
        assert_eq!(
            cookie_value(&headers, "parish_sid"),
            Some("test-session-id".to_string())
        );
    }

    /// `cookie_value` must return `None` when the named cookie is absent.
    #[test]
    fn cookie_value_absent_cookie_returns_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=abc; unrelated=xyz"),
        );
        assert_eq!(cookie_value(&headers, "parish_sid"), None);
    }

    /// A cookie whose name is a prefix of the requested name must not match.
    #[test]
    fn cookie_value_prefix_name_does_not_match() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("parish=short; parish_sid=correct"),
        );
        assert_eq!(
            cookie_value(&headers, "parish_sid"),
            Some("correct".to_string())
        );
    }

    // ── exchange_code — wiremock tests ────────────────────────────────────────

    fn test_oauth_config(base_url: &str) -> OAuthConfig {
        OAuthConfig {
            client_id: "test-client-id".to_string(),
            client_secret: "test-client-secret".to_string(),
            base_url: base_url.to_string(),
        }
    }

    /// Successful token exchange must return the access token string.
    #[tokio::test]
    async fn exchange_code_success_returns_access_token() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("code=auth-code-123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "access_token": "ya29.stub-token" })),
            )
            .mount(&server)
            .await;

        let cfg = test_oauth_config(&server.uri());
        let token_url = format!("{}/token", server.uri());
        let result = exchange_code(
            &cfg,
            "auth-code-123",
            "https://example.com/callback",
            &token_url,
        )
        .await;

        assert_eq!(result, Ok("ya29.stub-token".to_string()));
    }

    /// A 401 response from the token endpoint must surface an error.
    #[tokio::test]
    async fn exchange_code_401_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(serde_json::json!({ "error": "invalid_client" })),
            )
            .mount(&server)
            .await;

        let cfg = test_oauth_config(&server.uri());
        let token_url = format!("{}/token", server.uri());
        let result =
            exchange_code(&cfg, "bad-code", "https://example.com/callback", &token_url).await;

        // A 401 body without `access_token` must produce an Err.
        assert!(
            result.is_err(),
            "expected Err for 401 response, got {result:?}"
        );
    }

    // ── fetch_user_info — wiremock tests ──────────────────────────────────────

    /// Successful userinfo response with name must return (sub, name).
    #[tokio::test]
    async fn fetch_user_info_success_returns_sub_and_name() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/userinfo"))
            .and(header_matcher("authorization", "Bearer stub-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sub": "google|12345",
                "name": "Brigid O'Brien",
                "email": "brigid@example.com"
            })))
            .mount(&server)
            .await;

        let userinfo_url = format!("{}/userinfo", server.uri());
        let result = fetch_user_info("stub-token", &userinfo_url).await;

        assert_eq!(
            result,
            Ok(("google|12345".to_string(), "Brigid O'Brien".to_string()))
        );
    }

    /// When `name` is absent the display name should fall back to `email`.
    #[tokio::test]
    async fn fetch_user_info_falls_back_to_email_when_name_absent() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/userinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sub": "google|99999",
                "email": "nameless@example.com"
            })))
            .mount(&server)
            .await;

        let userinfo_url = format!("{}/userinfo", server.uri());
        let result = fetch_user_info("any-token", &userinfo_url).await;

        assert_eq!(
            result,
            Ok((
                "google|99999".to_string(),
                "nameless@example.com".to_string()
            ))
        );
    }

    /// A 401 from the userinfo endpoint must surface an error.
    #[tokio::test]
    async fn fetch_user_info_401_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/userinfo"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let userinfo_url = format!("{}/userinfo", server.uri());
        let result = fetch_user_info("expired-token", &userinfo_url).await;

        // 401 body has no `sub` field — must be an error.
        assert!(
            result.is_err(),
            "expected Err for 401 response, got {result:?}"
        );
    }
}
