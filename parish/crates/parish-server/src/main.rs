//! `parish-server` binary entry point.
//!
//! Standalone Axum HTTP/WebSocket server. Boots `run_server` from the
//! crate's library surface with paths resolved from CLI / env inputs first;
//! a cwd ancestor-walk fallback runs only in debug builds (rule #9 — release
//! builds never parent-walk).

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use opentelemetry::trace::TracerProvider as _;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Parish — Axum HTTP/WebSocket server
#[derive(Parser, Debug)]
#[command(name = "parish-server", version, about)]
struct Cli {
    /// TCP port to bind on.
    #[arg(long, env = "PARISH_PORT", default_value_t = 3001)]
    port: u16,

    /// Mod data directory (containing `world.json` + `npcs.json`).
    /// In debug builds, defaults to an ancestor-walk of the cwd for
    /// `mods/rundale/`; release builds use a bare default and require this
    /// flag (or `PARISH_DATA_DIR`) for packaged/daemonised launches (rule #9).
    #[arg(long, env = "PARISH_DATA_DIR", value_name = "DIR")]
    data_dir: Option<PathBuf>,

    /// Built frontend directory (containing `index.html`).
    /// Same debug-vs-release resolution as `--data-dir`: ancestor-walk in
    /// debug, bare default + this flag (or `PARISH_STATIC_DIR`) in release.
    #[arg(long, env = "PARISH_STATIC_DIR", value_name = "DIR")]
    static_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    setup_tracing_and_otel();
    tracing::info!("Starting parish-server...");
    let cli = Cli::parse();

    let data_dir = cli.data_dir.unwrap_or_else(find_data_dir);
    let static_dir = cli.static_dir.unwrap_or_else(find_ui_dist_dir);
    tracing::info!(
        "Listening on port {} (data={}, static={})",
        cli.port,
        data_dir.display(),
        static_dir.display()
    );

    parish_server::run_server(cli.port, data_dir, static_dir).await
}

/// Sets up tracing and optional OpenTelemetry.
fn setup_tracing_and_otel() {
    std::fs::create_dir_all("logs").ok();
    let file_appender = tracing_appender::rolling::daily("logs", "parish-server.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let otel_provider = parish_server::tracing_setup::try_build_otel_provider("parish-server");
    let otel_tracer = otel_provider.as_ref().map(|p| p.tracer("parish-server"));

    let registry = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("parish=info")))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        );

    if let Some(tracer) = otel_tracer {
        registry.with(Some(OpenTelemetryLayer::new(tracer))).init();
    } else {
        registry
            .with(Option::<OpenTelemetryLayer<_, opentelemetry::trace::noop::NoopTracer>>::None)
            .init();
    }
}

/// Resolves the mod data directory when neither `--data-dir` nor
/// `PARISH_DATA_DIR` is supplied.
///
/// AGENTS.md rule #9: packaged and daemonised launches must resolve runtime
/// paths from explicit input, never by parent-walking `current_dir()`. So the
/// ancestor scan is **gated to debug builds** as a dev convenience; release
/// builds emit a warning and fall back to a single non-walking default that
/// operators are expected to override with `--data-dir` / `PARISH_DATA_DIR`.
fn find_data_dir() -> PathBuf {
    const MOD_REL: &str = "mods/rundale";
    walk_for_marker(
        MOD_REL,
        &["world.json"],
        MOD_REL,
        "data",
        "--data-dir / PARISH_DATA_DIR",
    )
}

/// Resolves the built-frontend directory when neither `--static-dir` nor
/// `PARISH_STATIC_DIR` is supplied. Same rule-9 gating as [`find_data_dir`].
fn find_ui_dist_dir() -> PathBuf {
    const DEFAULT_REL: &str = "apps/ui/dist";
    // The cwd-relative candidate set is only consulted under the debug-gated
    // walk; release builds use the single `DEFAULT_REL` default below.
    walk_for_candidates(
        &["apps/ui/dist", "parish/apps/ui/dist", "ui/dist"],
        "index.html",
        DEFAULT_REL,
        "static",
        "--static-dir / PARISH_STATIC_DIR",
    )
}

/// Debug-only ancestor walk for a single relative marker directory.
///
/// In **debug** builds, walks up to 4 ancestors of the startup cwd looking for
/// `<ancestor>/<rel>/<marker>`. In **release** builds the walk is skipped
/// entirely (rule #9) — it logs a warning and returns the bare default.
fn walk_for_marker(
    rel: &str,
    markers: &[&str],
    default_rel: &str,
    kind: &str,
    flag_hint: &str,
) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        for _ in 0..4 {
            let candidate = p.join(rel);
            if markers.iter().all(|m| candidate.join(m).exists()) {
                return candidate;
            }
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => break,
            }
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = (markers,); // silence unused in release
    fallback_default(default_rel, kind, flag_hint)
}

/// Debug-only ancestor walk over several relative candidates that share one
/// marker filename (used for the UI dist dir, which has multiple layouts).
fn walk_for_candidates(
    candidates: &[&str],
    marker: &str,
    default_rel: &str,
    kind: &str,
    flag_hint: &str,
) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        for _ in 0..4 {
            for c in candidates {
                if p.join(c).join(marker).exists() {
                    return p.join(c);
                }
            }
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => break,
            }
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = (candidates, marker); // silence unused in release
    fallback_default(default_rel, kind, flag_hint)
}

/// Shared rule-9 fallback: warn (release) that no explicit path was supplied
/// and return the bare default without any cwd parent-walk.
fn fallback_default(default_rel: &str, kind: &str, flag_hint: &str) -> PathBuf {
    #[cfg(not(debug_assertions))]
    tracing::warn!(
        "no explicit {kind} directory supplied; using '{default_rel}' relative to the \
         startup cwd without ancestor discovery (rule #9). Pass {flag_hint} in packaged \
         or daemonised deployments."
    );
    #[cfg(debug_assertions)]
    let _ = (kind, flag_hint);
    PathBuf::from(default_rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    // AC-1/AC-2: when no explicit path is supplied, the rule-9 fallback returns
    // the bare default verbatim — it never prepends ancestor path components
    // from a cwd parent-walk. This holds in both debug and release builds; in
    // debug the walk above may short-circuit before reaching the fallback, but
    // the fallback itself must remain a pure, non-walking default.
    #[test]
    fn fallback_default_returns_bare_default_without_walking() {
        let p = fallback_default("mods/rundale", "data", "--data-dir");
        assert_eq!(p, PathBuf::from("mods/rundale"));
        // No absolute prefix, no extra ancestor components.
        assert!(!p.is_absolute());
        assert_eq!(p.components().count(), 2);
    }

    // AC-2: the static-dir fallback is likewise non-walking.
    #[test]
    fn fallback_default_static_dir_is_bare() {
        let p = fallback_default("apps/ui/dist", "static", "--static-dir");
        assert_eq!(p, PathBuf::from("apps/ui/dist"));
        assert!(!p.is_absolute());
    }

    // AC-4 (release behaviour): in release builds the resolvers must NOT walk
    // ancestors — they return exactly the bare default regardless of cwd.
    // (In debug builds the dev-convenience walk is allowed, so this assertion
    // is only meaningful under `--release`.)
    #[cfg(not(debug_assertions))]
    #[test]
    fn release_resolvers_do_not_parent_walk() {
        assert_eq!(find_data_dir(), PathBuf::from("mods/rundale"));
        assert_eq!(find_ui_dist_dir(), PathBuf::from("apps/ui/dist"));
    }
}
