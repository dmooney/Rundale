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

use crate::middleware::{
    SESSION_COOKIE, TOWER_OAUTH_STATE_KEY, TOWER_SESSION_ID_KEY, extract_cookie_value,
};
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

    let url = build_google_auth_url(cfg, &csrf_state, &redirect_uri);

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

    let target_session_id = match resolve_oauth_link(
        &global,
        &provider_user_id,
        &display_name,
        current_session_id,
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
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
    let (new_session_id, _, _) = match get_or_create_session(&global, None).await {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "30")],
                "Server at capacity",
            )
                .into_response();
        }
    };
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

    let url = build_google_auth_url(cfg, &csrf_state, &redirect_uri);

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

    let target_session_id = match resolve_oauth_link(
        &global,
        &provider_user_id,
        &display_name,
        current_session_id,
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
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
    let (new_session_id, _, _) = match get_or_create_session(&global, None).await {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "30")],
                "Server at capacity",
            )
                .into_response();
        }
    };
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

/// Resolves the target session ID for an OAuth callback: either reuses a
/// previously-linked session or creates and links a new anonymous session.
///
/// Shared between the cookie-based and tower-sessions OAuth flows (TD-003).
async fn resolve_oauth_link(
    global: &Arc<GlobalState>,
    provider_user_id: &str,
    display_name: &str,
    current_session_id: Option<String>,
) -> Result<String, Response> {
    let target_session_id = if let Some(existing) =
        global.sessions.find_by_oauth("google", provider_user_id)
    {
        let (resolved_id, _, is_new) = match get_or_create_session(global, Some(&existing)).await {
            Ok(t) => t,
            Err(_) => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(header::RETRY_AFTER, "30")],
                    "Server at capacity",
                )
                    .into_response());
            }
        };
        if !is_new {
            resolved_id
        } else {
            let sid = match current_session_id.as_deref() {
                Some(id) if global.sessions.exists_in_db(id) => id.to_string(),
                _ => resolved_id,
            };
            global
                .sessions
                .link_oauth("google", provider_user_id, &sid, display_name);
            sid
        }
    } else {
        let sid = match current_session_id.as_deref() {
            Some(id) if global.sessions.exists_in_db(id) => id.to_string(),
            _ => {
                let (new_id, _, _) = match get_or_create_session(global, None).await {
                    Ok(t) => t,
                    Err(_) => {
                        return Err((
                            StatusCode::SERVICE_UNAVAILABLE,
                            [(header::RETRY_AFTER, "30")],
                            "Server at capacity",
                        )
                            .into_response());
                    }
                };
                global.sessions.persist_new(&new_id);
                new_id
            }
        };
        global
            .sessions
            .link_oauth("google", provider_user_id, &sid, display_name);
        sid
    };
    Ok(target_session_id)
}

/// Builds the Google OAuth authorization URL with the given parameters.
fn build_google_auth_url(
    cfg: &crate::session::OAuthConfig,
    csrf_state: &str,
    redirect_uri: &str,
) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code\
         &scope=openid%20email%20profile&state={}",
        GOOGLE_AUTH_URL,
        urlenccode(&cfg.client_id),
        urlenccode(redirect_uri),
        urlenccode(csrf_state),
    )
}

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
        .and_then(|cookies| extract_cookie_value(cookies, name))
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

    use std::num::NonZeroUsize;

    use crate::middleware::{TOWER_OAUTH_STATE_KEY, TOWER_SESSION_ID_KEY};
    use crate::session::{GlobalState, OAuthConfig, SessionRegistry};
    use crate::session_store_impl::open_sessions_db;
    use crate::state::UiConfigSnapshot;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::{cookie_value, exchange_code, fetch_user_info, get_auth_status, urlenccode};

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

        assert!(
            result.is_err(),
            "expected Err for 401 response, got {result:?}"
        );
    }

    // ── TD-015: get_auth_status ─────────────────────────────────────────────

    /// With no OAuth config and no session, `/api/auth/status` reports
    /// `oauth_enabled: false, logged_in: false`.
    #[tokio::test]
    async fn auth_status_no_oauth_no_session() {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let saves_dir = dir.path().to_path_buf();
        let sessions = SessionRegistry::open(&saves_dir).unwrap();
        let identity_conn = open_sessions_db(&saves_dir).unwrap();
        let identity_store: std::sync::Arc<dyn parish_core::identity::IdentityStore> =
            std::sync::Arc::new(crate::session_store_impl::SqliteIdentityStore::new(
                identity_conn,
            ));

        let data_dir = saves_dir.clone();
        let tile_cache = parish_core::tile_cache::TileCache::new(
            saves_dir.join("tile-cache"),
            Default::default(),
        );
        let global = Arc::new(GlobalState {
            sessions,
            identity_store,
            oauth_config: None,
            data_dir: data_dir.clone(),
            world_path: data_dir.join("world.json"),
            saves_dir,
            game_mod: None,
            pronunciations: Vec::new(),
            ui_config: UiConfigSnapshot {
                hints_label: String::new(),
                default_accent: String::new(),
                splash_text: String::new(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                auto_pause_timeout_seconds: 60,
            },
            theme_palette: parish_core::game_mod::default_theme_palette(),
            transport: parish_core::world::transport::TransportConfig::default(),
            template_config: crate::state::GameConfig {
                provider_name: String::new(),
                base_url: String::new(),
                api_key: None,
                model_name: String::new(),
                cloud_provider_name: None,
                cloud_model_name: None,
                cloud_api_key: None,
                cloud_base_url: None,
                improv_enabled: false,
                max_follow_up_turns: 2,
                idle_banter_after_secs: 25,
                auto_pause_after_secs: 60,
                category_provider: Default::default(),
                category_model: Default::default(),
                category_api_key: Default::default(),
                category_base_url: Default::default(),
                flags: parish_core::config::FeatureFlags::default(),
                category_rate_limit: Default::default(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                reveal_unexplored_locations: false,
                auto_setup_model: None,
            },
            inference_config: parish_core::config::InferenceConfig::default(),
            ollama_process: tokio::sync::Mutex::new(
                parish_core::inference::client::OllamaProcess::none(),
            ),
            tile_cache,
            idempotency_cache: tokio::sync::Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(1).unwrap(),
            )),
            max_concurrent_sessions: None,
        });

        let headers = HeaderMap::new();
        let result = get_auth_status(State(global), headers, None).await;
        assert!(
            !result.oauth_enabled,
            "oauth_enabled must be false when no OAuth config"
        );
        assert!(
            !result.logged_in,
            "logged_in must be false when no session linked"
        );
        assert_eq!(result.provider, None);
        assert_eq!(result.display_name, None);
    }

    // ── TD-012: OAuth route integration tests ───────────────────────────────

    /// Helper: builds a minimal GlobalState with an OAuth config for testing.
    fn test_auth_global_state(enable_oauth: bool) -> Arc<GlobalState> {
        use crate::session_store_impl::SqliteIdentityStore;
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let saves_dir = dir.path().to_path_buf();
        let sessions = SessionRegistry::open(&saves_dir).unwrap();
        let identity_conn = open_sessions_db(&saves_dir).unwrap();
        let identity_store: Arc<dyn parish_core::identity::IdentityStore> =
            Arc::new(SqliteIdentityStore::new(identity_conn));
        let tile_cache = parish_core::tile_cache::TileCache::new(
            saves_dir.join("tile-cache"),
            Default::default(),
        );
        Arc::new(GlobalState {
            sessions,
            identity_store,
            oauth_config: if enable_oauth {
                Some(OAuthConfig {
                    client_id: "test-client".to_string(),
                    client_secret: "test-secret".to_string(),
                    base_url: "https://example.com".to_string(),
                })
            } else {
                None
            },
            data_dir: saves_dir.clone(),
            world_path: saves_dir.join("world.json"),
            saves_dir,
            game_mod: None,
            pronunciations: Vec::new(),
            ui_config: UiConfigSnapshot {
                hints_label: String::new(),
                default_accent: String::new(),
                splash_text: String::new(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                auto_pause_timeout_seconds: 60,
            },
            theme_palette: parish_core::game_mod::default_theme_palette(),
            transport: parish_core::world::transport::TransportConfig::default(),
            template_config: crate::state::GameConfig {
                provider_name: String::new(),
                base_url: String::new(),
                api_key: None,
                model_name: String::new(),
                cloud_provider_name: None,
                cloud_model_name: None,
                cloud_api_key: None,
                cloud_base_url: None,
                improv_enabled: false,
                max_follow_up_turns: 2,
                idle_banter_after_secs: 25,
                auto_pause_after_secs: 60,
                category_provider: Default::default(),
                category_model: Default::default(),
                category_api_key: Default::default(),
                category_base_url: Default::default(),
                flags: parish_core::config::FeatureFlags::default(),
                category_rate_limit: Default::default(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                reveal_unexplored_locations: false,
                auto_setup_model: None,
            },
            inference_config: parish_core::config::InferenceConfig::default(),
            ollama_process: tokio::sync::Mutex::new(
                parish_core::inference::client::OllamaProcess::none(),
            ),
            tile_cache,
            idempotency_cache: tokio::sync::Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(1).unwrap(),
            )),
            max_concurrent_sessions: None,
        })
    }

    /// `login_google` returns 404 when OAuth is not configured.
    #[tokio::test]
    async fn login_google_no_oauth_returns_404() {
        let global = test_auth_global_state(false);
        let app = axum::Router::new()
            .route("/auth/login/google", get(super::login_google))
            .with_state(global);
        let req = axum::http::Request::builder()
            .uri("/auth/login/google")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// `login_google` redirects to Google when OAuth is configured.
    #[tokio::test]
    async fn login_google_redirects_to_google() {
        let global = test_auth_global_state(true);
        let app = axum::Router::new()
            .route("/auth/login/google", get(super::login_google))
            .with_state(global);
        let req = axum::http::Request::builder()
            .uri("/auth/login/google")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // 303 See Other = redirect.
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            location.starts_with("https://accounts.google.com/"),
            "login must redirect to Google's auth URL"
        );
        assert!(
            location.contains("client_id=test-client"),
            "redirect URL must contain the client_id"
        );
    }

    /// `login_google_tower` returns 404 when OAuth is not configured.
    #[tokio::test]
    async fn login_google_tower_no_oauth_returns_404() {
        let session = fresh_tower_session();
        let global = test_auth_global_state(false);
        let app = axum::Router::new()
            .route("/auth/login/google", get(super::login_google_tower))
            .with_state(global);
        let req = axum::http::Request::builder()
            .uri("/auth/login/google")
            .extension(session)
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// `login_google_tower` redirects to Google when OAuth is configured.
    #[tokio::test]
    async fn login_google_tower_redirects_to_google() {
        let session = fresh_tower_session();
        let global = test_auth_global_state(true);
        let app = axum::Router::new()
            .route("/auth/login/google", get(super::login_google_tower))
            .with_state(global);
        let req = axum::http::Request::builder()
            .uri("/auth/login/google")
            .extension(session)
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            location.starts_with("https://accounts.google.com/"),
            "login_tower must redirect to Google's auth URL"
        );
    }

    /// `logout` returns a redirect when OAuth is configured.
    #[tokio::test]
    async fn logout_returns_redirect() {
        let global = test_auth_global_state(true);
        let app = axum::Router::new()
            .route("/auth/logout", get(super::logout))
            .with_state(global);
        let req = axum::http::Request::builder()
            .uri("/auth/logout")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(location, "/", "logout must redirect to /");
    }

    /// `logout_tower` returns a redirect when OAuth is configured.
    #[tokio::test]
    async fn logout_tower_returns_redirect() {
        let session = fresh_tower_session();
        let global = test_auth_global_state(true);
        let app = axum::Router::new()
            .route("/auth/logout", get(super::logout_tower))
            .with_state(global);
        let req = axum::http::Request::builder()
            .uri("/auth/logout")
            .extension(session)
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(location, "/", "logout_tower must redirect to /");
    }

    /// `callback_google` returns 400 when the `code` parameter is missing.
    #[tokio::test]
    async fn callback_google_missing_code_returns_400() {
        let global = test_auth_global_state(true);
        let app = axum::Router::new()
            .route("/auth/callback/google", get(super::callback_google))
            .with_state(global);
        let req = axum::http::Request::builder()
            .uri("/auth/callback/google")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// `callback_google_tower` returns 400 when the `code` parameter is missing.
    #[tokio::test]
    async fn callback_google_tower_missing_code_returns_400() {
        let session = fresh_tower_session();
        let global = test_auth_global_state(true);
        let app = axum::Router::new()
            .route("/auth/callback/google", get(super::callback_google_tower))
            .with_state(global);
        let req = axum::http::Request::builder()
            .uri("/auth/callback/google")
            .extension(session)
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
