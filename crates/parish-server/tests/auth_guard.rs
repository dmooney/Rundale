/// Integration tests for the CF-Access guard and WS session-token validation
/// (issues #276, #373, #377, #379, #381).
///
/// Session-token tests call `cf_auth::SessionToken` directly.
/// Router tests drive a minimal axum router via `tower::ServiceExt::oneshot`.
use axum::Router;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt as _;

use parish_server::cf_auth::SessionToken;

// ── Session-token unit-style tests ────────────────────────────────────────────

#[test]
fn missing_token_is_rejected() {
    // Empty string has no dot separator → malformed.
    assert!(SessionToken::validate_full("").is_err());
}

#[test]
fn malformed_token_no_dot_is_rejected() {
    assert!(SessionToken::validate_full("notavalidtoken").is_err());
}

#[test]
fn token_with_bad_signature_is_rejected() {
    let token = SessionToken::mint_full("user@example.com");
    // Corrupt the last byte of the signature.
    let bad = format!("{}Z", &token[..token.len() - 1]);
    assert!(SessionToken::validate_full(&bad).is_err());
}

#[test]
fn expired_token_is_rejected() {
    // Craft a token where the expiry field is unix epoch 0 (always in the past).
    // Format: "<expiry_hex>:<email>.<sig>" — expiry check fires before sig check.
    let past_token = "0000000000000000:user@example.com.deadbeef";
    let result = SessionToken::validate_full(past_token);
    assert_eq!(result, Err("token expired"));
}

#[test]
fn valid_token_round_trips() {
    let email = "alice@example.com";
    let token = SessionToken::mint_full(email);
    let recovered = SessionToken::validate_full(&token).unwrap();
    assert_eq!(recovered, email);
}

// ── /api/health reachability (no ConnectInfo needed) ─────────────────────────

/// A minimal router with a stub auth middleware that rejects everything
/// *except* `/api/health`, mirroring the placement in `run_server`.
fn health_router() -> Router {
    use axum::middleware::{self as axum_mw, Next};
    use axum::response::Response;

    async fn stub_guard(
        req: Request<axum::body::Body>,
        next: Next,
    ) -> Result<Response, StatusCode> {
        if req.uri().path() == "/api/health" {
            return Ok(next.run(req).await);
        }
        Err(StatusCode::UNAUTHORIZED)
    }

    Router::new()
        .route("/api/health", get(|| async { (StatusCode::OK, "ok") }))
        .route("/api/protected", get(|| async { StatusCode::OK }))
        .layer(axum_mw::from_fn(stub_guard))
}

#[tokio::test]
async fn health_route_returns_200_without_auth() {
    let req = Request::builder()
        .uri("/api/health")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = health_router().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn protected_route_returns_401_without_auth() {
    let req = Request::builder()
        .uri("/api/protected")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = health_router().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
