//! Integration tests for `POST /api/session-init` (#377 / TD-013).
//!
//! This route mints short-lived HMAC tokens used to authenticate WebSocket
//! upgrade requests via `?token=`. The handler requires a valid `AuthContext`
//! extension (injected by `cf_access_guard`).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use tower::ServiceExt as _;

use parish_server::cf_auth::{AuthContext, SessionToken};
use parish_server::routes::session_init;

fn session_init_router() -> Router {
    Router::new().route("/api/session-init", post(session_init))
}

#[tokio::test]
async fn session_init_returns_token_with_valid_auth() {
    let app = session_init_router();

    let mut req = Request::builder()
        .method("POST")
        .uri("/api/session-init")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(AuthContext {
        account_id: uuid::Uuid::new_v4(),
        email: "user@example.com".to_string(),
    });

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let token = json["token"]
        .as_str()
        .expect("response must contain a 'token' field");
    assert!(!token.is_empty(), "token must not be empty");

    let email = SessionToken::validate_full(token).unwrap();
    assert_eq!(email, "user@example.com");
}

#[tokio::test]
async fn session_init_rejects_missing_auth_context() {
    let app = session_init_router();

    let req = Request::builder()
        .method("POST")
        .uri("/api/session-init")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
