use anyhow::Result;
use parish::inference::client::{OllamaClient, OllamaProcess};
use parish::inference::{self, InferenceQueue};
use parish::input::{Command, InputResult, classify_input, parse_intent};
use parish::npc::{self, Npc, NpcAction};
use parish::tui::{self, App};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

/// Default Ollama model for NPC inference.
const DEFAULT_MODEL: &str = "qwen3:14b";

/// Default Ollama API base URL.
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Parish...");

    // Configuration from environment
    let ollama_url =
        std::env::var("PARISH_OLLAMA_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());
    let model = std::env::var("PARISH_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    // Ensure Ollama is running (start it if needed)
    let mut ollama_process = OllamaProcess::ensure_running(&ollama_url).await?;
    if ollama_process.was_started_by_us() {
        tracing::info!("Ollama started by Parish — will stop on exit");
    }

    // Initialize inference pipeline
    let client = OllamaClient::new(&ollama_url);
    let (tx, rx) = mpsc::channel(32);
    let _worker = inference::spawn_inference_worker(client.clone(), rx);
    let queue = InferenceQueue::new(tx);

    // Initialize app
    let mut app = App::new();
    app.inference_queue = Some(queue);
    app.npcs.push(Npc::new_test_npc());

    // Show initial location description
    let loc_name = app.world.current_location().name.clone();
    let loc_desc = app.world.current_location().description.clone();
    app.world.log(format!("— {} —", loc_name));
    app.world.log(loc_desc);
    app.world.log(String::new());

    // Show NPC presence
    for npc in &app.npcs {
        if npc.location == app.world.player_location {
            app.world.log(format!("{} is here.", npc.name));
        }
    }
    app.world.log(String::new());

    // Initialize terminal
    let mut terminal = tui::init_terminal()?;
    let mut request_id: u64 = 0;

    // Main game loop
    loop {
        // Draw frame
        terminal.draw(|frame| tui::draw(frame, &app))?;

        // Check for quit
        if app.should_quit {
            break;
        }

        // Handle input
        if let Some(raw_input) = tui::handle_input(&mut app, Duration::from_millis(100))? {
            app.world.log(format!("> {}", raw_input));

            match classify_input(&raw_input) {
                InputResult::SystemCommand(cmd) => {
                    handle_system_command(&mut app, cmd);
                }
                InputResult::GameInput(text) => {
                    // Find NPC at player's location
                    let npc = app
                        .npcs
                        .iter()
                        .find(|n| n.location == app.world.player_location)
                        .cloned();

                    if let Some(npc) = npc {
                        // Build context and send inference request
                        let system_prompt = npc::build_tier1_system_prompt(&npc);
                        let context = npc::build_tier1_context(&npc, &app.world, &text);

                        if let Some(queue) = &app.inference_queue {
                            request_id += 1;
                            app.world.log("...".to_string());

                            match queue
                                .send(request_id, model.clone(), context, Some(system_prompt))
                                .await
                            {
                                Ok(rx) => match rx.await {
                                    Ok(response) => {
                                        // Remove the "..." placeholder
                                        if app.world.text_log.last() == Some(&"...".to_string()) {
                                            app.world.text_log.pop();
                                        }

                                        if let Some(err) = &response.error {
                                            app.world.log(format!("[Ollama error: {}]", err));
                                        } else {
                                            render_npc_response(&mut app, &npc, &response.text);
                                        }
                                    }
                                    Err(_) => {
                                        if app.world.text_log.last() == Some(&"...".to_string()) {
                                            app.world.text_log.pop();
                                        }
                                        app.world.log("[Inference channel closed]".to_string());
                                    }
                                },
                                Err(e) => {
                                    app.world.log(format!("[Failed to send request: {}]", e));
                                }
                            }
                        } else {
                            app.world.log("[No inference engine available]".to_string());
                        }
                    } else {
                        // Try intent parsing for non-NPC actions
                        let intent = parse_intent(&client, &text, &model).await?;
                        match intent.intent {
                            parish::input::IntentKind::Look => {
                                let loc = app.world.current_location();
                                app.world.log(loc.description.clone());
                            }
                            _ => {
                                app.world.log("Nothing happens.".to_string());
                            }
                        }
                    }
                    app.world.log(String::new());
                }
            }
        }
    }

    // Restore terminal
    tui::restore_terminal(&mut terminal)?;

    // Stop Ollama if we started it
    ollama_process.stop();

    tracing::info!("Parish exited cleanly.");

    Ok(())
}

/// Handles a system command.
fn handle_system_command(app: &mut App, cmd: Command) {
    match cmd {
        Command::Quit => {
            app.world.log("Farewell.".to_string());
            app.should_quit = true;
        }
        Command::Pause => {
            app.world.clock.pause();
            app.world.log("[Time paused]".to_string());
        }
        Command::Resume => {
            app.world.clock.resume();
            app.world.log("[Time resumed]".to_string());
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
            app.world.log(format!(
                "Location: {} | {} | {} {}",
                loc, time, season, paused
            ));
        }
        Command::Help => {
            app.world.log("Commands:".to_string());
            app.world.log("  /quit     — Exit the game".to_string());
            app.world.log("  /pause    — Pause time".to_string());
            app.world.log("  /resume   — Resume time".to_string());
            app.world.log("  /status   — Show game status".to_string());
            app.world.log("  /help     — Show this help".to_string());
            app.world
                .log("  /save     — Save game (Phase 4)".to_string());
            app.world
                .log("  /fork <n> — Fork save (Phase 4)".to_string());
            app.world
                .log("  /load <n> — Load save (Phase 4)".to_string());
        }
        Command::Save | Command::Fork(_) | Command::Load(_) | Command::Branches | Command::Log => {
            app.world
                .log("[Not yet implemented — coming in Phase 4]".to_string());
        }
    }
    app.world.log(String::new());
}

/// Renders an NPC response, attempting to parse as structured JSON first.
fn render_npc_response(app: &mut App, npc: &Npc, response_text: &str) {
    // Try to parse as structured NpcAction
    if let Ok(action) = serde_json::from_str::<NpcAction>(response_text) {
        if let Some(dialogue) = &action.dialogue {
            app.world
                .log(format!("{} says: \"{}\"", npc.name, dialogue));
        }
        if !action.action.is_empty() && action.dialogue.is_none() {
            app.world.log(format!("{} {}.", npc.name, action.action));
        } else if !action.action.is_empty() {
            app.world.log(format!("({} {}.)", npc.name, action.action));
        }
    } else {
        // Fallback: treat the whole response as dialogue
        let trimmed = response_text.trim();
        if !trimmed.is_empty() {
            app.world.log(format!("{}: {}", npc.name, trimmed));
        }
    }
}
