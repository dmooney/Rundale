//! Parish web server — serves the Svelte UI in a browser via axum.
//!
//! Provides the same game experience as the Tauri desktop app, but over
//! standard HTTP + WebSocket so it can run in any browser.
//!
//! Each browser visitor gets their own isolated game session, identified by
//! a `parish_sid` cookie.  Sessions are persisted across server restarts via
//! `saves/<session_id>/` directories and `saves/sessions.db`.

pub mod auth;
pub mod cf_auth;
pub mod command_host;
pub mod drain;
pub mod editor_routes;
pub mod emitter;
pub mod lock_metrics;
pub mod middleware;
pub mod route_registry;
pub mod routes;
pub mod session;
pub mod session_store_impl;
pub mod state;
pub mod sync_routes;
pub mod sync_types;
pub mod tile_routes;
pub mod tracing_setup;
pub mod ws;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

// Inline-script SHA-256 hashes extracted from apps/ui/dist/**/*.html at build
// time by build.rs.  Declares `SCRIPT_SRC_HASHES: &[&str]`.
include!(concat!(env!("OUT_DIR"), "/csp_script_hashes.rs"));

use axum::Router;
use axum::extract::ConnectInfo;
use axum::http::header::{
    CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware as axum_mw;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use parish_core::game_mod::GameMod;
use parish_core::ipc::ThemePalette;
use parish_core::mod_source::{LocalDiskModSource, ModSource};
use parish_core::world::transport::TransportConfig;

use parish_core::config::FeatureFlags;
use session::{GlobalState, OAuthConfig, SessionRegistry};
use session_store_impl::{SqliteIdentityStore, open_sessions_db};
use state::{GameConfig, UiConfigSnapshot};

/// Specific HTTPS origins the frontend genuinely connects to or loads images
/// from.  These are the only external endpoints referenced in
/// `apps/ui/src/` — everything else is same-origin (`'self'`).
///
/// - `https://tile.openstreetmap.org` — default OSM raster tile source
///   (`parish-config` `default_tile_sources`).
/// - `https://demotiles.maplibre.org` — MapLibre glyph PBFs used by the
///   map label layer (`style.ts` `GLYPHS_URL`).
/// - `https://fonts.googleapis.com` — Google Fonts CSS `<link>` in `app.html`.
/// - `https://fonts.gstatic.com` — Google Fonts glyph files (font-src).
///
/// NLS historic tiles (`mapseries-tilesets.s3.amazonaws.com`) are now proxied
/// through `/tiles/{source_id}/{z}/{x}/{y}.png` (issue #360), so the S3
/// origin no longer needs to appear in the CSP.
///
/// Update this list whenever `apps/ui/src/` gains a new external dependency.
/// Keeping it in a dedicated constant lets the security-headers test assert
/// membership without repeating the origin strings.
pub const ALLOWED_EXTERNAL_ORIGINS: &[&str] = &[
    "https://tile.openstreetmap.org",
    "https://demotiles.maplibre.org",
    "https://fonts.googleapis.com",
    "https://fonts.gstatic.com",
];

/// Content-Security-Policy value shared between production and tests.
///
/// # connect-src and img-src
///
/// The bare `https:` wildcard has been replaced with only the specific HTTPS
/// origins the frontend actually uses (see [`ALLOWED_EXTERNAL_ORIGINS`]).
/// This addresses issue #751.
///
/// MapLibre fetches tiles via `fetch()` (CORS → connect-src) AND renders them
/// as raster images (img-src), so the tile-server origins appear in both
/// directives.  MapLibre also fetches glyph PBFs at runtime for the label
/// layer, so `demotiles.maplibre.org` is in connect-src as well.
///
/// # script-src: hash-based allowlist (TD-036, #543)
///
/// `'unsafe-inline'` has been removed.  Instead, `build.rs` extracts the
/// SHA-256 digest of every inline `<script>` block emitted by the SvelteKit
/// static-adapter build and records them in `SCRIPT_SRC_HASHES`.  Those
/// hashes are included here so the browser accepts the SvelteKit bootstrap
/// script while rejecting all other inline code.
///
/// The hash list is regenerated automatically whenever `cargo build` detects
/// that `apps/ui/dist` has changed (via the `rerun-if-changed` directive in
/// `build.rs`).
pub static CSP_POLICY: LazyLock<String> = LazyLock::new(|| build_csp_policy(SCRIPT_SRC_HASHES));

/// Builds the Content-Security-Policy header value from a slice of
/// `'sha256-<base64>'` tokens produced by `build.rs`.
///
/// Extracted from the [`CSP_POLICY`] initialiser so that unit tests can
/// exercise the full policy string with synthetic hashes without depending
/// on the build-time `SCRIPT_SRC_HASHES` constant (which is an empty slice
/// in test environments where `apps/ui/dist` has not been built).
pub(crate) fn build_csp_policy(script_hashes: &[&str]) -> String {
    // Build the script-src directive.  Always include 'self' for module scripts
    // loaded via <script src>.  Then append each hash token emitted by build.rs.
    let mut script_src = String::from("'self'");
    for hash in script_hashes {
        script_src.push(' ');
        script_src.push_str(hash);
    }

    format!(
        "default-src 'self'; \
         script-src {script_src}; \
         worker-src 'self' blob:; \
         style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; \
         img-src 'self' data: blob: https://tile.openstreetmap.org; \
         connect-src 'self' ws: wss: https://tile.openstreetmap.org https://demotiles.maplibre.org https://fonts.googleapis.com; \
         font-src 'self' https://fonts.gstatic.com; \
         frame-ancestors 'none'; \
         base-uri 'self'; \
         form-action 'self'"
    )
}

// ── GPL-3.0 redistribution: licence files served alongside the hosted web
//    build.  Tauri bundles ship these via `tauri.conf.json` →
//    `bundle.resources`; the Axum server embeds them at compile time via
//    `include_str!` and serves them from the corresponding routes wired up
//    in `run_server` (mounted *after* `cf_access_guard` so they remain
//    publicly reachable).
pub async fn serve_license() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        include_str!("../../../../LICENSE"),
    )
}

pub async fn serve_notice() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        include_str!("../../../../NOTICE"),
    )
}

pub async fn serve_third_party_notices() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/markdown; charset=utf-8")],
        include_str!("../../../../THIRD_PARTY_NOTICES.md"),
    )
}

/// Applies the production security-response-header stack to `router`.
///
/// All six `SetResponseHeaderLayer` entries are defined here once and used by
/// both [`run_server`] (production path) and the `security_headers` integration
/// tests, so the tests exercise the real header values rather than a hand-rolled
/// duplicate.
pub fn apply_security_layers(router: Router) -> Router {
    // CSP_POLICY is a LazyLock<String> — parse it into a HeaderValue once at
    // startup.  `from_str` only fails for characters outside ISO-8859-1, which
    // the CSP value does not contain.
    let csp_value =
        HeaderValue::from_str(CSP_POLICY.as_str()).expect("CSP_POLICY is a valid header value");
    router
        .layer(SetResponseHeaderLayer::overriding(
            CONTENT_SECURITY_POLICY,
            csp_value,
        ))
        .layer(SetResponseHeaderLayer::overriding(
            STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
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

/// Global auth-failure counter — exposed via `GET /metrics`.
static AUTH_FAILURES: AtomicU64 = AtomicU64::new(0);

/// Injects an `AuthContext` into request extensions after resolving the
/// stable `account_id` for the given email.  Shared by the three auth paths
/// in [`cf_access_guard`] (loopback bypass, JWT success, debug fallback).
fn inject_auth_context(
    req: &mut Request<axum::body::Body>,
    identity_store: &dyn parish_core::identity::IdentityStore,
    email: String,
    flags: &parish_core::config::FeatureFlags,
) {
    let account_id = resolve_account_id(identity_store, &email, flags);
    tracing::Span::current().record("account_id", account_id.to_string().as_str());
    req.extensions_mut()
        .insert(cf_auth::AuthContext { account_id, email });
}

/// Resolves or mints a stable `account_id` for the given CF-Access email.
///
/// On first encounter the email is registered as a new account via
/// `IdentityStore::create_account`; on subsequent requests the existing ID is
/// returned via `lookup_by_provider`.  Both operations are idempotent (#618).
///
/// When the `account-id-keying` feature flag is disabled (kill-switch) the
/// function derives a deterministic UUID from the email bytes so every user
/// still gets a unique, stable identity without touching the persistent store.
/// This preserves per-user isolation while allowing a rollback without
/// redeploying a new binary.
fn resolve_account_id(
    identity_store: &dyn parish_core::identity::IdentityStore,
    email: &str,
    flags: &parish_core::config::FeatureFlags,
) -> uuid::Uuid {
    // Kill-switch path: deterministic UUID derived from email bytes.
    // XOR-fold is collision-resistant for our user population and produces
    // a unique, non-nil UUID per email without any DB access.
    if flags.is_disabled("account-id-keying") {
        let bytes = email.as_bytes();
        let mut buf = [0u8; 16];
        for (i, &b) in bytes.iter().enumerate() {
            buf[i % 16] ^= b;
        }
        // Set version (4) and variant bits so the UUID is well-formed.
        buf[6] = (buf[6] & 0x0f) | 0x40;
        buf[8] = (buf[8] & 0x3f) | 0x80;
        return uuid::Uuid::from_bytes(buf);
    }

    // Use the CF-Access email as the provider_user_id under a synthetic
    // "cf-access" provider.  This keeps the schema consistent with Google
    // OAuth rows while supporting CF-Access-only deployments.
    const PROVIDER: &str = "cf-access";

    if let Some(existing_id) = identity_store.lookup_by_provider(PROVIDER, email) {
        if let Ok(id) = uuid::Uuid::parse_str(&existing_id) {
            return id;
        }
        tracing::warn!(
            email = %email,
            raw = %existing_id,
            "resolve_account_id: stored id is not a valid UUID, minting a new one"
        );
    }

    // New account — mint a UUID, persist it, then link the provider identity.
    let new_id = uuid::Uuid::new_v4();
    let id_str = new_id.to_string();
    identity_store.create_account(&id_str);
    identity_store.link_provider(PROVIDER, email, &id_str, email);
    tracing::info!(
        account_id = %new_id,
        email = %email,
        "resolve_account_id: minted new account"
    );
    new_id
}

/// Middleware that enforces Cloudflare Access authentication.
///
/// **Production** (`CF_ACCESS_AUD` env set): validates `Cf-Access-Jwt-Assertion`
/// against the team JWKS; injects [`cf_auth::AuthContext`] into request extensions.
///
/// **Debug-only fallback** (`debug_assertions` + loopback): skipped entirely so
/// local dev works without a Cloudflare tunnel.
///
/// **Fail-closed**: if `CF_ACCESS_AUD` is unset in a release build, every
/// request returns 401 to avoid running unauthenticated in production.
///
/// **#618** — after JWT validation the guard resolves a stable `account_id` UUID
/// via [`resolve_account_id`] and stores it alongside the email in the injected
/// [`cf_auth::AuthContext`].
async fn cf_access_guard(
    axum::extract::State(global): axum::extract::State<Arc<session::GlobalState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // #379 — loopback bypass is debug-only.
    if cfg!(debug_assertions) && addr.ip().is_loopback() {
        inject_auth_context(
            &mut req,
            global.identity_store.as_ref(),
            "dev@localhost".to_string(),
            &global.template_config.flags,
        );
        return Ok(next.run(req).await);
    }

    // #373 — only /api/health is exempt; /api/ui-config must be authenticated.
    if req.uri().path() == "/api/health" {
        return Ok(next.run(req).await);
    }

    // #276 — real JWT validation path.
    if let Some(verifier) = cf_auth::global_verifier() {
        let jwt_header = req
            .headers()
            .get("Cf-Access-Jwt-Assertion")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        match jwt_header {
            Some(token) => match verifier.validate(&token).await {
                Ok(email) => {
                    inject_auth_context(
                        &mut req,
                        global.identity_store.as_ref(),
                        email,
                        &global.template_config.flags,
                    );
                    return Ok(next.run(req).await);
                }
                Err(e) => {
                    tracing::warn!(source_ip = %addr, error = %e, "cf_access_guard: 401 — JWT validation failed");
                    AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
                    return Err(StatusCode::UNAUTHORIZED);
                }
            },
            None => {
                tracing::warn!(source_ip = %addr, "cf_access_guard: 401 — missing Cf-Access-Jwt-Assertion");
                AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    // #276 — no verifier: CF_ACCESS_AUD unset.
    #[cfg(not(debug_assertions))]
    {
        // Fail closed in release builds — no token infrastructure configured.
        tracing::error!(
            source_ip = %addr,
            "cf_access_guard: CF_ACCESS_AUD not set in release build — rejecting all requests"
        );
        AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Debug build with no verifier: fall back to header-presence check.
    #[cfg(debug_assertions)]
    {
        let debug_email: Option<String> = req
            .headers()
            .get("CF-Access-Authenticated-User-Email")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .filter(|e| !e.is_empty() && e.contains('@'));
        if let Some(email) = debug_email {
            inject_auth_context(
                &mut req,
                global.identity_store.as_ref(),
                email,
                &global.template_config.flags,
            );
            return Ok(next.run(req).await);
        }
        tracing::warn!(source_ip = %addr, "cf_access_guard: 401 — debug fallback, missing CF-Access-Authenticated-User-Email");
        AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Starts the Parish web server on the given port.
///
/// When `headless_models` is true the server brings up (or detect-reuses) the
/// bundled local vllm-mlx Qwen two-slot loadout and binds the four inference
/// categories to it, so `POST /api/command` produces real NPC dialogue with no
/// desktop app (#1364). Otherwise the provider is resolved from env/presets as
/// before.
pub async fn run_server(
    port: u16,
    data_dir: PathBuf,
    static_dir: PathBuf,
    headless_models: bool,
) -> anyhow::Result<()> {
    handle_dotenv();
    let world_path = resolve_world_path(&data_dir);

    // ── LLM client + config (template, cloned per session) ───────────────────
    let (provider_cfg, config) = build_client_and_config(headless_models);
    let (config, runtime_processes) = run_llm_bootstrap(provider_cfg, config).await?;

    // ── Game mod / engine config / UI config ──────────────────────────────────
    let game_mod: Option<GameMod> = load_base_mod_via_source().await;
    let (splash_text, theme_palette) = resolve_splash_and_theme(&game_mod);

    let engine_config_path = parish_core::config::resolve_config_path(&data_dir);
    let engine_config = parish_core::config::load_engine_config(&engine_config_path);
    let (mut config, _tile_sources_snapshot, _active_tile_source, ui_config) =
        resolve_engine_and_ui_config(
            config,
            &engine_config,
            &game_mod,
            &splash_text,
            &theme_palette,
        );

    // ── Feature flags / session infrastructure / OAuth / WS key ─────────────
    let flags_path = data_dir.join("parish-flags.json");
    config.flags = FeatureFlags::load_from_file(&flags_path);

    // ── Saves directory ───────────────────────────────────────────────────────
    // App-name drives the per-user data folder (saves + tile cache). It comes
    // from the active mod (Rundale → `Rundale`); engine fallback is `Parish`.
    // Shared helper in parish-core keeps the three entry points in lockstep
    // (rule #12 — no copy-paste of cross-runtime orchestration).
    let app_name = parish_core::game_mod::app_name_from_mod(&game_mod);
    let saves_dir = parish_core::persistence::picker::resolve_project_saves_dir(&app_name);
    let (sessions, identity_store, pronunciations) =
        open_session_components(&saves_dir, &game_mod)?;
    let oauth_config = build_oauth_config();
    if oauth_config.is_some() {
        tracing::info!("Google OAuth enabled");
    }
    check_ws_signing_key_warning();

    // ── Tile cache / admission control / GlobalState ────────────────────────
    let tile_cache = init_tile_cache(&saves_dir, &app_name, &data_dir, &engine_config).await;
    let max_concurrent_sessions = resolve_admission_control(&config, &engine_config);
    let global = Arc::new(GlobalState {
        sessions,
        identity_store,
        oauth_config,
        data_dir: data_dir.clone(),
        world_path,
        saves_dir,
        game_mod,
        pronunciations,
        ui_config,
        theme_palette,
        transport: TransportConfig::default(),
        template_config: config,
        inference_config: engine_config.inference,
        runtime_processes: tokio::sync::Mutex::new(runtime_processes),
        tile_cache,
        idempotency_cache: {
            use std::num::NonZeroUsize;
            tokio::sync::Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(session::IDEMPOTENCY_CACHE_CAPACITY).unwrap(),
            ))
        },
        max_concurrent_sessions,
    });

    // ── Background tasks / middleware infrastructure ────────────────────────
    spawn_session_cleanup_background_task(&global);
    let ip_limiter = build_ip_rate_limiter_state();
    let use_tower_sessions = should_use_tower_sessions(&global);

    // ── Build router, apply layers, serve ────────────────────────────────────
    let oauth_enabled = global.oauth_config.is_some();
    let app = build_router(oauth_enabled, use_tower_sessions);
    let app = attach_static_and_auth(app, &global, &static_dir);
    let app = apply_session_layer(app, &global, use_tower_sessions);
    let app = apply_outer_layers(app, &global, ip_limiter);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Parish web server listening on http://{}", addr);
    tracing::info!("Serving static files from {}", static_dir.display());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// Registers all API, editor, WebSocket, tile-proxy, and auth routes.
///
/// Returns a bare [`Router`] without middleware or state — callers attach those
/// via [`attach_static_and_auth`], [`apply_session_layer`], and
/// [`apply_outer_layers`].
///
/// OAuth routes are conditionally added based on `oauth_enabled`.  When OAuth
/// is enabled, the correct variant (tower-sessions or legacy) is selected via
/// `use_tower_sessions` so the callback and logout handlers match the session
/// machinery installed by [`apply_session_layer`].
fn build_router(
    oauth_enabled: bool,
    use_tower_sessions: bool,
) -> Router<Arc<session::GlobalState>> {
    let mut app = Router::new()
        // #373 — /api/health: exempt from auth, returns 200 for health probes.
        .route("/api/health", get(routes::get_health))
        // #373 — /metrics: auth-protected, exposes auth failure counter.
        .route("/metrics", get(get_metrics))
        // #377 — /api/session-init: issues short-lived WS session tokens.
        .route("/api/session-init", post(routes::session_init))
        .route("/api/world-snapshot", get(routes::get_world_snapshot))
        .route("/api/setup-snapshot", get(routes::get_setup_snapshot))
        .route("/api/map", get(routes::get_map))
        .route("/api/npcs-here", get(routes::get_npcs_here))
        .route("/api/engine-state", get(routes::get_engine_state))
        .route("/api/theme", get(routes::get_theme))
        .route(
            "/api/list-available-providers",
            get(routes::get_available_providers),
        )
        .route("/api/ui-config", get(routes::get_ui_config))
        .route("/api/app-icon.png", get(routes::get_app_icon))
        .route("/api/favicon.png", get(routes::get_favicon))
        .route("/api/debug-snapshot", get(routes::get_debug_snapshot))
        .route("/api/turn", get(routes::get_turn))
        .route("/api/submit-input", post(routes::submit_input))
        .route("/api/react-to-message", post(routes::react_to_message))
        .route("/api/discover-save-files", get(routes::discover_save_files))
        .route("/api/save-game", post(routes::save_game))
        .route("/api/load-branch", post(routes::load_branch))
        .route("/api/create-branch", post(routes::create_branch))
        .route("/api/new-save-file", post(routes::new_save_file))
        .route("/api/new-game", post(routes::new_game))
        .route("/api/save-state", get(routes::get_save_state))
        .route("/api/submit-bug-report", post(routes::submit_bug_report))
        .route("/api/mods", get(routes::list_mods))
        .route("/api/mods/switch", post(routes::switch_mod))
        // ── Demo routes (desktop-only feature; server returns 501) ──────────
        .route("/api/demo-config", get(routes::get_demo_config))
        .route("/api/demo-context", get(routes::get_demo_context))
        .route(
            "/api/llm-player-action",
            post(routes::get_llm_player_action),
        )
        // ── Screenshot stubs (Tauri-only feature; server returns 501) ───────
        .route("/api/save-screenshot", post(routes::save_screenshot))
        .route("/api/latest-screenshot", get(routes::get_latest_screenshot))
        .route("/api/take-screenshot", post(routes::take_screenshot))
        // ── Synchronous command/state endpoints (thin clients, agents) ─────
        .route("/api/command", post(sync_routes::post_command))
        .route("/api/state", get(sync_routes::get_state))
        .route("/api/ws", get(ws::ws_handler))
        // ── Tile proxy (issue #360) ──────────────────────────────────────
        // Requires a valid session (session_middleware already in the stack).
        // `source_id` is validated against the registered tile sources.
        //
        // Route uses a wildcard (`*path`) because axum 0.8 does not allow a
        // static suffix (`.png`) in the same path segment as a parameter
        // (`{y}`).  The handler parses `source_id/z/x/y.png` from the
        // captured wildcard string.
        .route("/tiles/{*path}", get(tile_routes::get_tile))
        // ── Editor routes (Parish Designer) ─────────────────────────────
        // #376 — update endpoints carry a 256 KiB body limit.
        .route(
            "/api/editor-list-mods",
            get(editor_routes::editor_list_mods),
        )
        .route("/api/editor-open-mod", post(editor_routes::editor_open_mod))
        .route(
            "/api/editor-get-snapshot",
            get(editor_routes::editor_get_snapshot),
        )
        .route("/api/editor-validate", get(editor_routes::editor_validate))
        .route(
            "/api/editor-update-npcs",
            post(editor_routes::editor_update_npcs)
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/editor-update-locations",
            post(editor_routes::editor_update_locations)
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/editor-save",
            post(editor_routes::editor_save)
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route("/api/editor-reload", post(editor_routes::editor_reload))
        .route("/api/editor-close", post(editor_routes::editor_close))
        .route(
            "/api/editor-list-saves",
            get(editor_routes::editor_list_saves),
        )
        .route(
            "/api/editor-list-branches",
            post(editor_routes::editor_list_branches)
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/editor-list-snapshots",
            post(editor_routes::editor_list_snapshots)
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/editor-read-snapshot",
            post(editor_routes::editor_read_snapshot)
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route("/api/auth/status", get(auth::get_auth_status));

    if oauth_enabled {
        if use_tower_sessions {
            app = app
                .route("/auth/login/google", get(auth::login_google_tower))
                .route("/auth/callback/google", get(auth::callback_google_tower))
                .route("/auth/logout", get(auth::logout_tower));
        } else {
            app = app
                .route("/auth/login/google", get(auth::login_google))
                .route("/auth/callback/google", get(auth::callback_google))
                .route("/auth/logout", get(auth::logout));
        }
    }

    app
}

/// Attaches the static-file fallback service, the CF-Access auth guard, and
/// the shared [`GlobalState`] to `router`.
///
/// After this call the router is typed as `Router` (state erased) and is ready
/// for session-layer wrapping.
///
/// The SvelteKit adapter-static `fallback: 'index.html'` setting means
/// client-side routes such as `/editor` rely on the server returning the SPA
/// shell for any path ServeDir cannot satisfy.  Without the fallback service,
/// `/editor` 404s and the Playwright e2e suite's Editor tests time out waiting
/// for elements that never render.
fn attach_static_and_auth(
    router: Router<Arc<session::GlobalState>>,
    global: &Arc<session::GlobalState>,
    static_dir: &Path,
) -> Router {
    router
        .fallback_service(
            ServeDir::new(static_dir)
                .append_index_html_on_directories(true)
                .not_found_service(ServeFile::new(static_dir.join("index.html"))),
        )
        .layer(axum_mw::from_fn_with_state(
            Arc::clone(global),
            cf_access_guard,
        ))
        .with_state(Arc::clone(global))
}

/// Wraps `router` with the session-cookie middleware stack.
///
/// Two paths are supported:
///
/// - **tower-sessions** (default, `use_tower_sessions = true`): installs a
///   [`tower_sessions::MemoryStore`]-backed [`tower_sessions::SessionManagerLayer`]
///   using the existing `parish_sid` cookie name, then wraps that with
///   `session_middleware_tower` and `idempotency_middleware`.
///
/// - **legacy** (`use_tower_sessions = false`): wraps with the hand-rolled
///   `session_middleware` and `idempotency_middleware` only.
///
/// In Tower/Axum the last `.layer()` is outermost, so idempotency is applied
/// first (inner), then the session layers wrap it.  This ordering is preserved
/// exactly from the original inline code; altering it changes which extensions
/// are visible to which middleware.
fn apply_session_layer(
    router: Router,
    global: &Arc<session::GlobalState>,
    use_tower_sessions: bool,
) -> Router {
    if use_tower_sessions {
        use tower_sessions::cookie::SameSite;
        use tower_sessions::cookie::time::Duration as CookieDuration;
        use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

        // `MemoryStore` does not implement `ExpiredDeletion`, so expired
        // entries are only filtered on read (via `is_active`).  There is no
        // background cleanup task because the backing map is not publicly
        // accessible.  Instead we rely on the 365-day `Expiry` set by
        // `SessionManagerLayer` below to bound memory growth.
        //
        // TODO: replace `MemoryStore` with a store that implements
        // `ExpiredDeletion` (e.g. `tower-sessions-sqlx-store`) if
        // long-running deployments reveal significant memory pressure.
        let session_store = std::sync::Arc::new(MemoryStore::default());
        let session_layer = SessionManagerLayer::new((*session_store).clone())
            .with_name(middleware::SESSION_COOKIE)
            .with_secure(true)
            .with_http_only(true)
            .with_same_site(SameSite::Lax)
            .with_path("/".to_string())
            .with_expiry(Expiry::OnInactivity(CookieDuration::days(365)));

        router
            // Idempotency middleware runs after session (session injects SessionId
            // extension; idempotency reads it).  In Tower/Axum the last `.layer()`
            // is outermost — so idempotency is applied first here (inner), then
            // the session layers wrap it.
            .layer(axum_mw::from_fn_with_state(
                Arc::clone(global),
                middleware::idempotency_middleware,
            ))
            .layer(axum_mw::from_fn_with_state(
                Arc::clone(global),
                middleware::session_middleware_tower,
            ))
            .layer(session_layer)
    } else {
        router
            .layer(axum_mw::from_fn_with_state(
                Arc::clone(global),
                middleware::idempotency_middleware,
            ))
            .layer(axum_mw::from_fn_with_state(
                Arc::clone(global),
                middleware::session_middleware,
            ))
    }
}

/// Adds the outermost layers to `router`: legal/licence routes, per-request
/// tracing, global IP rate limiter, and security-hardening response headers.
///
/// Layer order is security-sensitive and must be preserved exactly:
///
/// 1. Legal routes (`/LICENSE`, `/NOTICE`, `/THIRD_PARTY_NOTICES.md`) —
///    mounted *after* `cf_access_guard` and `session_middleware` so they
///    remain publicly reachable while the rate-limit and security headers
///    still apply.
/// 2. `request_id_layer` — per-request tracing.  Runs *inside* the rate
///    limiter so only admitted requests are traced.
/// 3. `ip_rate_limit_middleware` — outside the auth guard; throttles floods
///    before JWT validation overhead is incurred (#381).
/// 4. `apply_security_layers` — outermost; covers every route.
fn apply_outer_layers(
    router: Router,
    global: &Arc<session::GlobalState>,
    ip_limiter: Arc<IpRateLimiterState>,
) -> Router {
    let router = router
        // ── GPL-3.0 redistribution: legal/licence files mounted *after*
        //    `cf_access_guard` and `session_middleware` so they remain
        //    publicly reachable (the licence must travel with the hosted
        //    binary).  Rate-limit + security-header layers below still apply.
        .route("/LICENSE", get(serve_license))
        .route("/NOTICE", get(serve_notice))
        .route("/THIRD_PARTY_NOTICES.md", get(serve_third_party_notices))
        // ── #621: Per-request tracing (request id, latency, status) ─────────
        // Runs inside the rate-limiter so only admitted requests are traced.
        // Reads the `otel-tracing` flag from GlobalState and is a no-op when
        // the flag is explicitly disabled.
        .layer(axum_mw::from_fn_with_state(
            Arc::clone(global),
            middleware::request_id_layer,
        ))
        // ── #381: Global per-IP rate limiter (outside auth guard, throttles floods) ──
        .layer(axum_mw::from_fn_with_state(
            ip_limiter,
            ip_rate_limit_middleware,
        ));
    // ── Security hardening headers (outermost layer — covers all routes) ──
    apply_security_layers(router)
}

// ── Extracted construction-step helpers ─────────────────────────────────────

/// Loads `.env` in debug builds; warns about an unloaded cwd `.env` in release.
///
/// Rule #9: a daemonised or packaged launch must not have its security-critical
/// config silently overridden by an `.env` discovered by walking ancestors of
/// the working directory. Release builds therefore only check the **explicit
/// startup cwd** for an `.env` (to emit the #786 warning) and never parent-walk;
/// debug builds keep `dotenvy::dotenv()` (which itself only checks the cwd, not
/// ancestors) for local dev ergonomics.
fn handle_dotenv() {
    #[cfg(debug_assertions)]
    dotenvy::dotenv().ok();
    #[cfg(not(debug_assertions))]
    {
        // Only the startup cwd is inspected — no ancestor walk (rule #9).
        let path = std::env::current_dir()
            .map(|dir| dir.join(".env"))
            .ok()
            .filter(|p| p.is_file());
        if let Some(path) = path {
            tracing::warn!(
                ".env file found at '{}' but will NOT be loaded in \
                 release builds — set environment variables explicitly to avoid \
                 accidentally overriding security-critical config (#786)",
                path.display()
            );
        }
    }
}

/// Picks the world file: `parish.json` preferred, falls back to `world.json`.
fn resolve_world_path(data_dir: &Path) -> PathBuf {
    let parish = data_dir.join("parish.json");
    let world = data_dir.join("world.json");
    if parish.exists() { parish } else { world }
}

/// Merges cloud-provider env vars into `config`, runs the shared provider
/// bootstrap, and populates per-category model slots from presets.
async fn run_llm_bootstrap(
    provider_cfg: parish_core::config::ProviderConfig,
    mut config: GameConfig,
) -> anyhow::Result<(GameConfig, parish_core::inference::client::RuntimeProcesses)> {
    let cloud_env = build_cloud_client_from_env();
    config.cloud_provider_name = cloud_env.provider_name;
    config.cloud_model_name = cloud_env.model_name;
    config.cloud_api_key = cloud_env.api_key;
    config.cloud_base_url = cloud_env.base_url;

    let progress = parish_core::inference::setup::StdoutProgress;
    let extra_vllm_mlx_slots = config.vllm_mlx_extra_slots();
    let extra_vllm_slots = config.vllm_extra_slots();
    let (_setup_client, resolved_model, runtime_procs) =
        parish_core::inference::setup::setup_provider_client(
            &provider_cfg,
            &extra_vllm_mlx_slots,
            &extra_vllm_slots,
            &parish_core::config::InferenceConfig::default(),
            &progress,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialise inference provider: {}", e))?;
    if provider_cfg.provider.id() == "ollama" {
        // Auto-setup pulled exactly one model. Pin it across all four
        // per-category slots so every role uses the model that is on
        // disk, instead of the static qwen3 preset list (which assumes
        // models the user has not pulled).
        config.pin_setup_model(resolved_model);
    } else {
        config.model_name = resolved_model;
    }
    // No-op for Ollama after `pin_setup_model` filled every slot; for
    // cloud providers fills per-role tier mapping (Opus/Sonnet/Haiku).
    config.fill_missing_models_from_presets();
    Ok((config, runtime_procs))
}

/// Extracts the game title and theme palette from the mod, with fallbacks.
fn resolve_splash_and_theme(game_mod: &Option<GameMod>) -> (String, ThemePalette) {
    let game_title = game_mod
        .as_ref()
        .and_then(|gm| gm.manifest.meta.title.clone())
        .unwrap_or_else(|| "Parish".to_string());
    let splash_text = format!(
        "{}\nCopyright \u{00A9} 2026 David Mooney. Licensed under GPL-3.0 \u{2014} see LICENSE.\nweb-server - {}",
        game_title,
        chrono::Local::now().format("%Y-%m-%d"),
    );
    let theme_palette = game_mod
        .as_ref()
        .map(|gm| gm.ui.theme.resolved_palette())
        .unwrap_or_else(parish_core::game_mod::default_theme_palette);
    (splash_text, theme_palette)
}

/// Applies engine-config tile sources, timeouts, and UI config onto `config`.
fn resolve_engine_and_ui_config(
    mut config: GameConfig,
    engine_config: &parish_core::config::EngineConfig,
    game_mod: &Option<GameMod>,
    splash_text: &str,
    theme_palette: &ThemePalette,
) -> (
    GameConfig,
    Vec<parish_core::ipc::TileSourceSnapshot>,
    String,
    UiConfigSnapshot,
) {
    // parish-server registers the `/tiles/{*path}` proxy route, so the
    // frontend uses `TileSourceConfig::url` (the same-origin proxy path).
    let tile_sources_snapshot =
        parish_core::ipc::TileSourceSnapshot::list_from_map_config(&engine_config.map, true);
    let active_tile_source = engine_config.map.default_tile_source.clone();
    config.active_tile_source = active_tile_source.clone();
    config.tile_sources = engine_config.map.id_label_pairs();
    config.idle_banter_after_secs = engine_config.session.idle_banter_after_secs;
    config.auto_pause_after_secs = engine_config.session.auto_pause_after_secs;

    let ui_config = if let Some(gm) = game_mod {
        UiConfigSnapshot {
            hints_label: gm.ui.sidebar.hints_label.clone(),
            default_accent: theme_palette.accent.clone(),
            splash_text: splash_text.to_string(),
            active_tile_source: active_tile_source.clone(),
            tile_sources: tile_sources_snapshot.clone(),
            auto_pause_timeout_seconds: engine_config.session.auto_pause_after_secs,
            app_icon_url: gm.app_icon_path().map(|_| "/api/app-icon.png".to_string()),
            favicon_url: gm.favicon_path().map(|_| "/api/favicon.png".to_string()),
            map_overlay: gm.ui.theme.map_overlay.clone(),
            base_mod_required: false,
        }
    } else {
        UiConfigSnapshot {
            hints_label: "Language Hints".to_string(),
            default_accent: theme_palette.accent.clone(),
            splash_text: splash_text.to_string(),
            active_tile_source,
            tile_sources: tile_sources_snapshot.clone(),
            auto_pause_timeout_seconds: engine_config.session.auto_pause_after_secs,
            app_icon_url: None,
            favicon_url: None,
            map_overlay: None,
            base_mod_required: true,
        }
    };

    (
        config,
        tile_sources_snapshot,
        engine_config.map.default_tile_source.clone(),
        ui_config,
    )
}

/// Opens sessions.db, the identity store, and extracts pronunciations.
fn open_session_components(
    saves_dir: &Path,
    game_mod: &Option<GameMod>,
) -> anyhow::Result<(
    SessionRegistry,
    Arc<dyn parish_core::identity::IdentityStore>,
    Vec<parish_core::game_mod::PronunciationEntry>,
)> {
    let sessions = SessionRegistry::open(saves_dir)
        .map_err(|e| anyhow::anyhow!("Failed to open sessions.db: {}", e))?;
    let identity_conn = open_sessions_db(saves_dir)
        .map_err(|e| anyhow::anyhow!("Failed to open sessions.db for identity store: {}", e))?;
    let identity_store: Arc<dyn parish_core::identity::IdentityStore> =
        Arc::new(SqliteIdentityStore::new(identity_conn));
    let pronunciations = game_mod
        .as_ref()
        .map(|gm| gm.pronunciations.clone())
        .unwrap_or_default();
    Ok((sessions, identity_store, pronunciations))
}

/// Warns when `PARISH_WS_SIGNING_KEY` is absent in debug builds.
fn check_ws_signing_key_warning() {
    if std::env::var("PARISH_WS_SIGNING_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .is_none()
        && cfg!(debug_assertions)
    {
        tracing::warn!(
            "PARISH_WS_SIGNING_KEY is not set — debug build will use a random ephemeral signing key. \
             WS session tokens will be invalidated on server restart."
        );
    }
}

/// Creates the tile cache directory and returns an initialised [`TileCache`].
///
/// Cache-dir resolution (Rule #9 — paths from config, not cwd):
/// 1. `PARISH_TILE_CACHE_DIR` env var — explicit operator/dev override.
///    Relative values are anchored to the startup cwd via
///    [`parish_persistence::paths::absolutise`] so a later `set_current_dir`
///    can't redirect cache writes.
/// 2. When `PARISH_SAVES_DIR` is also set (saves location is explicit),
///    nest the cache as `<saves_dir>/tile-cache` so a single env override
///    keeps both directories under the operator's chosen root — including
///    container-style mounts like `/saves` where `parent()` would point
///    at the unwritable filesystem root.
/// 3. Otherwise, use `<user_data_dir>/tile-cache`, a sibling of the
///    auto-resolved saves dir under the platform user-data root.
///
/// Bundled-dir resolution order:
/// 1. `PARISH_BUNDLED_TILES_DIR` env var
/// 2. `engine_config.map.bundled_tiles_dir` from `parish.toml`
/// 3. `{data_dir}/tiles` if that directory exists on disk (conventional default)
async fn init_tile_cache(
    saves_dir: &Path,
    app_name: &str,
    data_dir: &Path,
    engine_config: &parish_core::config::EngineConfig,
) -> parish_core::tile_cache::TileCache {
    let tile_cache_dir = std::env::var("PARISH_TILE_CACHE_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| parish_core::persistence::paths::absolutise(PathBuf::from(s.trim())))
        .unwrap_or_else(|| {
            let saves_overridden = std::env::var("PARISH_SAVES_DIR")
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if saves_overridden {
                // Operator picked the saves location; keep tile-cache nested
                // there so single-var overrides stay coherent and writable.
                saves_dir.join("tile-cache")
            } else {
                // Default layout: tile-cache is a sibling of saves under the
                // shared user-data root.
                parish_core::persistence::paths::resolve_user_data_dir(app_name).join("tile-cache")
            }
        });
    if let Err(e) = tokio::fs::create_dir_all(&tile_cache_dir).await {
        tracing::warn!(
            dir = %tile_cache_dir.display(),
            error = %e,
            "Could not create tile cache dir — tile proxy will fail on first miss"
        );
    }
    // Only sources with a non-empty `upstream_url` are served through the
    // proxy; the cache's url_templates is keyed by source id so requests for
    // un-proxied sources (e.g. OSM, which the browser fetches directly) are
    // rejected by `TileCache::get` before any disk or upstream I/O happens.
    // `cfg.url` is the *frontend* URL (often a same-origin proxy path);
    // `cfg.upstream_url` is the absolute URL `reqwest` fetches from. They
    // are deliberately separate — see PR #955.
    let tile_url_templates: std::collections::HashMap<String, String> = engine_config
        .map
        .tile_sources
        .iter()
        .filter(|(_, cfg)| !cfg.upstream_url.is_empty())
        .map(|(id, cfg)| (id.clone(), cfg.upstream_url.clone()))
        .collect();
    let mut cache =
        parish_core::tile_cache::TileCache::new(tile_cache_dir.clone(), tile_url_templates);

    // Resolve bundled tile directory: env var → TOML config → conventional default.
    //
    // Relative paths from env or TOML are resolved against `data_dir` (rule #9 —
    // packaged/daemon runs cannot rely on CWD matching the project root). The
    // conventional default `{data_dir}/tiles` is probed with async I/O so we
    // never block the Tokio executor during startup.
    let configured: Option<PathBuf> = std::env::var("PARISH_BUNDLED_TILES_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| engine_config.map.bundled_tiles_dir.clone());

    let bundled_dir: Option<PathBuf> = match configured {
        Some(p) if p.is_relative() => Some(data_dir.join(p)),
        Some(p) => Some(p),
        None => {
            let default = data_dir.join("tiles");
            match tokio::fs::metadata(&default).await {
                Ok(m) if m.is_dir() => Some(default),
                _ => None,
            }
        }
    };
    if let Some(ref bd) = bundled_dir {
        tracing::info!(dir = %bd.display(), "Bundled tile directory configured");
        cache = cache.with_bundled_dir(bd.clone());
    }

    tracing::info!(dir = %tile_cache_dir.display(), "Tile cache initialised");
    cache
}

/// Resolves the admission-control ceiling from env var or engine config.
fn resolve_admission_control(
    config: &GameConfig,
    engine_config: &parish_core::config::EngineConfig,
) -> Option<usize> {
    let flag_active = !config.flags.is_disabled("admission-control");
    if flag_active {
        let cap = std::env::var("PARISH_MAX_SESSIONS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(engine_config.session.max_concurrent_sessions);
        tracing::info!(
            cap,
            source = if std::env::var("PARISH_MAX_SESSIONS").is_ok() {
                "PARISH_MAX_SESSIONS env"
            } else {
                "engine config / default"
            },
            "Admission control enabled"
        );
        Some(cap)
    } else {
        tracing::info!("Admission control disabled via feature flag");
        None
    }
}

/// Spawns the background task that periodically evicts stale in-memory
/// sessions and reaps expired session data from disk.
fn spawn_session_cleanup_background_task(global: &Arc<GlobalState>) {
    let g = Arc::clone(global);
    tokio::spawn(async move {
        const MEMORY_TTL: Duration = Duration::from_secs(86_400);
        const DISK_TTL: Duration = Duration::from_secs(30 * 86_400);
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            g.sessions.cleanup_stale(MEMORY_TTL);
            let g2 = Arc::clone(&g);
            let purged = tokio::task::spawn_blocking(move || {
                g2.sessions
                    .purge_expired_disk_sessions(&g2.saves_dir, DISK_TTL)
            })
            .await
            .unwrap_or(0);
            if purged > 0 {
                tracing::info!(purged, "Session cleanup reaped expired disk sessions");
            } else {
                tracing::debug!("Session cleanup ran (no disk expirations)");
            }
        }
    });
}

/// Constructs the global per-IP rate limiter (120 req/min).
fn build_ip_rate_limiter_state() -> Arc<IpRateLimiterState> {
    use governor::{Quota, RateLimiter};
    use std::num::NonZeroU32;
    let trust_proxy = std::env::var("PARISH_TRUST_PROXY")
        .unwrap_or_default()
        .trim()
        == "1";
    Arc::new(IpRateLimiterState {
        limiter: RateLimiter::keyed(Quota::per_minute(NonZeroU32::new(120).unwrap())),
        trust_proxy,
    })
}

/// Returns `true` when tower-sessions middleware should be used.
fn should_use_tower_sessions(global: &GlobalState) -> bool {
    let use_ts = !global
        .template_config
        .flags
        .is_disabled("tower-sessions-auth");
    if use_ts {
        tracing::info!("Session middleware: tower-sessions (default)");
    } else {
        tracing::warn!(
            "Session middleware: legacy hand-rolled cookie code \
             (tower-sessions-auth flag explicitly disabled)"
        );
    }
    use_ts
}

/// Load the setting mod via [`LocalDiskModSource`], returning `None` on any
/// error so the server starts with no mod rather than refusing to start.
///
/// Using the [`ModSource`] trait here means a future S3/HTTP source can
/// replace [`LocalDiskModSource`] without changing the call site in
/// [`run_server`].
async fn load_base_mod_via_source() -> Option<GameMod> {
    let source = LocalDiskModSource::new().ok()?;
    let summaries = source.list_mods().await.ok()?;
    let base = summaries
        .into_iter()
        .find(|s| s.kind == parish_core::game_mod::ModKind::Base)?;
    match source.load_mod(&base.id).await {
        Ok(gm) => {
            tracing::info!(
                "Loaded game mod '{}' via LocalDiskModSource",
                gm.manifest.meta.name
            );
            Some(gm)
        }
        Err(e) => {
            tracing::warn!("Failed to load mod '{}': {}", base.id, e);
            None
        }
    }
}

/// Reads Google OAuth credentials from environment variables.
fn build_oauth_config() -> Option<OAuthConfig> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())?;
    let base_url = std::env::var("PARISH_PUBLIC_URL")
        .or_else(|_| std::env::var("PARISH_BASE_URL"))
        .unwrap_or_else(|_| "http://localhost:3001".to_string());
    Some(OAuthConfig {
        client_id,
        client_secret,
        base_url,
    })
}
/// Returns a copy of `url` safe to emit in logs: strips `user:pass@` userinfo
/// and any `?query` string, since `PARISH_BASE_URL` may embed basic-auth
/// credentials or signed proxy tokens.
fn sanitize_base_url(url: &str) -> String {
    let (prefix, rest) = match url.find("://") {
        Some(i) => {
            let (a, b) = url.split_at(i + 3);
            (a.to_string(), b)
        }
        None => (String::new(), url),
    };
    let path_start = rest.find('/').unwrap_or(rest.len());
    let authority_and_path = match rest[..path_start].find('@') {
        Some(at) => format!("{}{}", &rest[at + 1..path_start], &rest[path_start..]),
        None => rest.to_string(),
    };
    let trimmed = match authority_and_path.find('?') {
        Some(q) => authority_and_path[..q].to_string(),
        None => authority_and_path,
    };
    format!("{}{}", prefix, trimmed)
}

/// Builds the provider-setup input and the template `GameConfig` from
/// environment variables.
///
/// Returns a [`ProviderConfig`] suitable for the shared
/// [`parish_core::inference::setup::setup_provider_client`] bootstrap and
/// the session `GameConfig` template. The caller runs the bootstrap and
/// then overwrites `config.model_name` with the auto-resolved tag (for
/// Ollama, the tier selector's pick).
fn build_client_and_config(
    headless_models: bool,
) -> (parish_core::config::ProviderConfig, GameConfig) {
    if headless_models {
        return build_headless_local_config();
    }
    let provider_cfg = parish_core::config::resolve_config(None, &Default::default())
        .unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to resolve configuration: {}; falling back to defaults",
                e
            );
            parish_core::config::ProviderConfig {
                provider: parish_core::config::Provider::default(),
                base_url: "http://localhost:11434".to_string(),
                api_key: None,
                model: None,
            }
        });

    let provider_name = provider_cfg.provider_display();
    let base_url = provider_cfg.base_url.clone();
    let api_key = provider_cfg.api_key.clone();

    // `model_name` starts as the resolved model override or `gemma4:e4b` as a
    // placeholder; the bootstrap replaces it with the auto-selected tier tag
    // before sessions are built.
    let model_name = provider_cfg
        .model
        .clone()
        .unwrap_or_else(|| "gemma4:e4b".to_string());

    tracing::info!(
        provider = %provider_name,
        model = %model_name,
        base_url = %sanitize_base_url(&base_url),
        has_api_key = api_key.is_some(),
        "Resolved inference configuration"
    );

    let config = GameConfig {
        provider_name,
        base_url,
        api_key,
        model_name,
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
        flags: FeatureFlags::default(),
        category_rate_limit: Default::default(),
        // Tile-source fields populated in build_app_state from engine config.
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        reveal_unexplored_locations: false,
        auto_setup_model: None,
    };

    (provider_cfg, config)
}

/// Builds the provider + game config for `--headless-models` (#1364): the
/// bundled local vllm-mlx Qwen two-slot loadout. The base [`ProviderConfig`]
/// points at the 14B dialogue slot (`:8000`); the per-category overrides on
/// [`GameConfig`] (set by [`GameConfig::apply_local_qwen_two_slot`]) route
/// Intent to the 1.5B slot (`:8001`) and Simulation/Reaction to the in-process
/// simulator. `vllm_mlx_extra_slots()` then emits the `:8001` slot so
/// `setup_provider_client` detect-reuses (or spawns) both servers — never
/// double-spawning a port that a running Tauri app already owns.
///
/// This reuses the shared loadout definition in `parish-core` rather than
/// re-deriving model ids / ports here (rule #12).
fn build_headless_local_config() -> (parish_core::config::ProviderConfig, GameConfig) {
    use parish_core::ipc::config::local_models;

    let provider_cfg = parish_core::config::ProviderConfig {
        provider: parish_core::config::Provider::from_str_loose(local_models::PROVIDER)
            .unwrap_or_default(),
        base_url: local_models::DIALOGUE_BASE_URL.to_string(),
        api_key: None,
        model: Some(local_models::DIALOGUE_MODEL.to_string()),
    };

    let mut config = GameConfig {
        provider_name: local_models::PROVIDER.to_string(),
        base_url: local_models::DIALOGUE_BASE_URL.to_string(),
        api_key: None,
        model_name: local_models::DIALOGUE_MODEL.to_string(),
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
        flags: FeatureFlags::default(),
        category_rate_limit: Default::default(),
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        reveal_unexplored_locations: false,
        auto_setup_model: None,
    };
    config.apply_local_qwen_two_slot();

    tracing::info!(
        provider = %config.provider_name,
        dialogue_model = %local_models::DIALOGUE_MODEL,
        dialogue_url = %local_models::DIALOGUE_BASE_URL,
        intent_model = %local_models::INTENT_MODEL,
        intent_url = %local_models::INTENT_BASE_URL,
        "Headless local-model loadout: detect-or-spawn vllm-mlx Qwen two-slot"
    );

    (provider_cfg, config)
}

/// Cloud LLM environment configuration loaded from `PARISH_CLOUD_*` vars.
struct CloudEnvConfig {
    provider_name: Option<String>,
    model_name: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
}

fn build_cloud_client_from_env() -> CloudEnvConfig {
    let provider = std::env::var("PARISH_CLOUD_PROVIDER")
        .ok()
        .filter(|s| !s.is_empty());
    let base_url = std::env::var("PARISH_CLOUD_BASE_URL").unwrap_or_else(|_| {
        provider
            .as_deref()
            .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
            .map(|p| p.default_base_url().to_string())
            .unwrap_or_else(|| "https://openrouter.ai/api".to_string())
    });
    let provider_enum = provider
        .as_deref()
        .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
        .unwrap_or_else(|| {
            parish_core::config::Provider::from_id("openrouter").unwrap_or_default()
        });
    let api_key = provider_enum
        .api_key_env_var()
        .and_then(|var| std::env::var(var).ok())
        .filter(|s| !s.is_empty());
    let model = std::env::var("PARISH_CLOUD_MODEL")
        .ok()
        .filter(|s| !s.is_empty());

    CloudEnvConfig {
        provider_name: provider,
        model_name: model,
        api_key,
        base_url: Some(base_url),
    }
}

/// `GET /metrics` — returns the current auth-failure counter.
///
/// Protected by CF-Access.  Returns plain text so it can be scraped by simple
/// tooling without a JSON parser.
async fn get_metrics() -> String {
    let failures = AUTH_FAILURES.load(Ordering::Relaxed);
    format!(
        "# HELP parish_auth_failures_total Total CF-Access auth failures since startup\n# TYPE parish_auth_failures_total counter\nparish_auth_failures_total {failures}\n"
    )
}

/// Shared state for [`ip_rate_limit_middleware`].
struct IpRateLimiterState {
    limiter: governor::RateLimiter<
        std::net::IpAddr,
        governor::state::keyed::DefaultKeyedStateStore<std::net::IpAddr>,
        governor::clock::DefaultClock,
    >,
    /// When `true`, the middleware reads the real client IP from
    /// `X-Forwarded-For` / `Cf-Connecting-Ip` headers instead of the TCP
    /// socket address.  Only enable when the server sits behind a trusted
    /// reverse proxy (set `PARISH_TRUST_PROXY=1`).
    trust_proxy: bool,
}

/// #381 / #596 — Per-IP global rate limiter middleware (120 req/min).
///
/// Placed *outside* the auth guard so pre-auth floods are throttled before
/// the JWT validation overhead is incurred.
///
/// Debug + loopback traffic is exempt: Playwright and local devtools make
/// bursts of legitimate requests (status bar polls, WS reconnects, tile
/// fetches, e2e test setup) that otherwise trip 429 and break UX.
///
/// #596 — When `PARISH_TRUST_PROXY=1` is set (via [`IpRateLimiterState`]),
/// the real client IP is read from `Cf-Connecting-Ip` (Cloudflare) or the
/// leftmost entry in `X-Forwarded-For` (generic reverse proxies).  Without
/// proxy trust the socket address is used, which is safe but buckets all
/// traffic from the proxy under one IP when deployed behind Cloudflare or
/// another reverse proxy.
async fn ip_rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    axum::extract::State(state): axum::extract::State<Arc<IpRateLimiterState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if cfg!(debug_assertions) && addr.ip().is_loopback() {
        return Ok(next.run(req).await);
    }

    // #596 — Resolve the real client IP.  When trust_proxy is true we prefer
    // `Cf-Connecting-Ip` (Cloudflare sets this reliably) and fall back to the
    // leftmost non-empty token in `X-Forwarded-For`.  If neither header is
    // present or parseable we fall back to the socket address.  When
    // trust_proxy is false we always use the socket address to prevent
    // spoofing by clients that inject headers themselves.
    let client_ip: std::net::IpAddr = if state.trust_proxy {
        extract_real_ip(req.headers()).unwrap_or_else(|| addr.ip())
    } else {
        addr.ip()
    };

    match state.limiter.check_key(&client_ip) {
        Ok(_) => Ok(next.run(req).await),
        Err(_) => {
            tracing::warn!(
                socket_ip = %addr,
                client_ip = %client_ip,
                "ip_rate_limit_middleware: 429 — too many requests"
            );
            Err(StatusCode::TOO_MANY_REQUESTS)
        }
    }
}

/// Extract the real client IP from proxy-forwarded headers.
///
/// Priority:
/// 1. `Cf-Connecting-Ip` — set by Cloudflare to the original client IP.
/// 2. Leftmost token in `X-Forwarded-For` — set by most reverse proxies.
///
/// Returns `None` if no header is present or the value cannot be parsed as an
/// IP address.  Only called when `PARISH_TRUST_PROXY=1` is set.
fn extract_real_ip(headers: &axum::http::HeaderMap) -> Option<std::net::IpAddr> {
    use axum::http::header::HeaderName;

    // Cloudflare sets `Cf-Connecting-Ip` to exactly the original client IP.
    static CF_CONNECTING_IP: std::sync::LazyLock<HeaderName> =
        std::sync::LazyLock::new(|| HeaderName::from_static("cf-connecting-ip"));
    if let Some(v) = headers.get(&*CF_CONNECTING_IP)
        && let Ok(s) = v.to_str()
        && let Ok(ip) = s.trim().parse()
    {
        return Some(ip);
    }

    // RFC 7239 `Forwarded` header: syntax is `for=<node>;proto=http`, possibly
    // comma-separated for multiple hops.  Extract the `for=` parameter from the
    // first (leftmost) directive, then strip optional port and bracket notation
    // used for IPv6 (e.g. `[::1]:8080` → `::1`).
    if let Some(v) = headers.get(axum::http::header::FORWARDED)
        && let Ok(s) = v.to_str()
        && let Some(ip) = parse_forwarded_for(s)
    {
        return Some(ip);
    }

    // Generic `X-Forwarded-For`: leftmost entry is the original client.
    if let Some(v) = headers.get(HeaderName::from_static("x-forwarded-for"))
        && let Ok(s) = v.to_str()
        && let Some(first) = s.split(',').next()
        && let Ok(ip) = first.trim().parse()
    {
        return Some(ip);
    }

    None
}

/// Parse an IP address from the `for=` parameter of an RFC 7239 `Forwarded`
/// header value.  Returns `None` if the header is missing, malformed, or
/// contains no valid IP address so the caller can fall through to
/// `X-Forwarded-For`.
///
/// Accepted `for=` node forms:
/// - bare IPv4:        `for=192.0.2.60`
/// - quoted IPv4:      `for="192.0.2.60"`
/// - bracketed IPv6:   `for="[::1]"` or `for=[::1]`
/// - IPv6 with port:   `for="[::1]:8080"`
/// - quoted with port: `for="192.0.2.60:1234"` (port stripped)
fn parse_forwarded_for(header: &str) -> Option<std::net::IpAddr> {
    // Only look at the first (leftmost/client) directive.
    let first_directive = header.split(',').next()?;

    // Each directive is a semicolon-separated list of parameters.
    for param in first_directive.split(';') {
        let param = param.trim();
        let lower = param.to_ascii_lowercase();
        let value = if lower.starts_with("for=") {
            &param[4..]
        } else {
            continue;
        };

        // Strip surrounding double quotes if present.
        let value = if value.starts_with('"') && value.ends_with('"') {
            &value[1..value.len() - 1]
        } else {
            value
        };

        // Bracketed IPv6: `[::1]` or `[::1]:port`
        if let Some(inner) = value.strip_prefix('[') {
            let addr_str = if let Some(pos) = inner.find(']') {
                &inner[..pos]
            } else {
                inner
            };
            if let Ok(ip) = addr_str.parse::<std::net::IpAddr>() {
                return Some(ip);
            }
            // Bracketed but unparseable — fall through to next param / give up.
            continue;
        }

        // Plain value: could be IPv4, IPv4:port, or bare IPv6.
        // Try direct parse first (handles bare IPv4 and bare IPv6).
        if let Ok(ip) = value.parse::<std::net::IpAddr>() {
            return Some(ip);
        }

        // IPv4 with port: strip the trailing `:port`.
        if let Some(pos) = value.rfind(':')
            && let Ok(ip) = value[..pos].parse::<std::net::IpAddr>()
        {
            return Some(ip);
        }

        // Unrecognisable — keep looking at other parameters (unusual) then give up.
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial(parish_env)]
    fn build_client_and_config_defaults() {
        // In test env, clear PARISH_PROVIDER to ensure it defaults to "simulator"
        unsafe { std::env::remove_var("PARISH_PROVIDER") };
        let (_client, config) = build_client_and_config(false);
        assert_eq!(config.provider_name, "simulator");
    }

    /// `--headless-models` selects the bundled local two-slot Qwen loadout:
    /// base provider vllm-mlx @ :8000 (14B dialogue), Intent @ :8001 (1.5B),
    /// Simulation/Reaction on the simulator. This is the #1364 AC1 binding.
    #[test]
    #[serial(parish_env)]
    fn build_client_and_config_headless_models_binds_local_qwen() {
        use parish_core::config::InferenceCategory;
        use parish_core::ipc::config::local_models;

        let (provider_cfg, config) = build_client_and_config(true);

        // Base slot → 14B @ :8000.
        assert_eq!(provider_cfg.provider.id(), "vllmmlx");
        assert_eq!(provider_cfg.base_url, local_models::DIALOGUE_BASE_URL);
        assert_eq!(
            provider_cfg.model.as_deref(),
            Some(local_models::DIALOGUE_MODEL)
        );
        assert!(provider_cfg.api_key.is_none());

        // Dialogue inherits the base 14B slot.
        assert_eq!(
            config
                .category_base_url
                .get(&InferenceCategory::Dialogue)
                .map(String::as_str),
            Some(local_models::DIALOGUE_BASE_URL)
        );
        // Intent → 1.5B @ :8001.
        assert_eq!(
            config
                .category_base_url
                .get(&InferenceCategory::Intent)
                .map(String::as_str),
            Some(local_models::INTENT_BASE_URL)
        );
        assert_eq!(
            config
                .category_model
                .get(&InferenceCategory::Intent)
                .map(String::as_str),
            Some(local_models::INTENT_MODEL)
        );
        // Simulation + Reaction → simulator.
        for cat in [InferenceCategory::Simulation, InferenceCategory::Reaction] {
            assert_eq!(
                config.category_provider.get(&cat).map(String::as_str),
                Some(local_models::SIMULATOR_PROVIDER)
            );
        }

        // The extra-slot builder emits exactly the 1.5B :8001 slot (the base
        // 14B :8000 slot is auto-spawned by setup_provider_client, and the
        // simulator categories spawn nothing).
        let extra = config.vllm_mlx_extra_slots();
        assert_eq!(extra.len(), 1, "only the 1.5B intent slot is an extra slot");
        assert_eq!(extra[0].base_url, local_models::INTENT_BASE_URL);
        assert_eq!(extra[0].model, local_models::INTENT_MODEL);
    }

    #[test]
    fn sanitize_base_url_strips_userinfo_and_query() {
        assert_eq!(
            sanitize_base_url("https://user:pass@api.example.com/v1"),
            "https://api.example.com/v1"
        );
        assert_eq!(
            sanitize_base_url("https://api.example.com/v1?token=secret"),
            "https://api.example.com/v1"
        );
        assert_eq!(
            sanitize_base_url("https://user:pass@api.example.com/v1?token=secret"),
            "https://api.example.com/v1"
        );
        assert_eq!(
            sanitize_base_url("http://localhost:11434"),
            "http://localhost:11434"
        );
        // '@' in path (after the authority) must not be treated as userinfo.
        assert_eq!(
            sanitize_base_url("https://api.example.com/foo@bar"),
            "https://api.example.com/foo@bar"
        );
    }

    #[test]
    #[serial(parish_env)]
    fn build_oauth_config_missing_returns_none() {
        // Ensure env vars are not set in the test environment.
        // SAFETY: serialised via `#[serial(parish_env)]` — no concurrent
        // threads touch these vars while this test runs.
        unsafe {
            std::env::remove_var("GOOGLE_CLIENT_ID");
            std::env::remove_var("GOOGLE_CLIENT_SECRET");
        }
        assert!(build_oauth_config().is_none());
    }

    #[test]
    #[serial(parish_env)]
    fn build_oauth_config_prefers_public_url() {
        // SAFETY: serialised via `#[serial(parish_env)]` — no concurrent
        // threads touch these vars while this test runs.
        unsafe {
            std::env::set_var("GOOGLE_CLIENT_ID", "test-id");
            std::env::set_var("GOOGLE_CLIENT_SECRET", "test-secret");
            std::env::set_var("PARISH_PUBLIC_URL", "https://myapp.example.com");
            std::env::set_var("PARISH_BASE_URL", "https://api.openrouter.ai");
        }
        let cfg = build_oauth_config().expect("should build with credentials set");
        assert_eq!(cfg.base_url, "https://myapp.example.com");
        unsafe {
            std::env::remove_var("GOOGLE_CLIENT_ID");
            std::env::remove_var("GOOGLE_CLIENT_SECRET");
            std::env::remove_var("PARISH_PUBLIC_URL");
            std::env::remove_var("PARISH_BASE_URL");
        }
    }

    #[test]
    #[serial(parish_env)]
    fn build_oauth_config_falls_back_to_base_url() {
        // SAFETY: serialised via `#[serial(parish_env)]` — no concurrent
        // threads touch these vars while this test runs.
        unsafe {
            std::env::set_var("GOOGLE_CLIENT_ID", "test-id");
            std::env::set_var("GOOGLE_CLIENT_SECRET", "test-secret");
            std::env::remove_var("PARISH_PUBLIC_URL");
            std::env::set_var("PARISH_BASE_URL", "https://myapp.example.com");
        }
        let cfg = build_oauth_config().expect("should build with credentials set");
        assert_eq!(cfg.base_url, "https://myapp.example.com");
        unsafe {
            std::env::remove_var("GOOGLE_CLIENT_ID");
            std::env::remove_var("GOOGLE_CLIENT_SECRET");
            std::env::remove_var("PARISH_BASE_URL");
        }
    }

    // ── #596 extract_real_ip tests ───────────────────────────────────────────

    fn make_headers(pairs: &[(&str, &str)]) -> axum::http::HeaderMap {
        let mut map = axum::http::HeaderMap::new();
        for (k, v) in pairs {
            map.insert(
                axum::http::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    #[test]
    fn extract_real_ip_returns_none_with_no_proxy_headers() {
        let headers = make_headers(&[]);
        assert_eq!(extract_real_ip(&headers), None);
    }

    #[test]
    fn extract_real_ip_prefers_cf_connecting_ip() {
        let headers = make_headers(&[
            ("cf-connecting-ip", "1.2.3.4"),
            ("x-forwarded-for", "9.9.9.9, 10.0.0.1"),
        ]);
        let ip = extract_real_ip(&headers).expect("should parse Cf-Connecting-Ip");
        assert_eq!(ip, "1.2.3.4".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn extract_real_ip_falls_back_to_x_forwarded_for_leftmost() {
        // Only X-Forwarded-For present; leftmost entry is the client.
        let headers = make_headers(&[("x-forwarded-for", "203.0.113.42, 10.0.0.1, 172.16.0.1")]);
        let ip = extract_real_ip(&headers).expect("should parse X-Forwarded-For");
        assert_eq!(ip, "203.0.113.42".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn extract_real_ip_handles_ipv6() {
        let headers = make_headers(&[("cf-connecting-ip", "2001:db8::1")]);
        let ip = extract_real_ip(&headers).expect("should parse IPv6");
        assert_eq!(ip, "2001:db8::1".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn extract_real_ip_returns_none_for_malformed_header() {
        // A malformed value (not a valid IP) must not panic — it silently
        // falls through to the next header / None.
        let headers = make_headers(&[("cf-connecting-ip", "not-an-ip")]);
        assert_eq!(extract_real_ip(&headers), None);
    }

    #[test]
    fn extract_real_ip_trims_whitespace_around_address() {
        let headers = make_headers(&[("x-forwarded-for", "  198.51.100.7 , 10.0.0.2")]);
        let ip = extract_real_ip(&headers).expect("should parse trimmed address");
        assert_eq!(ip, "198.51.100.7".parse::<std::net::IpAddr>().unwrap());
    }

    // ── RFC 7239 Forwarded header tests (#629) ──────────────────────────────

    #[test]
    fn parse_forwarded_for_bare_ipv4() {
        let ip =
            parse_forwarded_for("for=192.0.2.60;proto=http").expect("should parse bare IPv4 for=");
        assert_eq!(ip, "192.0.2.60".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn parse_forwarded_for_quoted_ipv4() {
        let ip = parse_forwarded_for("for=\"192.0.2.60\";proto=http")
            .expect("should parse quoted IPv4 for=");
        assert_eq!(ip, "192.0.2.60".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn parse_forwarded_for_ipv6_in_brackets() {
        let ip = parse_forwarded_for("for=\"[2001:db8::1]\";proto=http")
            .expect("should parse bracketed IPv6");
        assert_eq!(ip, "2001:db8::1".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn parse_forwarded_for_ipv6_brackets_with_port() {
        // RFC 7239 §6: IPv6 with port looks like [::1]:8080
        let ip =
            parse_forwarded_for("for=\"[::1]:8080\"").expect("should strip port and parse IPv6");
        assert_eq!(ip, "::1".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn parse_forwarded_for_multiple_hops_uses_leftmost() {
        // Only the first (client) directive should be used.
        let ip = parse_forwarded_for("for=203.0.113.1, for=10.0.0.1")
            .expect("should pick leftmost directive");
        assert_eq!(ip, "203.0.113.1".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn parse_forwarded_for_returns_none_when_no_for_param() {
        // `by=` only — no `for=` present.
        assert_eq!(parse_forwarded_for("by=203.0.113.43;proto=https"), None);
    }

    #[test]
    fn parse_forwarded_for_returns_none_for_invalid_ip() {
        assert_eq!(parse_forwarded_for("for=not-an-ip"), None);
    }

    #[test]
    fn extract_real_ip_uses_forwarded_header_rfc7239() {
        let headers = make_headers(&[("forwarded", "for=203.0.113.5;proto=https")]);
        let ip = extract_real_ip(&headers).expect("should parse RFC 7239 Forwarded");
        assert_eq!(ip, "203.0.113.5".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn extract_real_ip_forwarded_ipv6_brackets() {
        let headers = make_headers(&[("forwarded", "for=\"[2001:db8::cafe]\";proto=https")]);
        let ip = extract_real_ip(&headers).expect("should parse bracketed IPv6 in Forwarded");
        assert_eq!(ip, "2001:db8::cafe".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn extract_real_ip_malformed_forwarded_falls_back_to_xff() {
        // Forwarded header present but malformed (no `for=`) — must fall through
        // to X-Forwarded-For rather than returning None.
        let headers = make_headers(&[
            ("forwarded", "proto=https;host=example.com"),
            ("x-forwarded-for", "198.51.100.42, 10.0.0.1"),
        ]);
        let ip = extract_real_ip(&headers)
            .expect("should fall back to X-Forwarded-For when Forwarded has no for=");
        assert_eq!(ip, "198.51.100.42".parse::<std::net::IpAddr>().unwrap());
    }

    #[test]
    fn extract_real_ip_invalid_ip_in_forwarded_falls_back_to_xff() {
        let headers = make_headers(&[
            ("forwarded", "for=not-an-ip"),
            ("x-forwarded-for", "198.51.100.7"),
        ]);
        let ip = extract_real_ip(&headers)
            .expect("should fall back to X-Forwarded-For when Forwarded for= is invalid");
        assert_eq!(ip, "198.51.100.7".parse::<std::net::IpAddr>().unwrap());
    }

    // ── TD-014: /metrics ────────────────────────────────────────────────────

    /// `GET /metrics` must return a plain-text Prometheus-formatted response
    /// containing the `parish_auth_failures_total` counter.
    #[tokio::test]
    async fn metrics_returns_counter_in_plain_text() {
        let resp = get_metrics().await;
        assert!(
            resp.contains("parish_auth_failures_total"),
            "metrics must contain the auth-failure counter"
        );
        assert!(
            resp.contains("# TYPE parish_auth_failures_total counter"),
            "metrics must have proper Prometheus type annotation"
        );
    }

    // ── TD-016: ip_rate_limit_middleware ─────────────────────────────────────

    /// The rate-limiter middleware must pass through the first request and
    /// return 429 when the per-IP quota is exceeded for a non-loopback address.
    #[tokio::test]
    async fn ip_rate_limit_middleware_blocks_at_capacity() {
        use governor::{Quota, RateLimiter as GovRateLimiter};
        use std::num::NonZeroU32;
        use std::sync::atomic::AtomicUsize;
        use tower::ServiceExt;

        let rate_state = Arc::new(IpRateLimiterState {
            limiter: GovRateLimiter::keyed(Quota::per_minute(NonZeroU32::new(1).unwrap())),
            trust_proxy: false,
        });

        let handler_count = Arc::new(AtomicUsize::new(0));

        // Inner: rate-limiter middleware.
        // Outer: injects ConnectInfo with a non-loopback IP so the debug bypass
        // does not apply.
        let app = {
            let rate_state = Arc::clone(&rate_state);
            let handler_count = Arc::clone(&handler_count);
            Router::new()
                .route(
                    "/test",
                    axum::routing::get(move || {
                        let c = Arc::clone(&handler_count);
                        async move {
                            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            StatusCode::OK
                        }
                    }),
                )
                .layer(axum::middleware::from_fn_with_state(
                    rate_state,
                    ip_rate_limit_middleware,
                ))
                .layer(axum::middleware::from_fn(
                    |mut req: Request<axum::body::Body>, next: Next| async move {
                        let addr: SocketAddr = "10.0.0.1:12345".parse().unwrap();
                        req.extensions_mut().insert(ConnectInfo(addr));
                        next.run(req).await
                    },
                ))
        };

        // First request — within the 1 req/min limit.
        let req = Request::builder()
            .uri("/test")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(handler_count.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Second request — exceeds rate limit → 429.
        let req = Request::builder()
            .uri("/test")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            handler_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "handler must not execute when rate-limited"
        );
    }

    // ── build_csp_policy unit tests (TD-036, #543) ────────────────────────────
    //
    // These tests exercise the `build_csp_policy` helper with synthetic hashes
    // so that the hash-injection branch (the `for hash in script_hashes` loop)
    // is covered even in environments where `apps/ui/dist` has not been built
    // (i.e. in CI, where `SCRIPT_SRC_HASHES` is an empty slice).

    #[test]
    fn build_csp_policy_no_hashes_uses_self_only() {
        let policy = build_csp_policy(&[]);
        let script_src = policy
            .split(';')
            .find(|d| d.trim().starts_with("script-src"))
            .expect("script-src directive must be present");
        // With no hashes, script-src should be exactly "script-src 'self'".
        assert_eq!(script_src.trim(), "script-src 'self'");
        assert!(
            !script_src.contains("'unsafe-inline'"),
            "no unsafe-inline when hash list is empty"
        );
    }

    #[test]
    fn build_csp_policy_single_hash_appended_to_script_src() {
        let hash = "'sha256-abc123='";
        let policy = build_csp_policy(&[hash]);
        let script_src = policy
            .split(';')
            .find(|d| d.trim().starts_with("script-src"))
            .expect("script-src directive must be present");
        assert!(
            script_src.contains("'self'"),
            "script-src must retain 'self'; got: {script_src}"
        );
        assert!(
            script_src.contains(hash),
            "script-src must contain the hash token; got: {script_src}"
        );
    }

    #[test]
    fn build_csp_policy_multiple_hashes_all_appear_in_script_src() {
        let hashes = ["'sha256-aaaaaa='", "'sha256-bbbbbb='", "'sha256-cccccc='"];
        let policy = build_csp_policy(&hashes);
        let script_src = policy
            .split(';')
            .find(|d| d.trim().starts_with("script-src"))
            .expect("script-src directive must be present");
        for h in &hashes {
            assert!(
                script_src.contains(h),
                "script-src must contain hash {h}; got: {script_src}"
            );
        }
        assert!(
            !script_src.contains("'unsafe-inline'"),
            "unsafe-inline must not appear when hashes are provided"
        );
    }

    #[test]
    fn build_csp_policy_always_includes_required_directives() {
        // Regardless of the hash list, the policy must include the other
        // directives unchanged.
        let policy = build_csp_policy(&["'sha256-test='"]);
        assert!(policy.contains("default-src 'self'"));
        assert!(policy.contains("frame-ancestors 'none'"));
        assert!(policy.contains("base-uri 'self'"));
        assert!(policy.contains("form-action 'self'"));
        assert!(policy.contains("connect-src 'self' ws: wss:"));
    }
}
