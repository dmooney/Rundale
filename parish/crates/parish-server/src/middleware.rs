//! Session cookie middleware.
//!
//! Two implementations live in this module:
//!
//! - [`session_middleware_tower`] — the default path (#364).  Uses the
//!   `tower-sessions` crate to manage the `parish_sid` cookie, eliminating
//!   the hand-rolled `Set-Cookie` insert/append bookkeeping that
//!   previously caused cookie clobbering between session creation and
//!   OAuth flows.  Activated when the `tower-sessions-auth` feature flag
//!   is **not** disabled.
//! - [`session_middleware`] — the legacy hand-rolled implementation.  Kept
//!   so the migration can be flipped off via
//!   `flags.disable("tower-sessions-auth")` without a redeploy if a
//!   regression is discovered.  Deprecated; will be removed once the new
//!   path has soaked.
//!
//! Either implementation reads (or creates) the per-visitor [`AppState`]
//! via [`SessionRegistry`] and injects it as an Axum [`Extension`] so
//! every downstream route handler can access it.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, header};
use axum::middleware::Next;
use axum::response::Response;
use tower_sessions::Session;

use crate::session::{GlobalState, get_or_create_session};

/// Cookie name used to identify a visitor's session.
///
/// Re-used by both the legacy hand-rolled middleware and the
/// `tower-sessions`-backed [`session_middleware_tower`] (configured via
/// `SessionManagerLayer::with_name`) so the on-the-wire cookie name does
/// not change with the migration — existing browsers keep their session
/// across the swap.
pub const SESSION_COOKIE: &str = "parish_sid";

/// Key used to stash the parish session UUID inside a `tower-sessions`
/// session.
pub const TOWER_SESSION_ID_KEY: &str = "parish_session_id";

/// Key used to stash the Google OAuth CSRF state inside a `tower-sessions`
/// session.  Replaces the dedicated `parish_oauth_state` cookie that the
/// legacy auth flow set in [`crate::auth::login_google`].
pub const TOWER_OAUTH_STATE_KEY: &str = "parish_oauth_state";

/// Current visitor's session id, injected by the session middleware so
/// that route handlers (e.g. debug-snapshot, auth-status) can look up
/// per-session metadata in the global [`crate::session::SessionRegistry`].
#[derive(Clone, Debug)]
pub struct SessionId(pub String);

// ── tower-sessions path (default, #364) ──────────────────────────────────────

/// Axum middleware that resolves (or creates) a per-visitor session,
/// using `tower-sessions` for cookie management.
///
/// The middleware reads the parish session UUID stored inside the
/// tower-session under [`TOWER_SESSION_ID_KEY`] (creating a new UUID if
/// the tower-session is fresh), looks the matching `SessionEntry` up in
/// the [`SessionRegistry`], and injects both as Axum extensions for
/// downstream handlers.  Cookie writing is delegated entirely to
/// `SessionManagerLayer`, so the bug-prone `Set-Cookie` `insert` vs
/// `append` ordering goes away.
pub async fn session_middleware_tower(
    State(global): State<Arc<GlobalState>>,
    session: Session,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Auth routes manage their own session linkage (callback writes the
    // parish session id directly), so we still need to early-return here
    // to avoid eagerly creating a throwaway session whose id then loses
    // a race with the OAuth callback's update.
    let path = req.uri().path().to_string();
    if path.starts_with("/auth/") {
        return next.run(req).await;
    }

    // Pull the parish session UUID out of the tower-session, or generate
    // a fresh one and persist it.  `tower-sessions` will only write a
    // `Set-Cookie` header on the way out if the session was modified,
    // which keeps idempotent reads cheap.
    let mut cookie_id: Option<String> = match session.get::<String>(TOWER_SESSION_ID_KEY).await {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "tower-sessions: failed to read parish_session_id from session"
            );
            None
        }
    };

    // Session persistence recovery: if the tower-session is fresh (no stored
    // parish UUID yet), try recovering from the raw `parish_sid` cookie.
    // This handles returning visitors whose cookie was issued before the
    // tower-sessions migration — their UUID is in the persistent DB but not
    // in the new MemoryStore, so tower-sessions would otherwise assign them
    // a brand-new session and clobber their save data.
    if cookie_id.is_none() {
        let raw_headers = req.headers();
        if let Some(raw_id) = extract_cookie_value(
            raw_headers
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
            SESSION_COOKIE,
        ) && global.sessions.exists_in_db(&raw_id)
        {
            tracing::debug!(
                session_id = %raw_id,
                "tower-sessions: recovered pre-migration parish_sid from cookie"
            );
            cookie_id = Some(raw_id);
        }
    }

    let (session_id, entry, is_new) = get_or_create_session(&global, cookie_id.as_deref()).await;

    // If this request created (or replaced) the parish session, persist
    // the new id back into the tower-session so subsequent requests find
    // it.  This is the equivalent of the old "Set-Cookie when is_new"
    // branch — except `tower-sessions` writes the cookie for us.
    let needs_persist = is_new || cookie_id.as_deref() != Some(session_id.as_str());
    if needs_persist
        && let Err(e) = session
            .insert(TOWER_SESSION_ID_KEY, session_id.clone())
            .await
    {
        tracing::warn!(
            error = %e,
            session_id = %session_id,
            "tower-sessions: failed to persist parish_session_id"
        );
    }

    // Inject the per-session AppState and session id as Axum extensions —
    // identical to the legacy middleware so route handlers don't need to
    // change.
    req.extensions_mut().insert(Arc::clone(&entry.app_state));
    req.extensions_mut().insert(SessionId(session_id));

    next.run(req).await
}

// ── Legacy path (kept for the `tower-sessions-auth` killswitch) ──────────────

/// Axum middleware that resolves (or creates) a per-visitor session.
///
/// Hand-rolled cookie handling.  Retained as the killswitch fallback for
/// [`session_middleware_tower`]; will be removed once tower-sessions has
/// soaked in production.  See module docs.
pub async fn session_middleware(
    State(global): State<Arc<GlobalState>>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Auth routes (/auth/login/*, /auth/callback/*, /auth/logout) manage
    // their own `parish_sid` cookies.  If the session middleware also runs
    // on these paths it creates a throwaway session whose Set-Cookie header
    // competes with the handler's — breaking the OAuth flow.
    let path = req.uri().path().to_string();
    if path.starts_with("/auth/") {
        return next.run(req).await;
    }

    // Extract the session ID from the incoming Cookie header.
    let cookie_id = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| extract_cookie_value(cookies, SESSION_COOKIE));

    let (session_id, entry, is_new) = get_or_create_session(&global, cookie_id.as_deref()).await;

    // Inject the per-session AppState and session id as Axum extensions.
    req.extensions_mut().insert(Arc::clone(&entry.app_state));
    req.extensions_mut().insert(SessionId(session_id.clone()));

    let mut response = next.run(req).await;

    // Set the cookie when a new session was created.
    if is_new
        && let Ok(value) = HeaderValue::from_str(&format!(
            "{}={}; HttpOnly; Secure; SameSite=Lax; Max-Age=31536000; Path=/",
            SESSION_COOKIE, session_id
        ))
    {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }

    response
}

/// Extracts the value of a named cookie from a raw `Cookie:` header string.
pub(crate) fn extract_cookie_value(cookies: &str, name: &str) -> Option<String> {
    for pair in cookies.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(name)
            && let Some(rest) = rest.strip_prefix('=')
        {
            return Some(rest.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_cookie_value_single() {
        assert_eq!(
            extract_cookie_value("parish_sid=abc123", "parish_sid"),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn extract_cookie_value_multiple() {
        assert_eq!(
            extract_cookie_value("foo=bar; parish_sid=xyz789; baz=qux", "parish_sid"),
            Some("xyz789".to_string())
        );
    }

    #[test]
    fn extract_cookie_value_missing() {
        assert_eq!(extract_cookie_value("foo=bar; baz=qux", "parish_sid"), None);
    }

    #[test]
    fn tower_session_id_key_stable() {
        // Regression sensor (#364): if this string changes, every existing
        // tower-session in production loses its parish session id mapping
        // on the next deploy.  Keep it stable across releases.
        assert_eq!(TOWER_SESSION_ID_KEY, "parish_session_id");
    }

    #[test]
    fn tower_oauth_state_key_stable() {
        // Regression sensor (#364): the OAuth login redirect stores the CSRF
        // state under this key, and the callback reads it back.  A drift
        // here breaks every in-flight Google login.
        assert_eq!(TOWER_OAUTH_STATE_KEY, "parish_oauth_state");
    }
}
