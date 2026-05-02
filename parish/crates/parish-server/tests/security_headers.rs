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
use parish_server::{ALLOWED_EXTERNAL_ORIGINS, CSP_POLICY, apply_security_layers};
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

// ── CSP tightening assertions (issue #751) ────────────────────────────────────

/// Parse a CSP header value into its directives, returning the tokens for the
/// named directive (or an empty slice if the directive is absent).
///
/// Tokenisation: split on `;`, trim each part, find the one that starts with
/// `directive_name`, then split the rest on whitespace to get the token list.
fn csp_directive_tokens<'a>(csp: &'a str, directive: &str) -> Vec<&'a str> {
    for part in csp.split(';') {
        let part = part.trim();
        if let Some(values) = part.strip_prefix(directive) {
            // Skip the directive name itself and collect the value tokens.
            return values.split_whitespace().collect();
        }
    }
    vec![]
}

#[tokio::test]
async fn csp_connect_src_no_bare_https_wildcard() {
    // connect-src must NOT contain the bare `https:` scheme wildcard.
    // The former policy had `https:` which allowed any HTTPS endpoint.
    let tokens = csp_directive_tokens(CSP_POLICY, "connect-src");
    assert!(
        !tokens.is_empty(),
        "CSP must contain a connect-src directive"
    );
    assert!(
        !tokens.contains(&"https:"),
        "connect-src must not contain bare `https:` wildcard; tokens: {tokens:?}"
    );
}

#[tokio::test]
async fn csp_connect_src_contains_required_ws_schemes() {
    // WebSocket origins are still required for the game's live WS connection.
    let tokens = csp_directive_tokens(CSP_POLICY, "connect-src");
    assert!(
        tokens.contains(&"'self'"),
        "connect-src must contain 'self'; tokens: {tokens:?}"
    );
    assert!(
        tokens.contains(&"ws:"),
        "connect-src must contain ws: for WebSocket; tokens: {tokens:?}"
    );
    assert!(
        tokens.contains(&"wss:"),
        "connect-src must contain wss: for secure WebSocket; tokens: {tokens:?}"
    );
}

#[tokio::test]
async fn csp_connect_src_contains_allowed_external_origins() {
    // Every origin in ALLOWED_EXTERNAL_ORIGINS that the frontend genuinely
    // connects to must appear in connect-src.  The tile-server and glyph-server
    // origins are fetched at runtime by MapLibre (tiles via fetch(), glyphs via
    // XHR), so they must be explicitly listed.
    //
    // Exceptions: fonts.gstatic.com is only in font-src (file downloads), not
    // connect-src, so we only check the subset that is actually fetched.
    let connect_fetch_origins: &[&str] = &[
        "https://tile.openstreetmap.org",
        "https://mapseries-tilesets.s3.amazonaws.com",
        "https://demotiles.maplibre.org",
        "https://fonts.googleapis.com",
    ];
    let tokens = csp_directive_tokens(CSP_POLICY, "connect-src");
    for origin in connect_fetch_origins {
        assert!(
            tokens.contains(origin),
            "connect-src must contain {origin}; tokens: {tokens:?}"
        );
    }
}

#[tokio::test]
async fn csp_img_src_no_bare_https_wildcard() {
    // img-src must NOT contain the bare `https:` scheme wildcard.
    let tokens = csp_directive_tokens(CSP_POLICY, "img-src");
    assert!(!tokens.is_empty(), "CSP must contain an img-src directive");
    assert!(
        !tokens.contains(&"https:"),
        "img-src must not contain bare `https:` wildcard; tokens: {tokens:?}"
    );
}

#[tokio::test]
async fn csp_img_src_contains_tile_servers() {
    // MapLibre renders tiles as raster images, so the tile-server origins must
    // appear in img-src as well as connect-src.
    let tile_origins: &[&str] = &[
        "https://tile.openstreetmap.org",
        "https://mapseries-tilesets.s3.amazonaws.com",
    ];
    let tokens = csp_directive_tokens(CSP_POLICY, "img-src");
    for origin in tile_origins {
        assert!(
            tokens.contains(origin),
            "img-src must contain {origin} for MapLibre raster tiles; tokens: {tokens:?}"
        );
    }
    // data: and blob: must be retained for MapLibre canvas sprite sheets.
    assert!(
        tokens.contains(&"data:"),
        "img-src must retain data: for canvas sprite data URIs; tokens: {tokens:?}"
    );
    assert!(
        tokens.contains(&"blob:"),
        "img-src must retain blob: for MapLibre blob URLs; tokens: {tokens:?}"
    );
}

#[tokio::test]
async fn csp_allowed_external_origins_list_exhaustive() {
    // ALLOWED_EXTERNAL_ORIGINS lists all external HTTPS origins used by the
    // frontend.  Assert that no origin in the list has been silently dropped
    // from every directive that uses it.
    //
    // Each origin must appear in at least one of: connect-src, img-src, or
    // font-src.  An origin in ALLOWED_EXTERNAL_ORIGINS that isn't in any of
    // these directives indicates either a stale list or a CSP regression.
    let connect_tokens = csp_directive_tokens(CSP_POLICY, "connect-src");
    let img_tokens = csp_directive_tokens(CSP_POLICY, "img-src");
    let font_tokens = csp_directive_tokens(CSP_POLICY, "font-src");

    for origin in ALLOWED_EXTERNAL_ORIGINS {
        let present = connect_tokens.contains(origin)
            || img_tokens.contains(origin)
            || font_tokens.contains(origin);
        assert!(
            present,
            "ALLOWED_EXTERNAL_ORIGINS entry {origin} does not appear in any of \
             connect-src, img-src, or font-src — either update the CSP or remove \
             the stale entry from ALLOWED_EXTERNAL_ORIGINS"
        );
    }
}

// Note: 'unsafe-inline' in script-src is intentionally retained while the
// SvelteKit hash-based replacement is pending (see TODO in lib.rs referencing
// issues #543 and #751).  No assertion is made about its presence or absence
// here to avoid the test becoming a no-op check once the TODO is resolved.
