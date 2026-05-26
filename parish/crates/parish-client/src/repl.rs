use std::io::{self, BufRead};
use std::path::Path;

use anyhow::Result;

use crate::client::{CommandOpts, ParishClient};
use crate::render::render_response;

pub async fn run_interactive(client: &ParishClient, json: bool) -> Result<()> {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let text = line.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        if text == "quit" || text == "/quit" || text == "exit" || text == "/exit" {
            break;
        }
        send(client, text, json).await?;
    }
    Ok(())
}

pub async fn run_script(client: &ParishClient, path: &Path, json: bool) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read script {}: {e}", path.display()))?;
    for line in content.lines() {
        let text = line.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        if text == "quit" || text == "/quit" || text == "exit" || text == "/exit" {
            break;
        }
        send(client, text, json).await?;
    }
    Ok(())
}

async fn send(client: &ParishClient, text: &str, json: bool) -> Result<()> {
    let resp = client.post_command(text, CommandOpts::default()).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        print!("{}", render_response(&resp));
    }
    Ok(())
}
