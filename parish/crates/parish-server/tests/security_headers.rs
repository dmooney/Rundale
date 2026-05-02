/// Integration tests for security hardening response headers (issue #389).
///
/// These tests drive a minimal router that is wrapped with the production
/// `apply_security_layers` helper from `parish_server`, so every assertion
/// exercises the real header values rather than a hand-rolled duplicate of the
/// `run_server` stack.
use axum::Router;
use axum::http::header::{
    CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use parish_server::{CSP_POLICY, apply_security_layers};
use tower::ServiceExt;

/// Build a minimal ping router wrapped in the production security-header stack.
fn security_header_router() -> Router {
    let inner = Router::new().route("/ping", get(|| async { StatusCode::OK }));
    apply_security_layers(inner)
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
    // Verify the value matches the production constant exactly.
    assert_eq!(
        csp_str, CSP_POLICY,
        "CSP header value must match the production CSP_POLICY constant"
    );
    assert!(
        csp_str.contains("default-src 'self'"),
        "CSP must set default-src 'self'; got: {csp_str}"
    );
    assert!(
        csp_str.contains("frame-ancestors 'none'"),
        "CSP must include frame-ancestors 'none'; got: {csp_str}"
    );
    // TODO(#543): script-src currently allows 'unsafe-inline' as a temporary
    // shim so that the SvelteKit inline bootstrap <script> is not rejected by
    // the browser (which would prevent page hydration).  Once the build pipeline
    // computes a 'sha256-...' hash for the bootstrap block, remove 'unsafe-inline'
    // and assert its absence here.
    // For now we assert that script-src is present and contains 'self'.
    assert!(
        csp_str
            .split(';')
            .any(|d| d.trim().starts_with("script-src") && d.contains("'self'")),
        "CSP script-src must include 'self'; got: {csp_str}"
    );
}

#[tokio::test]
async fn response_has_strict_transport_security_header() {
    let app = security_header_router();
    let req = Request::builder()
        .uri("/ping")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let hsts = resp
        .headers()
        .get(STRICT_TRANSPORT_SECURITY)
        .expect("Strict-Transport-Security header must be present");
    assert_eq!(hsts, "max-age=31536000; includeSubDomains");
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
