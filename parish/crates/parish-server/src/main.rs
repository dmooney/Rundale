//! `parish-server` binary entry point.
//!
//! Standalone Axum HTTP/WebSocket server. Boots `run_server` from the
//! crate's library surface with paths resolved from CLI / env / cwd
//! lookups in that order.

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
    /// Defaults to walking the cwd for `mods/rundale/`.
    #[arg(long, env = "PARISH_DATA_DIR", value_name = "DIR")]
    data_dir: Option<PathBuf>,

    /// Built frontend directory (containing `index.html`).
    /// Defaults to walking the cwd for `apps/ui/dist/`.
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

/// Walks up to 4 ancestors of the cwd looking for `mods/rundale/world.json`.
///
/// Subject to AGENTS.md rule #9 — cwd-relative resolution breaks in
/// daemonised or `/tmp` deployments; operators should pass `--data-dir`
/// explicitly. Retained for parity with the legacy `parish-engine --web`
/// behaviour so existing dev workflows keep working.
fn find_data_dir() -> PathBuf {
    const MOD_REL: &str = "mods/rundale";
    let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..4 {
        if p.join(MOD_REL).join("world.json").exists() {
            return p.join(MOD_REL);
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
    PathBuf::from(MOD_REL)
}

/// Walks up to 4 ancestors of the cwd looking for a built frontend.
///
/// Subject to AGENTS.md rule #9 — operators should pass `--static-dir`
/// explicitly in packaged builds.
fn find_ui_dist_dir() -> PathBuf {
    let candidates = ["apps/ui/dist", "parish/apps/ui/dist", "ui/dist"];
    let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..4 {
        for c in &candidates {
            if p.join(c).join("index.html").exists() {
                return p.join(c);
            }
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
    PathBuf::from("apps/ui/dist")
}
