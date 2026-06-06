//! Session-token issuance endpoint (#377).
//!
//! `POST /api/session-init` — issues a short-lived HMAC token for WebSocket auth.

use axum::Json;
use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::IntoResponse;

// ── #377 — WS session-token issuance ────────────────────────────────────────

/// Response body for `POST /api/session-init`.
#[derive(serde::Serialize)]
pub struct SessionInitResponse {
    pub token: String,
}

/// `POST /api/session-init` — issues a short-lived HMAC token for WS auth.
///
/// Reads the `AuthContext` injected by `cf_access_guard` and mints a 5-minute
/// token.  The caller passes `?token=<value>` when opening `/api/ws`.
pub async fn session_init(
    Extension(auth): Extension<crate::cf_auth::AuthContext>,
) -> impl IntoResponse {
    let token = crate::cf_auth::SessionToken::mint_full(&auth.email);
    (StatusCode::OK, Json(SessionInitResponse { token }))
}
