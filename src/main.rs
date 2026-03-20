use anyhow::Result;
use clap::Parser;
use parish::headless;
use parish::inference::setup::{self, StdoutProgress};
use parish::inference::{self, InferenceQueue};
use parish::input::{Command, InputResult, classify_input, parse_intent};
use parish::npc::{
    self, Npc, SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary,
    parse_npc_stream_response,
};
use parish::tui::{self, App};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing_subscriber::EnvFilter;

/// Default Ollama API base URL.
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Parish — An Irish Living World Text Adventure
#[derive(Parser, Debug)]
#[command(name = "parish", version, about)]
struct Cli {
    /// Run in headless mode (plain stdin/stdout REPL, no TUI)
    #[arg(long)]
    headless: bool,

    /// Override the Ollama model (skips auto-detection)
    #[arg(long, env = "PARISH_MODEL")]
    model: Option<String>,

    /// Override the Ollama API URL
    #[arg(long, env = "PARISH_OLLAMA_URL", default_value = DEFAULT_OLLAMA_URL)]
    ollama_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Parish...");

    let cli = Cli::parse();

    // Run the full Ollama setup sequence
    let progress = StdoutProgress;
    let setup = setup::setup_ollama(&cli.ollama_url, cli.model.as_deref(), &progress).await?;

    if cli.headless {
        return headless::run_headless(setup).await;
    }

    // TUI mode
    let model = setup.model_name.clone();
    let client = setup.client.clone();
    let mut ollama_process = setup.process;

    // Initialize inference pipeline
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

    // Shared streaming state for the TUI render loop
    let streaming_buf: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let streaming_active: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

    // Main game loop
    loop {
        // Check if streaming tokens have arrived and update the log
        {
            let active = *streaming_active.lock().unwrap();
            if active {
                let mut buf = streaming_buf.lock().unwrap();
                if !buf.is_empty() {
                    // Update the last log line (the streaming line) with new tokens
                    if let Some(last) = app.world.text_log.last_mut() {
                        last.push_str(&buf);
                    }
                    buf.clear();
                }
            }
        }

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

                            // Create streaming channel
                            let (token_tx, mut token_rx) = mpsc::unbounded_channel::<String>();

                            // Start a streaming log line with NPC name prefix
                            app.world.log(format!("{}: ", npc.name));

                            // Mark streaming as active
                            *streaming_active.lock().unwrap() = true;

                            // Spawn separator-aware buffering task.
                            // Only puts dialogue text (before ---) into the shared buffer.
                            let buf_clone = Arc::clone(&streaming_buf);
                            let active_clone = Arc::clone(&streaming_active);
                            let stream_handle = tokio::spawn(async move {
                                let mut accumulated = String::new();
                                let mut displayed_len: usize = 0;
                                let mut separator_found = false;

                                while let Some(token) = token_rx.recv().await {
                                    accumulated.push_str(&token);

                                    if separator_found {
                                        continue;
                                    }

                                    // Check for separator (--- on its own line)
                                    if let Some((dialogue_end, _meta_start)) =
                                        find_response_separator(&accumulated)
                                    {
                                        if dialogue_end > displayed_len {
                                            let new_text = accumulated[displayed_len..dialogue_end]
                                                .to_string();
                                            buf_clone.lock().unwrap().push_str(&new_text);
                                        }
                                        separator_found = true;
                                        continue;
                                    }

                                    // Display tokens, holding back enough to detect separator
                                    let raw_end =
                                        accumulated.len().saturating_sub(SEPARATOR_HOLDBACK);
                                    let safe_end = floor_char_boundary(&accumulated, raw_end);
                                    if safe_end > displayed_len {
                                        let new_text =
                                            accumulated[displayed_len..safe_end].to_string();
                                        buf_clone.lock().unwrap().push_str(&new_text);
                                        displayed_len = safe_end;
                                    }
                                }

                                // Flush remaining if no separator found
                                if !separator_found && displayed_len < accumulated.len() {
                                    let remaining = accumulated[displayed_len..].to_string();
                                    buf_clone.lock().unwrap().push_str(&remaining);
                                }

                                *active_clone.lock().unwrap() = false;
                            });

                            match queue
                                .send(
                                    request_id,
                                    model.clone(),
                                    context,
                                    Some(system_prompt),
                                    Some(token_tx),
                                )
                                .await
                            {
                                Ok(mut rx) => {
                                    // Poll for response while continuing to render
                                    let response = loop {
                                        // Flush any buffered tokens to the log
                                        {
                                            let mut buf = streaming_buf.lock().unwrap();
                                            if !buf.is_empty() {
                                                if let Some(last) = app.world.text_log.last_mut() {
                                                    last.push_str(&buf);
                                                }
                                                buf.clear();
                                            }
                                        }

                                        // Draw frame to show streaming progress
                                        terminal.draw(|frame| tui::draw(frame, &app))?;

                                        // Check if response is ready (non-blocking)
                                        match rx.try_recv() {
                                            Ok(resp) => break Some(resp),
                                            Err(oneshot::error::TryRecvError::Empty) => {
                                                tokio::time::sleep(Duration::from_millis(50)).await;
                                                continue;
                                            }
                                            Err(oneshot::error::TryRecvError::Closed) => {
                                                break None;
                                            }
                                        }
                                    };

                                    // Wait for streaming task to finish
                                    let _ = stream_handle.await;

                                    // Flush any remaining filtered tokens
                                    {
                                        let mut buf = streaming_buf.lock().unwrap();
                                        if !buf.is_empty() {
                                            if let Some(last) = app.world.text_log.last_mut() {
                                                last.push_str(&buf);
                                            }
                                            buf.clear();
                                        }
                                    }

                                    match response {
                                        Some(resp) => {
                                            if let Some(err) = &resp.error {
                                                app.world.log(format!("[Ollama error: {}]", err));
                                            } else {
                                                // Parse metadata silently
                                                let parsed = parse_npc_stream_response(&resp.text);
                                                if let Some(meta) = &parsed.metadata {
                                                    tracing::debug!(
                                                        "NPC metadata: action={}, mood={}",
                                                        meta.action,
                                                        meta.mood
                                                    );
                                                }
                                            }
                                        }
                                        None => {
                                            app.world.log("[Inference channel closed]".to_string());
                                        }
                                    }
                                }
                                Err(e) => {
                                    *streaming_active.lock().unwrap() = false;
                                    let _ = stream_handle.await;
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
