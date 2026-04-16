//! Session cookie middleware.
//!
//! Reads the `parish_sid` cookie, resolves (or creates) the per-visitor
//! [`AppState`] via [`SessionRegistry`], and injects it as an Axum
//! [`Extension`] so every downstream route handler can access it.
//!
//! If a new session was created (no valid cookie was present), the middleware
//! adds a `Set-Cookie` header to the response.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::session::{GlobalState, get_or_create_session};

/// Cookie name used to identify a visitor's session.
pub const SESSION_COOKIE: &str = "parish_sid";

/// Current visitor's session id, injected by [`session_middleware`] so that
/// route handlers (e.g. debug) can look up per-session metadata in the
/// global [`crate::session::SessionRegistry`].
#[derive(Clone, Debug)]
pub struct SessionId(pub String);

/// Axum middleware that resolves (or creates) a per-visitor session.
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
            "{}={}; HttpOnly; SameSite=Lax; Max-Age=31536000; Path=/",
            SESSION_COOKIE, session_id
        ))
    {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }

    response
}

/// Extracts the value of a named cookie from a raw `Cookie:` header string.
fn extract_cookie_value(cookies: &str, name: &str) -> Option<String> {
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
}
