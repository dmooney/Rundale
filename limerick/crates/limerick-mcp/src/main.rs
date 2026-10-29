//! `limerick-mcp` — stdio MCP server entry point.
//!
//! Launches a JSON-RPC 2.0 server on stdin/stdout and routes MCP `tools/call`
//! invocations into a [`limerick_mcp::backend::TauriBackend`]. The default
//! backend talks HTTP to a running `limerick-server`; pass `--base-url` to
//! point it at a non-default host or port.

use limerick_mcp::session_host::SessionHostBackend;
use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use limerick_mcp::backend::{LimerickHttpBackend, TauriBackend};
use limerick_mcp::jsonrpc::{ResponseWriter, serve};
use limerick_mcp::mcp::McpServer;

#[derive(Parser, Debug)]
#[command(
    name = "limerick-mcp",
    about = "MCP stdio server that drives a Limerick (or compatible Tauri) instance"
)]
struct Cli {
    /// Backend to use; HTTP remains the default.
    #[arg(long, default_value = "http", value_parser = ["http", "session-host"])]
    backend: String,
    /// Absolute path to the session host executable.
    #[arg(long)]
    host_executable: Option<PathBuf>,
    #[arg(long)]
    mod_dir: Option<PathBuf>,
    #[arg(long)]
    save_dir: Option<PathBuf>,
    /// Inference mode for the session host (defaults to mock).
    #[arg(long, default_value = "mock", value_parser = ["mock", "fixture", "sse", "live"])]
    inference: String,
    #[arg(long)]
    sse_fixture: Option<PathBuf>,
    /// Base URL of the limerick-server backend (without trailing slash).
    #[arg(long, default_value = "http://127.0.0.1:3030")]
    base_url: String,

    /// Optional Cf-Access email forwarded with each request. Useful when the
    /// target server is gated behind Cloudflare Access.
    #[arg(long, env = "LIMERICK_MCP_AUTH_EMAIL")]
    auth_email: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logs go to stderr — stdout is reserved for the JSON-RPC stream.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();
    tracing::info!(base_url = %cli.base_url, "limerick-mcp starting");

    let backend: Arc<dyn TauriBackend> = if cli.backend == "session-host" {
        Arc::new(
            SessionHostBackend::spawn(
                cli.host_executable
                    .ok_or_else(|| anyhow::anyhow!("--host-executable is required"))?,
                cli.mod_dir
                    .ok_or_else(|| anyhow::anyhow!("--mod-dir is required"))?,
                cli.save_dir
                    .ok_or_else(|| anyhow::anyhow!("--save-dir is required"))?,
                &cli.inference,
                cli.sse_fixture,
            )
            .await?,
        )
    } else {
        let mut http = LimerickHttpBackend::new(&cli.base_url);
        if let Some(email) = cli.auth_email {
            http = http.with_auth_email(email);
        }
        Arc::new(http)
    };
    let server = Arc::new(McpServer::new(backend.clone()));

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let writer = ResponseWriter::new(stdout);
    let result = serve(stdin, writer, server).await;
    backend.shutdown().await;
    result?;
    Ok(())
}
