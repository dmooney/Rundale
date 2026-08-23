use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

mod catalog_command;
mod client;
mod config_command;
mod render;
mod repl;
mod session;

#[derive(Parser)]
#[command(name = "parish", about = "CLI client for a running Parish game server")]
struct Cli {
    /// Single command to run (omit for interactive REPL)
    command: Option<String>,

    /// Run commands from a script file (one per line)
    #[arg(long, value_name = "FILE")]
    script: Option<PathBuf>,

    /// Print raw JSON response instead of rendered prose
    #[arg(long)]
    json: bool,

    /// Server base URL
    #[arg(long, env = "PARISH_SERVER", default_value = "http://localhost:3001")]
    server: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    if config_command::is_invocation() {
        return config_command::run();
    }
    if catalog_command::is_invocation() {
        return catalog_command::run().await;
    }
    let cli = Cli::parse();
    let client = client::ParishClient::new(&cli.server)?;

    if let Some(script) = cli.script {
        repl::run_script(&client, &script, cli.json).await
    } else if let Some(cmd) = cli.command {
        let resp = client.post_command(&cmd, Default::default()).await?;
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&resp)?);
        } else {
            print!("{}", render::render_response(&resp));
        }
        Ok(())
    } else {
        repl::run_interactive(&client, cli.json).await
    }
}
