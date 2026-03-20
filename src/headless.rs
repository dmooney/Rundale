//! Headless CLI mode for testing without the TUI.
//!
//! Provides a simple stdin/stdout REPL that reuses the same game logic
//! (NPC inference, intent parsing, system commands) as the TUI mode.
//! Activated with `--headless` on the command line.

use crate::inference::client::OllamaClient;
use crate::inference::setup::OllamaSetup;
use crate::inference::{self, InferenceQueue};
use crate::input::{Command, InputResult, classify_input, parse_intent};
use crate::npc::{
    self, Npc, SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary,
    parse_npc_stream_response,
};
use crate::tui::App;
use anyhow::Result;
use std::io::{BufRead, Write};
use tokio::sync::mpsc;

/// Runs the game in headless mode with a plain stdin/stdout REPL.
///
/// Sets up the inference pipeline, initializes the game world with
/// the test NPC, and enters a read-eval-print loop. Each line of
/// input is processed identically to TUI mode.
pub async fn run_headless(setup: OllamaSetup) -> Result<()> {
    let OllamaSetup {
        process: _process,
        client,
        model_name,
        gpu_info,
    } = setup;

    println!("=== Parish — Headless Mode ===");
    println!("GPU: {}", gpu_info);
    println!("Model: {}", model_name);
    println!("Type /help for commands, /quit to exit.");
    println!();

    // Initialize inference pipeline
    let (tx, rx) = mpsc::channel(32);
    let _worker = inference::spawn_inference_worker(client.clone(), rx);
    let queue = InferenceQueue::new(tx);

    // Initialize app state
    let mut app = App::new();
    app.inference_queue = Some(queue);
    app.npcs.push(Npc::new_test_npc());

    // Show initial location
    let loc_name = app.world.current_location().name.clone();
    let loc_desc = app.world.current_location().description.clone();
    println!("--- {} ---", loc_name);
    println!("{}", loc_desc);
    println!();

    // Show NPC presence
    for npc in &app.npcs {
        if npc.location == app.world.player_location {
            println!("{} is here.", npc.name);
        }
    }
    println!();

    let mut request_id: u64 = 0;
    let stdin = std::io::stdin();
    let reader = stdin.lock();

    for line in reader.lines() {
        let raw_input = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = raw_input.trim().to_string();
        if trimmed.is_empty() {
            print!("> ");
            std::io::stdout().flush().ok();
            continue;
        }

        match classify_input(&trimmed) {
            InputResult::SystemCommand(cmd) => {
                if handle_headless_command(&mut app, cmd) {
                    break;
                }
            }
            InputResult::GameInput(text) => {
                handle_headless_game_input(&mut app, &client, &model_name, &text, &mut request_id)
                    .await?;
            }
        }

        if app.should_quit {
            break;
        }

        print!("> ");
        std::io::stdout().flush().ok();
    }

    println!("Farewell.");
    Ok(())
}

/// Handles a system command in headless mode. Returns true if the game should exit.
fn handle_headless_command(app: &mut App, cmd: Command) -> bool {
    match cmd {
        Command::Quit => {
            app.should_quit = true;
            true
        }
        Command::Pause => {
            app.world.clock.pause();
            println!("[Time paused]");
            false
        }
        Command::Resume => {
            app.world.clock.resume();
            println!("[Time resumed]");
            false
        }
        Command::Status => {
            let time = app.world.clock.time_of_day();
            let season = app.world.clock.season();
            let loc = app.world.current_location().name.clone();
            let paused = if app.world.clock.is_paused() {
                " (paused)"
            } else {
                ""
            };
            println!("Location: {} | {} | {}{}", loc, time, season, paused);
            false
        }
        Command::Help => {
            println!("Commands:");
            println!("  /quit     - Exit the game");
            println!("  /pause    - Pause time");
            println!("  /resume   - Resume time");
            println!("  /status   - Show game status");
            println!("  /help     - Show this help");
            println!("  /save     - Save game (Phase 4)");
            println!("  /fork <n> - Fork save (Phase 4)");
            println!("  /load <n> - Load save (Phase 4)");
            false
        }
        Command::Save | Command::Fork(_) | Command::Load(_) | Command::Branches | Command::Log => {
            println!("[Not yet implemented — coming in Phase 4]");
            false
        }
    }
}

/// Handles game input (NPC interaction or intent parsing) in headless mode.
async fn handle_headless_game_input(
    app: &mut App,
    client: &OllamaClient,
    model: &str,
    text: &str,
    request_id: &mut u64,
) -> Result<()> {
    let npc = app
        .npcs
        .iter()
        .find(|n| n.location == app.world.player_location)
        .cloned();

    if let Some(npc) = npc {
        let system_prompt = npc::build_tier1_system_prompt(&npc);
        let context = npc::build_tier1_context(&npc, &app.world, text);

        if let Some(queue) = &app.inference_queue {
            *request_id += 1;

            // Create streaming channel
            let (token_tx, mut token_rx) = mpsc::unbounded_channel::<String>();

            // Print NPC name prefix, then stream tokens inline
            print!("{}: ", npc.name);
            std::io::stdout().flush().ok();

            match queue
                .send(
                    *request_id,
                    model.to_string(),
                    context,
                    Some(system_prompt),
                    Some(token_tx),
                )
                .await
            {
                Ok(rx) => {
                    // Stream tokens with separator-aware filtering.
                    // Hold back SEP_LEN chars to detect the separator
                    // before it reaches the display.
                    let stream_handle = tokio::spawn(async move {
                        let mut accumulated = String::new();
                        let mut displayed_len: usize = 0;
                        let mut separator_found = false;

                        while let Some(token) = token_rx.recv().await {
                            accumulated.push_str(&token);

                            if separator_found {
                                continue;
                            }

                            // Check for separator (--- on its own line, flexible whitespace)
                            if let Some((dialogue_end, _meta_start)) =
                                find_response_separator(&accumulated)
                            {
                                // Display any remaining dialogue before separator
                                if dialogue_end > displayed_len {
                                    let new_text = &accumulated[displayed_len..dialogue_end];
                                    print!("{}", new_text);
                                    std::io::stdout().flush().ok();
                                }
                                separator_found = true;
                                continue;
                            }

                            // Display tokens, holding back enough to detect separator
                            let raw_end = accumulated.len().saturating_sub(SEPARATOR_HOLDBACK);
                            let safe_end = floor_char_boundary(&accumulated, raw_end);
                            if safe_end > displayed_len {
                                let new_text = &accumulated[displayed_len..safe_end];
                                print!("{}", new_text);
                                std::io::stdout().flush().ok();
                                displayed_len = safe_end;
                            }
                        }

                        // Stream ended — flush remaining if no separator found
                        if !separator_found && displayed_len < accumulated.len() {
                            let remaining = &accumulated[displayed_len..];
                            print!("{}", remaining);
                            std::io::stdout().flush().ok();
                        }

                        println!();
                        accumulated
                    });

                    // Wait for the full response
                    match rx.await {
                        Ok(response) => {
                            let _streamed = stream_handle.await.unwrap_or_default();

                            if let Some(err) = &response.error {
                                println!("[Ollama error: {}]", err);
                            } else {
                                // Parse metadata silently (dialogue already displayed)
                                let parsed = parse_npc_stream_response(&response.text);
                                if let Some(meta) = &parsed.metadata {
                                    tracing::debug!(
                                        "NPC metadata: action={}, mood={}",
                                        meta.action,
                                        meta.mood
                                    );
                                }
                            }
                        }
                        Err(_) => {
                            let _ = stream_handle.await;
                            println!("[Inference channel closed]");
                        }
                    }
                }
                Err(e) => {
                    println!();
                    println!("[Failed to send request: {}]", e);
                }
            }
        } else {
            println!("[No inference engine available]");
        }
    } else {
        let intent = parse_intent(client, text, model).await?;
        match intent.intent {
            crate::input::IntentKind::Look => {
                let loc = app.world.current_location();
                println!("{}", loc.description);
            }
            _ => {
                println!("Nothing happens.");
            }
        }
    }

    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::App;

    #[test]
    fn test_handle_headless_command_quit() {
        let mut app = App::new();
        let result = handle_headless_command(&mut app, Command::Quit);
        assert!(result);
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_headless_command_pause() {
        let mut app = App::new();
        let result = handle_headless_command(&mut app, Command::Pause);
        assert!(!result);
        assert!(app.world.clock.is_paused());
    }

    #[test]
    fn test_handle_headless_command_resume() {
        let mut app = App::new();
        app.world.clock.pause();
        let result = handle_headless_command(&mut app, Command::Resume);
        assert!(!result);
        assert!(!app.world.clock.is_paused());
    }

    #[test]
    fn test_handle_headless_command_help() {
        let mut app = App::new();
        let result = handle_headless_command(&mut app, Command::Help);
        assert!(!result);
    }

    #[test]
    fn test_handle_headless_command_status() {
        let mut app = App::new();
        let result = handle_headless_command(&mut app, Command::Status);
        assert!(!result);
    }

    #[test]
    fn test_handle_headless_command_unimplemented() {
        let mut app = App::new();
        assert!(!handle_headless_command(&mut app, Command::Save));
        assert!(!handle_headless_command(
            &mut app,
            Command::Fork("test".to_string())
        ));
        assert!(!handle_headless_command(
            &mut app,
            Command::Load("test".to_string())
        ));
        assert!(!handle_headless_command(&mut app, Command::Branches));
        assert!(!handle_headless_command(&mut app, Command::Log));
    }
}
