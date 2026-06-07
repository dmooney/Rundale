use std::io::{self, BufRead};
use std::path::Path;

use anyhow::Result;

use crate::client::{CommandOpts, ParishClient};
use crate::render::render_response;

/// Whether a trimmed input line should be skipped without sending: blank lines
/// and `#`-prefixed comments are ignored by both the interactive and script
/// loops. Extracted as a pure helper so the line-filter contract is unit-
/// testable without a live server (TD-001).
fn should_skip(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Whether a trimmed input line is a quit sentinel that ends the loop.
/// Accepts `quit`/`exit` with or without a leading slash.
fn is_quit(trimmed: &str) -> bool {
    matches!(trimmed, "quit" | "/quit" | "exit" | "/exit")
}

pub async fn run_interactive(client: &ParishClient, json: bool) -> Result<()> {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let text = line.trim();
        if should_skip(text) {
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
        if should_skip(text) {
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
    use super::{is_quit, should_skip};

    /// Mirrors the per-line decision both loops make: returns the list of lines
    /// that would actually be sent, stopping at the first quit sentinel. This is
    /// the exact filter `run_interactive` / `run_script` apply, lifted out so it
    /// can be tested without a server.
    fn lines_to_send(raw: &str) -> Vec<&str> {
        let mut out = Vec::new();
        for line in raw.lines() {
            let text = line.trim();
            if should_skip(text) {
                continue;
            }
            if is_quit(text) {
                break;
            }
            out.push(text);
        }
        out
    }

    // AC-1: blank and comment lines are skipped.
    #[test]
    fn should_skip_blank_and_comment_lines() {
        assert!(should_skip(""));
        assert!(should_skip("# a comment"));
        assert!(should_skip("#"));
        assert!(!should_skip("look"));
        assert!(!should_skip("go to the church"));
    }

    // AC-1: every quit/exit spelling (with and without slash) ends the loop.
    #[test]
    fn is_quit_recognises_all_sentinels() {
        for q in ["quit", "/quit", "exit", "/exit"] {
            assert!(is_quit(q), "{q} should be a quit sentinel");
        }
        assert!(!is_quit("look"));
        assert!(!is_quit("quitter")); // not an exact match
        assert!(!is_quit("/quote"));
    }

    // AC-1: the loop filter trims, drops blanks/comments, and stops at quit.
    #[test]
    fn loop_filter_drops_blanks_comments_and_stops_at_quit() {
        let script = "\
look
   # leading-whitespace comment

  go north
/quit
this line is never reached";
        assert_eq!(
            lines_to_send(script),
            vec!["look", "go north"],
            "blank/comment lines dropped, whitespace trimmed, lines after /quit ignored"
        );
    }

    // AC-1: a script with no quit sentinel sends every non-skipped line.
    #[test]
    fn loop_filter_without_quit_sends_all_meaningful_lines() {
        let script = "look\n# note\ntalk to Bridget\n";
        assert_eq!(lines_to_send(script), vec!["look", "talk to Bridget"]);
    }
}
