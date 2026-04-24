/// Integration tests for security hardening response headers (issue #389).
///
/// These tests build a minimal router that applies the same
/// `SetResponseHeaderLayer` stack used in `run_server`, then drive it with
/// `tower::ServiceExt::oneshot` to assert that every response carries the
/// required headers without needing a live TCP port.
use axum::Router;
use axum::http::header::{
    CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::routing::get;
use parish_server::CSP_POLICY;
use tower::ServiceExt;
use tower_http::set_header::SetResponseHeaderLayer;

/// Build the same security-header layer stack as `run_server`.
fn security_header_router() -> Router {
    Router::new()
        .route("/ping", get(|| async { StatusCode::OK }))
        .layer(SetResponseHeaderLayer::overriding(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP_POLICY),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
}

#[tokio::test]
async fn response_has_content_security_policy_header() {
    let app = security_header_router();
    let req = Request::builder()
        .uri("/ping")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let csp = resp
        .headers()
        .get(CONTENT_SECURITY_POLICY)
        .expect("Content-Security-Policy header must be present on every response");
    let csp_str = csp.to_str().unwrap();
    assert!(
        csp_str.contains("default-src 'self'"),
        "CSP must set default-src 'self'; got: {csp_str}"
    );
    assert!(
        csp_str.contains("frame-ancestors 'none'"),
        "CSP must include frame-ancestors 'none'; got: {csp_str}"
    );
    assert!(
        !csp_str.contains("'unsafe-inline'")
            || !csp_str
                .split(';')
                .any(|d| d.trim().starts_with("script-src") && d.contains("'unsafe-inline'")),
        "CSP script-src must NOT include 'unsafe-inline'; got: {csp_str}"
    );
}

#[tokio::test]
async fn response_has_x_frame_options_deny() {
    let app = security_header_router();
    let req = Request::builder()
        .uri("/ping")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let xfo = resp
        .headers()
        .get(X_FRAME_OPTIONS)
        .expect("X-Frame-Options header must be present on every response");
    assert_eq!(xfo, "DENY");
}

#[tokio::test]
async fn response_has_x_content_type_options_nosniff() {
    let app = security_header_router();
    let req = Request::builder()
        .uri("/ping")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let xcto = resp
        .headers()
        .get(X_CONTENT_TYPE_OPTIONS)
        .expect("X-Content-Type-Options header must be present");
    assert_eq!(xcto, "nosniff");
}

#[tokio::test]
async fn response_has_referrer_policy() {
    let app = security_header_router();
    let req = Request::builder()
        .uri("/ping")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let rp = resp
        .headers()
        .get(REFERRER_POLICY)
        .expect("Referrer-Policy header must be present");
    assert_eq!(rp, "strict-origin-when-cross-origin");
}

#[tokio::test]
async fn response_has_permissions_policy() {
    let app = security_header_router();
    let req = Request::builder()
        .uri("/ping")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let pp = resp
        .headers()
        .get("permissions-policy")
        .expect("Permissions-Policy header must be present");
    assert_eq!(pp, "camera=(), microphone=(), geolocation=()");
}
