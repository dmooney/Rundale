//! `limerick-mcp` — stdio MCP server entry point.
//!
//! Launches a JSON-RPC 2.0 server on stdin/stdout and routes MCP `tools/call`
//! invocations into a [`limerick_mcp::backend::TauriBackend`]. The default
//! backend talks HTTP to a running `limerick-server`; pass `--base-url` to
//! point it at a non-default host or port.

use std::sync::Arc;

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

    let mut http = LimerickHttpBackend::new(&cli.base_url);
    if let Some(email) = cli.auth_email {
        http = http.with_auth_email(email);
    }
    let backend: Arc<dyn TauriBackend> = Arc::new(http);
    let server = Arc::new(McpServer::new(backend));

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let writer = ResponseWriter::new(stdout);
    serve(stdin, writer, server).await?;
    Ok(())
}
