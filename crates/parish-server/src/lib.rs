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
pub mod editor_routes;
pub mod middleware;
pub mod routes;
pub mod session;
pub mod state;
pub mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::ConnectInfo;
use axum::http::header::{
    CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware as axum_mw;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use parish_core::game_mod::{GameMod, find_default_mod};
use parish_core::world::transport::TransportConfig;

use parish_core::config::FeatureFlags;
use session::{GlobalState, OAuthConfig, SessionRegistry};
use state::{GameConfig, UiConfigSnapshot};

/// Content-Security-Policy value shared between production and tests.
///
/// # script-src 'unsafe-inline' (TODO: replace with hash)
///
/// SvelteKit's production build injects a small inline bootstrap `<script>` in
/// `dist/index.html` that hydrates the app.  Removing `'unsafe-inline'` from
/// `script-src` causes the browser to reject that script, so the page never
/// hydrates (codex P1, PR #543).
///
/// The proper fix is to compute the SHA-256 of that bootstrap block and add
/// `'sha256-<base64>'` to `script-src`.  That hash is deterministic per build
/// but must be regenerated whenever SvelteKit changes the bootstrap text.
/// Until that build-time integration exists, `'unsafe-inline'` is restored here
/// so the app keeps working.
///
/// TODO: replace `'unsafe-inline'` with `'sha256-...'` computed from
/// `apps/ui/dist/index.html` at build time.
/// See: <https://github.com/dmooney/Parish/issues/543>
pub const CSP_POLICY: &str = "default-src 'self'; \
                              script-src 'self' 'unsafe-inline'; \
                              worker-src 'self' blob:; \
                              style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; \
                              img-src 'self' data: blob: https:; \
                              connect-src 'self' ws: wss: https:; \
                              font-src 'self' https://fonts.gstatic.com; \
                              frame-ancestors 'none'; \
                              base-uri 'self'; \
                              form-action 'self'";

/// Global auth-failure counter — exposed via `GET /metrics`.
static AUTH_FAILURES: AtomicU64 = AtomicU64::new(0);

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
async fn cf_access_guard(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // #379 — loopback bypass is debug-only.
    if cfg!(debug_assertions) && addr.ip().is_loopback() {
        // In debug+loopback: inject a synthetic AuthContext so downstream
        // handlers that require it don't panic.
        req.extensions_mut().insert(cf_auth::AuthContext {
            email: "dev@localhost".to_string(),
        });
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
                Ok(ctx) => {
                    req.extensions_mut().insert(ctx);
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
            req.extensions_mut().insert(cf_auth::AuthContext { email });
            return Ok(next.run(req).await);
        }
        tracing::warn!(source_ip = %addr, "cf_access_guard: 401 — debug fallback, missing CF-Access-Authenticated-User-Email");
        AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Starts the Parish web server on the given port.
pub async fn run_server(port: u16, data_dir: PathBuf, static_dir: PathBuf) -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // ── World path ────────────────────────────────────────────────────────────
    let world_path = {
        let parish = data_dir.join("parish.json");
        let world = data_dir.join("world.json");
        if parish.exists() { parish } else { world }
    };

    // ── LLM client + config (template, cloned per session) ───────────────────
    let (provider_cfg, mut config) = build_client_and_config();
    let cloud_env = build_cloud_client_from_env();
    config.cloud_provider_name = cloud_env.provider_name;
    config.cloud_model_name = cloud_env.model_name;
    config.cloud_api_key = cloud_env.api_key;
    config.cloud_base_url = cloud_env.base_url;

    // Run the shared provider bootstrap (Ollama install / auto-start / GPU
    // detect / model pull / warmup) — CLAUDE.md rule #2 (mode parity).
    // Per-session clients are built from the template config, which at this
    // point has the auto-selected model tag resolved. The returned client
    // is discarded — sessions build their own. The `OllamaProcess` is kept
    // in `GlobalState` so the child server dies with this process.
    let progress = parish_core::inference::setup::StdoutProgress;
    let (_setup_client, resolved_model, ollama_process) =
        parish_core::inference::setup::setup_provider_client(
            &provider_cfg,
            &parish_core::config::InferenceConfig::default(),
            &progress,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialise inference provider: {}", e))?;
    config.model_name = resolved_model;

    // ── Game mod ──────────────────────────────────────────────────────────────
    let game_mod = find_default_mod().and_then(|dir| GameMod::load(&dir).ok());
    let game_title = game_mod
        .as_ref()
        .and_then(|gm| gm.manifest.meta.title.clone())
        .unwrap_or_else(|| "Parish".to_string());

    // #373 — omit commit SHA from splash text to avoid leaking RAILWAY_GIT_COMMIT_SHA.
    let splash_text = format!(
        "{}\nCopyright \u{00A9} 2026 David Mooney. All rights reserved.\nweb-server - {}",
        game_title,
        chrono::Local::now().format("%Y-%m-%d"),
    );

    let theme_palette = game_mod
        .as_ref()
        .map(|gm| gm.ui.theme.resolved_palette())
        .unwrap_or_else(parish_core::game_mod::default_theme_palette);

    // Load engine config (parish.toml) for the tile-source registry. Missing
    // file or parse errors fall back to baked defaults
    // (OSM + Ireland Historic 6").
    let engine_config = parish_core::config::load_engine_config(None);
    let tile_sources_snapshot =
        parish_core::ipc::TileSourceSnapshot::list_from_map_config(&engine_config.map);
    let active_tile_source = engine_config.map.default_tile_source.clone();
    config.active_tile_source = active_tile_source.clone();
    config.tile_sources = engine_config.map.id_label_pairs();

    let ui_config = if let Some(ref gm) = game_mod {
        UiConfigSnapshot {
            hints_label: gm.ui.sidebar.hints_label.clone(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
            active_tile_source: active_tile_source.clone(),
            tile_sources: tile_sources_snapshot.clone(),
        }
    } else {
        UiConfigSnapshot {
            hints_label: "Language Hints".to_string(),
            default_accent: theme_palette.accent.clone(),
            splash_text,
            active_tile_source,
            tile_sources: tile_sources_snapshot,
        }
    };

    // ── Feature flags ──────────────────────────────────────────────────────
    let flags_path = data_dir.join("parish-flags.json");
    config.flags = FeatureFlags::load_from_file(&flags_path);

    // ── Saves directory ───────────────────────────────────────────────────────
    let saves_dir = parish_core::persistence::picker::ensure_saves_dir();

    // ── Session registry ──────────────────────────────────────────────────────
    let sessions = SessionRegistry::open(&saves_dir)
        .map_err(|e| anyhow::anyhow!("Failed to open sessions.db: {}", e))?;

    // ── Pronunciations (shared, loaded once) ──────────────────────────────────
    let pronunciations = game_mod
        .as_ref()
        .map(|gm| gm.pronunciations.clone())
        .unwrap_or_default();

    // ── Google OAuth config (optional) ────────────────────────────────────────
    let oauth_config = build_oauth_config();
    if oauth_config.is_some() {
        tracing::info!("Google OAuth enabled");
    }

    // ── #377: WS signing key ──
    // Release builds require PARISH_WS_SIGNING_KEY (signing_key() panics
    // otherwise). Debug builds auto-generate an ephemeral key; warn so
    // developers notice tokens invalidate on restart.
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

    // ── Global state ──────────────────────────────────────────────────────────
    let global = Arc::new(GlobalState {
        sessions,
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
        ollama_process: tokio::sync::Mutex::new(ollama_process),
    });

    // ── Session cleanup background task ───────────────────────────────────────
    {
        let g = Arc::clone(&global);
        tokio::spawn(async move {
            // Memory TTL: 1 day idle → evict from DashMap (cookie still
            // valid; next visit restores from sessions.db).
            const MEMORY_TTL: Duration = Duration::from_secs(86_400);
            // Disk TTL: 30 days idle → purge sessions.db row +
            // saves/<id>/ directory so long-running deployments don't
            // accumulate dead sessions forever (#482).
            const DISK_TTL: Duration = Duration::from_secs(30 * 86_400);
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                g.sessions.cleanup_stale(MEMORY_TTL);
                let saves_root = g.saves_dir.clone();
                let purged = g
                    .sessions
                    .purge_expired_disk_sessions(&saves_root, DISK_TTL);
                if purged > 0 {
                    tracing::info!(purged, "Session cleanup reaped expired disk sessions");
                } else {
                    tracing::debug!("Session cleanup ran (no disk expirations)");
                }
            }
        });
    }

    // ── #381: Global per-IP rate limiter (120 req/min) ────────────────────────
    use governor::{Quota, RateLimiter};
    use std::num::NonZeroU32;
    let ip_limiter = Arc::new(RateLimiter::keyed(Quota::per_minute(
        NonZeroU32::new(120).unwrap(),
    )));

    // ── Build router ──────────────────────────────────────────────────────────
    let oauth_enabled = global.oauth_config.is_some();

    let mut app = Router::new()
        // #373 — /api/health: exempt from auth, returns 200 for health probes.
        .route("/api/health", get(routes::get_health))
        // #373 — /metrics: auth-protected, exposes auth failure counter.
        .route("/metrics", get(get_metrics))
        // #377 — /api/session-init: issues short-lived WS session tokens.
        .route("/api/session-init", post(routes::session_init))
        .route("/api/world-snapshot", get(routes::get_world_snapshot))
        .route("/api/map", get(routes::get_map))
        .route("/api/npcs-here", get(routes::get_npcs_here))
        .route("/api/theme", get(routes::get_theme))
        .route("/api/ui-config", get(routes::get_ui_config))
        .route("/api/debug-snapshot", get(routes::get_debug_snapshot))
        .route("/api/submit-input", post(routes::submit_input))
        .route("/api/react-to-message", post(routes::react_to_message))
        .route("/api/discover-save-files", get(routes::discover_save_files))
        .route("/api/save-game", post(routes::save_game))
        .route("/api/load-branch", post(routes::load_branch))
        .route("/api/create-branch", post(routes::create_branch))
        .route("/api/new-save-file", post(routes::new_save_file))
        .route("/api/new-game", post(routes::new_game))
        .route("/api/save-state", get(routes::get_save_state))
        .route("/api/ws", get(ws::ws_handler))
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
        app = app
            .route("/auth/login/google", get(auth::login_google))
            .route("/auth/callback/google", get(auth::callback_google))
            .route("/auth/logout", get(auth::logout));
    }

    let app = app
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        .layer(axum_mw::from_fn(cf_access_guard))
        .with_state(Arc::clone(&global))
        .layer(axum_mw::from_fn_with_state(
            Arc::clone(&global),
            middleware::session_middleware,
        ))
        // ── #381: Global per-IP rate limiter (outside auth guard, throttles floods) ──
        .layer(axum_mw::from_fn_with_state(
            ip_limiter,
            ip_rate_limit_middleware,
        ))
        // ── Security hardening headers (outermost layer — covers all routes) ──
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
        ));

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
fn build_client_and_config() -> (parish_core::config::ProviderConfig, GameConfig) {
    let provider_name =
        std::env::var("PARISH_PROVIDER").unwrap_or_else(|_| "simulator".to_string());
    let model = std::env::var("PARISH_MODEL").unwrap_or_default();
    let provider_enum =
        parish_core::config::Provider::from_str_loose(&provider_name).unwrap_or_default();
    let base_url = std::env::var("PARISH_BASE_URL").unwrap_or_else(|_| {
        let default = provider_enum.default_base_url();
        if default.is_empty() {
            "http://localhost:11434".to_string()
        } else {
            default.to_string()
        }
    });
    let api_key = std::env::var("PARISH_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());

    let provider_cfg = parish_core::config::ProviderConfig {
        provider: provider_enum,
        base_url: base_url.clone(),
        api_key: api_key.clone(),
        model: if model.is_empty() {
            None
        } else {
            Some(model.clone())
        },
    };

    // `model_name` starts as the env override (if any) or `gemma4:e4b` as a
    // placeholder; the bootstrap replaces it with the auto-selected tier tag
    // before sessions are built.
    let model_name = if model.is_empty() {
        "gemma4:e4b".to_string()
    } else {
        model
    };

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
        category_provider: [None, None, None, None],
        category_model: [None, None, None, None],
        category_api_key: [None, None, None, None],
        category_base_url: [None, None, None, None],
        flags: FeatureFlags::default(),
        category_rate_limit: [None, None, None, None],
        // Tile-source fields populated in build_app_state from engine config.
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        reveal_unexplored_locations: false,
    };

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
    let api_key = std::env::var("PARISH_CLOUD_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());
    let model = std::env::var("PARISH_CLOUD_MODEL")
        .ok()
        .filter(|s| !s.is_empty());

    CloudEnvConfig {
        provider_name: provider.or_else(|| api_key.as_ref().map(|_| "openrouter".to_string())),
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

/// #381 — Per-IP global rate limiter middleware (120 req/min).
///
/// Placed *outside* the auth guard so pre-auth floods are throttled before
/// the JWT validation overhead is incurred.
///
/// Debug + loopback traffic is exempt: Playwright and local devtools make
/// bursts of legitimate requests (status bar polls, WS reconnects, tile
/// fetches, e2e test setup) that otherwise trip 429 and break UX.
async fn ip_rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    axum::extract::State(limiter): axum::extract::State<
        Arc<
            governor::RateLimiter<
                std::net::IpAddr,
                governor::state::keyed::DefaultKeyedStateStore<std::net::IpAddr>,
                governor::clock::DefaultClock,
            >,
        >,
    >,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if cfg!(debug_assertions) && addr.ip().is_loopback() {
        return Ok(next.run(req).await);
    }
    match limiter.check_key(&addr.ip()) {
        Ok(_) => Ok(next.run(req).await),
        Err(_) => {
            tracing::warn!(source_ip = %addr, "ip_rate_limit_middleware: 429 — too many requests");
            Err(StatusCode::TOO_MANY_REQUESTS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_client_and_config_defaults() {
        // In test env, PARISH_PROVIDER is usually not set → defaults to "simulator"
        let (_client, config) = build_client_and_config();
        assert_eq!(config.provider_name, "simulator");
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
    fn build_oauth_config_missing_returns_none() {
        // Ensure env vars are not set in the test environment.
        // SAFETY: single-threaded test; no other thread reads these vars.
        unsafe {
            std::env::remove_var("GOOGLE_CLIENT_ID");
            std::env::remove_var("GOOGLE_CLIENT_SECRET");
        }
        assert!(build_oauth_config().is_none());
    }

    #[test]
    fn build_oauth_config_prefers_public_url() {
        // SAFETY: single-threaded test; no other thread reads these vars.
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
    fn build_oauth_config_falls_back_to_base_url() {
        // SAFETY: single-threaded test; no other thread reads these vars.
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
}
