//! Session cookie middleware, idempotency middleware, and per-request tracing middleware.
//!
//! Two session implementations live in this module:
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
//!
//! The [`idempotency_middleware`] handles the `Idempotency-Key` header for
//! mutating routes (#619).
//!
//! ## Request-ID middleware (`#621`)
//!
//! [`request_id_layer`] runs at the outermost layer of the middleware stack
//! (just inside the rate-limiter).  It:
//! 1. Mints a UUID v4 per inbound HTTP request.
//! 2. Injects a [`RequestId`] extension so route handlers can read it.
//! 3. Opens a `tracing` span with standard fields (`request_id`, `route`,
//!    `method`) and emits a structured `http.request` event with latency and
//!    status on completion.
//! 4. Echoes the id in the `X-Request-Id` response header so clients can
//!    correlate logs.
//!
//! The middleware is gated by the `otel-tracing` feature flag (default-on).
//! When the flag is disabled it is a simple pass-through.

use std::sync::Arc;
use std::time::Instant;

use axum::body::{Body, to_bytes};
use axum::extract::State;
use axum::http::{HeaderName, HeaderValue, Method, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_sessions::Session;
use uuid::Uuid;

use crate::session::{
    CachedResponse, GlobalState, IDEMPOTENCY_TTL, IdempotencyKey, get_or_create_session,
};

// ── Request-ID header name (stable, #621) ───────────────────────────────────

/// HTTP response header that echoes the per-request UUID back to the client.
pub static X_REQUEST_ID: std::sync::LazyLock<HeaderName> =
    std::sync::LazyLock::new(|| HeaderName::from_static("x-request-id"));

/// Per-request UUID injected into Axum extensions by [`request_id_layer`].
///
/// Route handlers that need to propagate the id downstream can extract it with:
///
/// ```ignore
/// Extension(req_id): Extension<RequestId>
/// ```
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// Axum middleware that mints a UUID per inbound HTTP request (#621).
///
/// Behaviour:
/// - Generates a UUID v4 via `uuid::Uuid::new_v4()`.
/// - Injects a [`RequestId`] into request extensions.
/// - Opens a `tracing` span for the request lifetime with structured fields.
/// - Records latency, HTTP method, route, and status on completion.
/// - Echoes the id in `X-Request-Id` on the response.
///
/// Gated by the `otel-tracing` flag on [`GlobalState`].  When the flag is
/// disabled the middleware is a zero-overhead pass-through.
pub async fn request_id_layer(
    State(global): State<Arc<GlobalState>>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Feature-flag gate: when `otel-tracing` is explicitly disabled, skip the
    // entire middleware to avoid any overhead for operators who opted out.
    if global.template_config.flags.is_disabled("otel-tracing") {
        return next.run(req).await;
    }

    let request_id = Uuid::new_v4().to_string();
    let route = req.uri().path().to_string();
    let method = req.method().as_str().to_string();

    // Inject the RequestId extension so downstream handlers can access it.
    req.extensions_mut().insert(RequestId(request_id.clone()));

    let start = Instant::now();

    // Open a tracing span for this request lifetime.  The span carries the
    // standard fields defined in docs/agent/tracing.md.
    let span = tracing::info_span!(
        "http.request",
        request_id = %request_id,
        route = %route,
        method = %method,
        // Placeholders recorded as empty strings; session middleware and route
        // handlers update them via `Span::current().record(...)` once known.
        session_id = tracing::field::Empty,
        account_id = tracing::field::Empty,
        model = tracing::field::Empty,
        status = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    );

    let mut response = {
        use tracing::Instrument as _;
        next.run(req).instrument(span.clone()).await
    };

    let latency_ms = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();

    // Record terminal fields on the span so they appear in OTel exporters.
    span.record("status", status);
    span.record("latency_ms", latency_ms);

    // Structured event for per-request metrics (latency, status, route).
    // Full Prometheus export is out of scope (#621); these events can be
    // scraped by log-based metric tooling today.
    tracing::info!(
        target: "parish_server::metrics",
        request_id = %request_id,
        route = %route,
        method = %method,
        status = status,
        latency_ms = latency_ms,
        "http.request.complete"
    );

    // Echo the request id in the response header.
    if let Ok(v) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(X_REQUEST_ID.clone(), v);
    }

    response
}

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

    let session_result = get_or_create_session(&global, cookie_id.as_deref()).await;
    let (session_id, entry, is_new) = match session_result {
        Ok(tuple) => tuple,
        Err(e) => {
            // Admission control: server is at capacity.
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "30")],
                format!(
                    "Server at capacity ({}/{} sessions). Retry after 30 seconds.",
                    e.current, e.cap
                ),
            )
                .into_response();
        }
    };

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
    req.extensions_mut().insert(SessionId(session_id.clone()));

    // #621 — Record session_id on the active request span so inference traces
    // and other downstream spans carry it without extra extraction.
    tracing::Span::current().record("session_id", &session_id as &str);

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

    let session_result = get_or_create_session(&global, cookie_id.as_deref()).await;
    let (session_id, entry, is_new) = match session_result {
        Ok(tuple) => tuple,
        Err(e) => {
            // Admission control: server is at capacity.
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "30")],
                format!(
                    "Server at capacity ({}/{} sessions). Retry after 30 seconds.",
                    e.current, e.cap
                ),
            )
                .into_response();
        }
    };

    // Inject the per-session AppState and session id as Axum extensions.
    req.extensions_mut().insert(Arc::clone(&entry.app_state));
    req.extensions_mut().insert(SessionId(session_id.clone()));

    // #621 — Record session_id on the active request span.
    tracing::Span::current().record("session_id", &session_id as &str);

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

// ── Idempotency-Key middleware (#619) ────────────────────────────────────────

/// Header name for the idempotency key sent by clients.
pub static IDEMPOTENCY_KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

/// Extension injected into the request so downstream handlers can read the
/// parsed idempotency key value if needed.
#[derive(Clone, Debug)]
pub struct IdempotencyKeyExt(pub String);

/// Axum middleware implementing `Idempotency-Key` replay for mutating routes.
///
/// # Behaviour
///
/// 1. Only runs on `POST`, `PUT`, `PATCH`, and `DELETE` requests.
/// 2. Reads the `Idempotency-Key` header.  If absent, the request passes
///    through unchanged.
/// 3. Checks the process-wide LRU cache (on [`GlobalState`]) for a prior
///    response keyed by `(session_id, idempotency_key)`.
/// 4. If a non-expired entry is found the cached response is returned
///    immediately — the handler body is **not** executed.  The
///    `Idempotency-Key` header is echoed back in the response.
/// 5. If no entry is found the request is forwarded to the handler.  On a
///    successful (2xx) response the response body is buffered and stored in
///    the cache for future replay.
///
/// # Feature flag
///
/// The middleware is disabled when the `idempotency-key` feature flag is
/// explicitly turned off (`config.flags.is_disabled("idempotency-key")`).
/// The flag is default-on per CLAUDE.md rule #6.
///
/// # TTL-controlled capacity
///
/// `IDEMPOTENCY_TTL` controls replay eligibility; the underlying LRU cache
/// evicts the least-recently-used entry when capacity is exceeded.
pub async fn idempotency_middleware(
    State(global): State<Arc<GlobalState>>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Only intercept mutating methods.
    let is_mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if !is_mutating {
        return next.run(req).await;
    }

    // Feature-flag guard: default-on; bail out if explicitly disabled.
    if global.template_config.flags.is_disabled("idempotency-key") {
        return next.run(req).await;
    }

    // Extract the Idempotency-Key header.
    let idem_key = match req.headers().get(&IDEMPOTENCY_KEY_HEADER) {
        Some(v) => match v.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                // Non-UTF-8 key — treat as absent.
                return next.run(req).await;
            }
        },
        None => return next.run(req).await,
    };

    // Derive a scope key from the session id injected by the session middleware.
    // The session middleware runs before idempotency middleware in the stack, so
    // the extension is always present for authenticated routes.
    let scope = match req.extensions().get::<crate::middleware::SessionId>() {
        Some(sid) => sid.0.clone(),
        None => {
            // No session (shouldn't happen on authenticated routes) — skip.
            return next.run(req).await;
        }
    };

    let cache_key: IdempotencyKey = (scope, idem_key.clone());

    // Inject the key as an extension so handlers can inspect it if needed.
    req.extensions_mut()
        .insert(IdempotencyKeyExt(idem_key.clone()));

    // Check for a cached response.
    {
        let mut cache = global.idempotency_cache.lock().await;
        if let Some(cached) = cache.get(&cache_key) {
            let age = Instant::now().duration_since(cached.inserted_at);
            if age <= IDEMPOTENCY_TTL {
                // Replay the cached response.
                let status = StatusCode::from_u16(cached.status)
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                let body_bytes = cached.body.clone();
                let content_type = cached.content_type.clone();

                tracing::debug!(
                    idempotency_key = %idem_key,
                    status = %status,
                    age_secs = age.as_secs(),
                    "idempotency_middleware: replaying cached response"
                );

                let mut builder = axum::http::Response::builder().status(status);
                if let Some(ref ct) = content_type
                    && let Ok(hv) = HeaderValue::from_str(ct)
                {
                    builder = builder.header(header::CONTENT_TYPE, hv);
                }
                if let Ok(hv) = HeaderValue::from_str(&idem_key) {
                    builder = builder.header(&IDEMPOTENCY_KEY_HEADER, hv);
                }
                return builder
                    .body(Body::from(body_bytes))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
            // Entry expired — remove it so a fresh execution is cached below.
            cache.pop(&cache_key);
        }
    }

    // Forward to the handler and capture the response.
    let response = next.run(req).await;
    let status = response.status();

    // Only cache successful responses.
    if status.is_success() {
        let (mut parts, body) = response.into_parts();

        // Buffer the body (cap at 1 MiB to guard against unexpectedly large
        // responses slipping into the cache).
        match to_bytes(body, 1024 * 1024).await {
            Ok(bytes) => {
                let content_type = parts
                    .headers
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);

                let entry = CachedResponse {
                    status: status.as_u16(),
                    body: bytes.to_vec(),
                    content_type,
                    inserted_at: Instant::now(),
                };
                global
                    .idempotency_cache
                    .lock()
                    .await
                    .put(cache_key, entry.clone());

                // Echo the key back in the response header.
                if let Ok(hv) = HeaderValue::from_str(&idem_key) {
                    parts.headers.insert(&IDEMPOTENCY_KEY_HEADER, hv);
                }

                axum::http::Response::from_parts(parts, Body::from(entry.body))
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "idempotency_middleware: failed to buffer response body — skipping cache"
                );
                // Return an error; we can't reconstruct the original body.
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    } else {
        response
    }
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
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use axum::Router;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::middleware as axum_mw;
    use axum::routing::post;
    use lru::LruCache;
    use tempfile::tempdir;
    use tower::ServiceExt;

    use super::*;
    use crate::session::{
        CachedResponse, IDEMPOTENCY_CACHE_CAPACITY, IDEMPOTENCY_TTL, IdempotencyKey,
        SessionRegistry,
    };
    use crate::session_store_impl::{SqliteIdentityStore, open_sessions_db};

    // ── Helper: minimal GlobalState for idempotency tests ───────────────────

    /// Builds a `GlobalState` with only the fields the idempotency middleware
    /// actually touches: `idempotency_cache` and `template_config.flags`.
    ///
    /// All other fields are stubs — the state is NOT suitable for any route
    /// handler that touches the game world, NPCs, or inference.
    fn test_global_state() -> Arc<crate::session::GlobalState> {
        let dir = tempdir().unwrap();
        let sessions = SessionRegistry::open(dir.path()).unwrap();
        // Keep tempdir alive for the lifetime of the returned Arc by leaking
        // it into a Box. This is intentional test-only simplification.
        Box::leak(Box::new(dir));

        let data_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        // Use a temp path for saves; leak the TempDir handle so it lives for
        // the duration of this test binary.
        let saves_tmp = Box::new(tempdir().unwrap());
        let saves_dir = saves_tmp.path().to_path_buf();
        Box::leak(saves_tmp);

        let world = parish_core::world::WorldState::from_parish_file(
            &data_dir.join("world.json"),
            parish_core::world::DEFAULT_START_LOCATION,
        )
        .unwrap();
        let npc_manager = parish_core::npc::manager::NpcManager::new();
        let transport = parish_core::world::transport::TransportConfig::default();
        let ui_config = crate::state::UiConfigSnapshot {
            hints_label: String::new(),
            default_accent: String::new(),
            splash_text: String::new(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            auto_pause_timeout_seconds: 60,
        };
        let theme_palette = parish_core::game_mod::default_theme_palette();
        let session_store: std::sync::Arc<dyn parish_core::session_store::SessionStore> =
            std::sync::Arc::new(crate::session_store_impl::DbSessionStore::new(
                saves_dir.clone(),
            ));
        let identity_conn = open_sessions_db(&saves_dir).unwrap();
        let identity_store: std::sync::Arc<dyn parish_core::identity::IdentityStore> =
            std::sync::Arc::new(SqliteIdentityStore::new(identity_conn));
        let app_state = crate::state::build_app_state(
            "test-session".to_string(),
            world,
            npc_manager,
            None,
            crate::state::GameConfig {
                provider_name: String::new(),
                base_url: String::new(),
                api_key: None,
                model_name: String::new(),
                cloud_provider_name: None,
                cloud_model_name: None,
                cloud_api_key: None,
                cloud_base_url: None,
                improv_enabled: false,
                max_follow_up_turns: 2,
                idle_banter_after_secs: 25,
                auto_pause_after_secs: 60,
                category_provider: Default::default(),
                category_model: Default::default(),
                category_api_key: Default::default(),
                category_base_url: Default::default(),
                flags: parish_core::config::FeatureFlags::default(),
                category_rate_limit: Default::default(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                reveal_unexplored_locations: false,
            },
            None,
            transport,
            ui_config,
            theme_palette,
            saves_dir.clone(),
            data_dir.clone(),
            None,
            data_dir.join("parish-flags.json"),
            parish_core::config::InferenceConfig::default(),
            session_store,
        );

        let tile_cache = parish_core::tile_cache::TileCache::new(
            saves_dir.join("tile-cache"),
            Default::default(),
        );

        Arc::new(crate::session::GlobalState {
            sessions,
            identity_store,
            oauth_config: None,
            data_dir,
            world_path: std::path::PathBuf::from("/dev/null"),
            saves_dir,
            game_mod: None,
            pronunciations: Vec::new(),
            ui_config: app_state.ui_config.clone(),
            theme_palette: app_state.theme_palette.clone(),
            transport: parish_core::world::transport::TransportConfig::default(),
            template_config: crate::state::GameConfig {
                provider_name: String::new(),
                base_url: String::new(),
                api_key: None,
                model_name: String::new(),
                cloud_provider_name: None,
                cloud_model_name: None,
                cloud_api_key: None,
                cloud_base_url: None,
                improv_enabled: false,
                max_follow_up_turns: 2,
                idle_banter_after_secs: 25,
                auto_pause_after_secs: 60,
                category_provider: Default::default(),
                category_model: Default::default(),
                category_api_key: Default::default(),
                category_base_url: Default::default(),
                flags: parish_core::config::FeatureFlags::default(),
                category_rate_limit: Default::default(),
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                reveal_unexplored_locations: false,
            },
            inference_config: parish_core::config::InferenceConfig::default(),
            ollama_process: tokio::sync::Mutex::new(
                parish_core::inference::client::OllamaProcess::none(),
            ),
            tile_cache,
            idempotency_cache: tokio::sync::Mutex::new(LruCache::new(
                NonZeroUsize::new(IDEMPOTENCY_CACHE_CAPACITY).unwrap(),
            )),
            max_concurrent_sessions: None,
        })
    }

    /// Builds a minimal test router: session_id is injected as a fixed
    /// extension so the idempotency middleware can scope cache keys.
    ///
    /// Layer ordering (outermost → innermost → handler):
    ///   session_id_injector → idempotency_middleware → handler
    ///
    /// In Axum, the LAST `.layer()` call applied is outermost (runs first).
    /// So to have session run before idempotency, we apply idempotency first
    /// (inner) and session_id_injector second (outer).
    fn test_router_with_handler(
        global: Arc<crate::session::GlobalState>,
        call_count: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Router {
        let cc = call_count.clone();
        Router::new()
            .route(
                "/test",
                post(move || {
                    let cc = cc.clone();
                    async move {
                        cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        axum::Json(serde_json::json!({"ok": true}))
                    }
                }),
            )
            // Inner: idempotency reads SessionId (injected by outer layer).
            .layer(axum_mw::from_fn_with_state(global, idempotency_middleware))
            // Outer: inject a synthetic SessionId before idempotency runs.
            .layer(axum_mw::from_fn(
                |mut req: Request<Body>, next: Next| async move {
                    req.extensions_mut()
                        .insert(SessionId("test-session".to_string()));
                    next.run(req).await
                },
            ))
    }

    // ── Unit tests ──────────────────────────────────────────────────────────

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

    #[test]
    fn idempotency_key_header_name_is_stable() {
        // The header name must remain `idempotency-key` (lowercase) to be
        // consistent with the IETF draft and clients that send it.
        assert_eq!(IDEMPOTENCY_KEY_HEADER.as_str(), "idempotency-key");
    }

    #[test]
    fn idempotency_defaults() {
        // Capacity and TTL should remain stable across refactors — they
        // are documented in architecture.md and callers depend on them.
        assert_eq!(IDEMPOTENCY_CACHE_CAPACITY, 1000);
        assert_eq!(IDEMPOTENCY_TTL, Duration::from_secs(24 * 60 * 60));
    }

    // ── Integration tests — same key returns identical response ─────────────

    /// Same `Idempotency-Key` on two successive POST requests:
    /// - both return the same body
    /// - the handler body only executes once (call_count == 1)
    /// - the response echoes the key header
    #[tokio::test]
    async fn same_idempotency_key_returns_cached_response_and_does_not_re_execute() {
        let global = test_global_state();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = test_router_with_handler(Arc::clone(&global), Arc::clone(&call_count));

        let make_req = || {
            axum::http::Request::builder()
                .method("POST")
                .uri("/test")
                .header("idempotency-key", "key-abc")
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap()
        };

        let resp1 = app.clone().oneshot(make_req()).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);
        let body1 = to_bytes(resp1.into_body(), 1024).await.unwrap();

        let resp2 = app.oneshot(make_req()).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);
        // Response header echoed back.
        assert_eq!(
            resp2
                .headers()
                .get("idempotency-key")
                .and_then(|v| v.to_str().ok()),
            Some("key-abc")
        );
        let body2 = to_bytes(resp2.into_body(), 1024).await.unwrap();

        // Bodies are byte-identical.
        assert_eq!(body1, body2);
        // Handler executed exactly once.
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// Different `Idempotency-Key` values each execute the handler body.
    #[tokio::test]
    async fn different_idempotency_keys_each_execute_handler() {
        let global = test_global_state();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = test_router_with_handler(Arc::clone(&global), Arc::clone(&call_count));

        let make_req = |key: &'static str| {
            axum::http::Request::builder()
                .method("POST")
                .uri("/test")
                .header("idempotency-key", key)
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap()
        };

        let _ = app.clone().oneshot(make_req("key-1")).await.unwrap();
        let _ = app.oneshot(make_req("key-2")).await.unwrap();

        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// Request without the `Idempotency-Key` header always executes the handler.
    #[tokio::test]
    async fn no_idempotency_key_always_executes() {
        let global = test_global_state();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = test_router_with_handler(Arc::clone(&global), Arc::clone(&call_count));

        let make_req = || {
            axum::http::Request::builder()
                .method("POST")
                .uri("/test")
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap()
        };

        let _ = app.clone().oneshot(make_req()).await.unwrap();
        let _ = app.oneshot(make_req()).await.unwrap();

        // Both calls executed because there was no idempotency key.
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// After TTL expiry a second request re-executes the handler.
    ///
    /// We simulate expiry by directly inserting a cache entry with a past
    /// `inserted_at` rather than sleeping for 24 h.
    #[tokio::test]
    async fn expired_entry_re_executes_handler() {
        let global = test_global_state();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Pre-populate the cache with an expired entry (inserted 25 h ago).
        {
            let mut cache = global.idempotency_cache.lock().await;
            let key: IdempotencyKey = ("test-session".to_string(), "key-expired".to_string());
            cache.put(
                key,
                CachedResponse {
                    status: 200,
                    body: br#"{"cached":true}"#.to_vec(),
                    content_type: Some("application/json".to_string()),
                    inserted_at: Instant::now()
                        .checked_sub(IDEMPOTENCY_TTL + Duration::from_secs(3600))
                        .unwrap(),
                },
            );
        }

        let app = test_router_with_handler(Arc::clone(&global), Arc::clone(&call_count));
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/test")
            .header("idempotency-key", "key-expired")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        // The live handler executed (not the stale cached body).
        assert!(body.starts_with(b"{\"ok\""));
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// GET requests are never intercepted by the idempotency middleware.
    #[tokio::test]
    async fn get_requests_bypass_idempotency() {
        let global = test_global_state();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Use a GET handler for this test.
        let cc = Arc::clone(&call_count);
        let app = Router::new()
            .route(
                "/test",
                axum::routing::get(move || {
                    let cc = cc.clone();
                    async move {
                        cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        axum::Json(serde_json::json!({"ok": true}))
                    }
                }),
            )
            // Inner layer first (idempotency), then outer (session injector).
            .layer(axum_mw::from_fn_with_state(global, idempotency_middleware))
            .layer(axum_mw::from_fn(
                |mut req: Request<Body>, next: Next| async move {
                    req.extensions_mut()
                        .insert(SessionId("test-session".to_string()));
                    next.run(req).await
                },
            ));

        let make_get = || {
            axum::http::Request::builder()
                .method("GET")
                .uri("/test")
                .header("idempotency-key", "key-get")
                .body(Body::empty())
                .unwrap()
        };

        let _ = app.clone().oneshot(make_get()).await.unwrap();
        let _ = app.oneshot(make_get()).await.unwrap();

        // Both GETs executed — middleware did not cache.
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}
