//! Headless CLI mode for testing without the TUI.
//!
//! Provides a simple stdin/stdout REPL that reuses the same game logic
//! (NPC inference, intent parsing, system commands) as the TUI mode.
//! Activated with `--headless` on the command line.

use crate::inference::client::OllamaClient;
use crate::inference::setup::OllamaSetup;
use crate::inference::{self, InferenceQueue};
use crate::input::{Command, InputResult, classify_input, parse_intent};
use crate::npc::{self, Npc, NpcAction};
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
            println!("...");

            match queue
                .send(*request_id, model.to_string(), context, Some(system_prompt))
                .await
            {
                Ok(rx) => match rx.await {
                    Ok(response) => {
                        if let Some(err) = &response.error {
                            println!("[Ollama error: {}]", err);
                        } else {
                            render_headless_npc_response(&npc, &response.text);
                        }
                    }
                    Err(_) => {
                        println!("[Inference channel closed]");
                    }
                },
                Err(e) => {
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

/// Renders an NPC response to stdout, attempting structured JSON first.
fn render_headless_npc_response(npc: &Npc, response_text: &str) {
    if let Ok(action) = serde_json::from_str::<NpcAction>(response_text) {
        if let Some(dialogue) = &action.dialogue {
            println!("{} says: \"{}\"", npc.name, dialogue);
        }
        if !action.action.is_empty() && action.dialogue.is_none() {
            println!("{} {}.", npc.name, action.action);
        } else if !action.action.is_empty() {
            println!("({} {}.)", npc.name, action.action);
        }
    } else {
        let trimmed = response_text.trim();
        if !trimmed.is_empty() {
            println!("{}: {}", npc.name, trimmed);
        }
    }
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

    #[test]
    fn test_render_headless_npc_response_json() {
        let npc = Npc::new_test_npc();
        let json = r#"{"action": "speaks", "dialogue": "Hello there!", "mood": "friendly"}"#;
        // Should not panic
        render_headless_npc_response(&npc, json);
    }

    #[test]
    fn test_render_headless_npc_response_plain() {
        let npc = Npc::new_test_npc();
        render_headless_npc_response(&npc, "Well, hello there stranger!");
    }

    #[test]
    fn test_render_headless_npc_response_empty() {
        let npc = Npc::new_test_npc();
        render_headless_npc_response(&npc, "");
    }

    #[test]
    fn test_render_headless_npc_response_action_only() {
        let npc = Npc::new_test_npc();
        let json = r#"{"action": "nods slowly", "mood": "thoughtful"}"#;
        render_headless_npc_response(&npc, json);
    }
}
