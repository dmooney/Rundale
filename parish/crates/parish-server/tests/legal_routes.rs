/// Integration tests for the GPL-3.0 redistribution routes (`/LICENSE`,
/// `/NOTICE`, `/THIRD_PARTY_NOTICES.md`).
///
/// These tests cover two contracts:
///
/// 1. **Content** — each route returns `200 OK`, the right `Content-Type`,
///    and a non-empty body sourced from the repo-root file via
///    `include_str!` in `parish_server::serve_*`.
///
/// 2. **Bypass** — when wired the way `run_server` wires them (registered
///    *after* `cf_access_guard` is applied), the routes remain publicly
///    reachable even with a fail-closed guard in front of the rest of the
///    app.  This pins down the layer-ordering invariant — Axum's `.layer()`
///    only wraps routes registered *before* it, so re-ordering would
///    silently put the licence files behind auth and violate GPL public
///    availability.
use axum::Router;
use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use parish_server::{serve_license, serve_notice, serve_third_party_notices};
use tower::ServiceExt;

const BODY_LIMIT: usize = 64 * 1024;

fn legal_router() -> Router {
    Router::new()
        .route("/LICENSE", get(serve_license))
        .route("/NOTICE", get(serve_notice))
        .route("/THIRD_PARTY_NOTICES.md", get(serve_third_party_notices))
}

async fn assert_route(path: &str, expected_content_type: &str) {
    let req = Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("build request");
    let resp = legal_router().oneshot(req).await.expect("router responded");

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "GET {path} must return 200 OK",
    );

    let ct = resp
        .headers()
        .get(CONTENT_TYPE)
        .unwrap_or_else(|| panic!("GET {path} must carry a Content-Type"))
        .to_str()
        .unwrap_or_else(|_| panic!("GET {path} Content-Type must be ASCII"));
    assert_eq!(
        ct, expected_content_type,
        "GET {path} Content-Type mismatch",
    );

    let bytes = axum::body::to_bytes(resp.into_body(), BODY_LIMIT)
        .await
        .unwrap_or_else(|e| panic!("GET {path} body read failed: {e}"));
    assert!(!bytes.is_empty(), "GET {path} body must be non-empty");
}

#[tokio::test]
async fn license_route_serves_plain_text() {
    assert_route("/LICENSE", "text/plain; charset=utf-8").await;
}

#[tokio::test]
async fn notice_route_serves_plain_text() {
    assert_route("/NOTICE", "text/plain; charset=utf-8").await;
}

#[tokio::test]
async fn third_party_notices_route_serves_markdown() {
    assert_route("/THIRD_PARTY_NOTICES.md", "text/markdown; charset=utf-8").await;
}

#[tokio::test]
async fn license_body_contains_gpl_marker() {
    // Sanity-check that the embedded body really is the GPL-3.0 LICENSE
    // file (not, say, an empty stub) — guards against an `include_str!`
    // path drift after a future repo move.
    let req = Request::builder()
        .uri("/LICENSE")
        .body(Body::empty())
        .unwrap();
    let resp = legal_router().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), BODY_LIMIT)
        .await
        .unwrap();
    let body = std::str::from_utf8(&bytes).expect("LICENSE is UTF-8");
    assert!(
        body.contains("GNU GENERAL PUBLIC LICENSE"),
        "LICENSE body must contain the GPL header; got first 200 bytes: {:?}",
        &body[..body.len().min(200)],
    );
}

/// Stand-in for `cf_access_guard`: rejects every request with `401`.  Used
/// to prove the layer-ordering invariant — routes mounted *after* this
/// layer are not wrapped by it and remain reachable.
async fn always_401(_req: Request<Body>, _next: Next) -> Result<Response, StatusCode> {
    Err(StatusCode::UNAUTHORIZED)
}

#[tokio::test]
async fn legal_routes_bypass_a_fail_closed_guard() {
    // Mirror `run_server`'s wiring shape:
    //   - register a "guarded" route
    //   - apply the always-401 layer
    //   - register the legal routes *after* the layer
    //
    // Then assert that the guarded route is rejected while the legal
    // routes pass through unscathed.
    let app = Router::new()
        .route("/api/health", get(|| async { StatusCode::OK }))
        .layer(middleware::from_fn(always_401))
        .route("/LICENSE", get(serve_license))
        .route("/NOTICE", get(serve_notice))
        .route("/THIRD_PARTY_NOTICES.md", get(serve_third_party_notices));

    // Guarded route is rejected by the stand-in.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "guard must reject /api/health (sanity check on the stand-in)",
    );

    // Legal routes bypass the guard.
    for path in ["/LICENSE", "/NOTICE", "/THIRD_PARTY_NOTICES.md"] {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "{path} must bypass the fail-closed guard",
        );
    }
}
