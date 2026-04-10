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

use crate::middleware::SESSION_COOKIE;
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
        "{}={}; HttpOnly; SameSite=Lax; Max-Age=600; Path=/",
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
        tracing::warn!("OAuth CSRF mismatch");
        return (StatusCode::BAD_REQUEST, "Invalid state").into_response();
    }

    // Exchange the authorization code for an access token.
    let redirect_uri = format!(
        "{}/auth/callback/google",
        cfg.base_url.trim_end_matches('/')
    );
    let access_token = match exchange_code(cfg, &code, &redirect_uri).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Token exchange failed: {}", e);
            return Redirect::to("/?oauth_error=1").into_response();
        }
    };

    // Fetch user info.
    let (provider_user_id, _display_name) = match fetch_user_info(&access_token).await {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("Userinfo fetch failed: {}", e);
            return Redirect::to("/?oauth_error=1").into_response();
        }
    };

    // Determine which session to use.
    let current_session_id = cookie_value(&headers, SESSION_COOKIE);

    let target_session_id = if let Some(existing) =
        global.sessions.find_by_oauth("google", &provider_user_id)
    {
        // A session is already linked to this Google account — use it.
        existing
    } else {
        // Link the current anonymous session to this Google account.
        let sid = match current_session_id.as_deref() {
            Some(id) if global.sessions.exists_in_db(id) => id.to_string(),
            _ => {
                // No current session — create a fresh one.
                let (new_id, _, _) = get_or_create_session(&global, None).await;
                global.sessions.persist_new(&new_id);
                new_id
            }
        };
        global
            .sessions
            .link_oauth("google", &provider_user_id, &sid);
        tracing::info!(session_id = %sid, google_id = %provider_user_id, "Google account linked");
        sid
    };

    // Build the response: set the parish_sid cookie to the target session,
    // clear the CSRF state cookie, and redirect to the game.
    let session_cookie = format!(
        "{}={}; HttpOnly; SameSite=Lax; Max-Age=31536000; Path=/",
        SESSION_COOKIE, target_session_id
    );
    let clear_state_cookie = format!(
        "{}=; HttpOnly; SameSite=Lax; Max-Age=0; Path=/",
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

/// `GET /api/auth/status` — returns OAuth configuration and login state.
pub async fn get_auth_status(
    State(global): State<Arc<GlobalState>>,
    headers: HeaderMap,
) -> Json<AuthStatus> {
    let oauth_enabled = global.oauth_config.is_some();

    let session_id = match cookie_value(&headers, SESSION_COOKIE) {
        Some(id) => id,
        None => {
            return Json(AuthStatus {
                oauth_enabled,
                logged_in: false,
                provider: None,
                display_name: None,
            });
        }
    };

    // Check whether this session has a linked OAuth account.
    let linked = find_oauth_account_for_session(&global, &session_id);

    Json(AuthStatus {
        oauth_enabled,
        logged_in: linked.is_some(),
        provider: linked.as_ref().map(|_| "google".to_string()),
        display_name: linked.map(|(_, name)| name),
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Exchanges an authorization code for a Google access token.
async fn exchange_code(
    cfg: &crate::session::OAuthConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(GOOGLE_TOKEN_URL)
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
async fn fetch_user_info(access_token: &str) -> Result<(String, String), String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(GOOGLE_USERINFO_URL)
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
/// Returns `(provider_user_id, display_name)` if found.  The Google `sub`
/// doubles as the display name since we don't persist the user's name.
fn find_oauth_account_for_session(
    global: &GlobalState,
    session_id: &str,
) -> Option<(String, String)> {
    global
        .sessions
        .google_account_for_session(session_id)
        .map(|uid| (uid.clone(), uid))
}

/// Reads a named cookie value from a `HeaderMap`.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            for pair in cookies.split(';') {
                let pair = pair.trim();
                if let Some(rest) = pair.strip_prefix(name) {
                    if let Some(rest) = rest.strip_prefix('=') {
                        return Some(rest.trim().to_string());
                    }
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
