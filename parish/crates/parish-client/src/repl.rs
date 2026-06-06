use std::io::{self, BufRead};
use std::path::Path;

use anyhow::Result;

use crate::client::{CommandOpts, ParishClient};
use crate::render::render_response;

/// A trimmed line that should be ignored: blank or a `#` comment.
fn is_skippable(text: &str) -> bool {
    text.is_empty() || text.starts_with('#')
}

/// A trimmed line that ends the session: `quit`/`exit`, bare or slash-prefixed.
fn is_quit(text: &str) -> bool {
    matches!(text, "quit" | "/quit" | "exit" | "/exit")
}

pub async fn run_interactive(client: &ParishClient, json: bool) -> Result<()> {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let text = line.trim();
        if is_skippable(text) {
            continue;
        }
        if is_quit(text) {
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
        if is_skippable(text) {
            continue;
        }
        if is_quit(text) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_blank_and_comment_lines() {
        assert!(is_skippable(""));
        assert!(is_skippable("# a comment"));
        assert!(is_skippable("#"));
        assert!(!is_skippable("look"));
        assert!(!is_skippable("go to the church"));
    }

    #[test]
    fn recognizes_quit_aliases() {
        for q in ["quit", "/quit", "exit", "/exit"] {
            assert!(is_quit(q), "{q} should quit");
        }
        for keep in ["look", "quitter", "/quitt", "exits"] {
            assert!(!is_quit(keep), "{keep} should not quit");
        }
    }
}
