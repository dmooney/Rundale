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
use parish::world::description::{format_exits, render_description};
use parish::world::movement::{self, MovementResult};
use std::path::Path;
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

    // Initialize app — load parish data if available
    let mut app = App::new();
    let parish_path = Path::new("data/parish.json");
    if parish_path.exists() {
        match parish::world::WorldState::from_parish_file(parish_path, parish::world::LocationId(1))
        {
            Ok(world) => app.world = world,
            Err(e) => tracing::warn!("Failed to load parish data: {}", e),
        }
    }
    app.inference_queue = Some(queue);
    app.npcs.push(Npc::new_test_npc());

    // Show initial location description
    show_location_arrival(&mut app);

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
        terminal.draw(|frame| tui::draw(frame, &mut app))?;

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
                    // Always parse intent first so Move/Look work even with NPCs present
                    let intent = parse_intent(&client, &text, &model).await?;

                    match intent.intent {
                        parish::input::IntentKind::Move => {
                            if let Some(target) = &intent.target {
                                handle_movement(&mut app, target);
                            } else {
                                app.world.log("Go where?".to_string());
                            }
                        }
                        parish::input::IntentKind::Look => {
                            show_location_description(&mut app);
                        }
                        _ => {
                            // Route to NPC conversation if one is present
                            let npc = app
                                .npcs
                                .iter()
                                .find(|n| n.location == app.world.player_location)
                                .cloned();

                            if let Some(npc) = npc {
                                let system_prompt = npc::build_tier1_system_prompt(&npc);
                                let context = npc::build_tier1_context(&npc, &app.world, &text);

                                if let Some(queue) = &app.inference_queue {
                                    request_id += 1;

                                    let (token_tx, mut token_rx) =
                                        mpsc::unbounded_channel::<String>();

                                    app.world.log(format!("{}: ", npc.name));
                                    *streaming_active.lock().unwrap() = true;

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

                                            if let Some((dialogue_end, _meta_start)) =
                                                find_response_separator(&accumulated)
                                            {
                                                if dialogue_end > displayed_len {
                                                    let new_text = accumulated
                                                        [displayed_len..dialogue_end]
                                                        .to_string();
                                                    buf_clone.lock().unwrap().push_str(&new_text);
                                                }
                                                separator_found = true;
                                                continue;
                                            }

                                            let raw_end = accumulated
                                                .len()
                                                .saturating_sub(SEPARATOR_HOLDBACK);
                                            let safe_end =
                                                floor_char_boundary(&accumulated, raw_end);
                                            if safe_end > displayed_len {
                                                let new_text = accumulated[displayed_len..safe_end]
                                                    .to_string();
                                                buf_clone.lock().unwrap().push_str(&new_text);
                                                displayed_len = safe_end;
                                            }
                                        }

                                        if !separator_found && displayed_len < accumulated.len() {
                                            let remaining =
                                                accumulated[displayed_len..].to_string();
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
                                            let response = loop {
                                                {
                                                    let mut buf = streaming_buf.lock().unwrap();
                                                    if !buf.is_empty() {
                                                        if let Some(last) =
                                                            app.world.text_log.last_mut()
                                                        {
                                                            last.push_str(&buf);
                                                        }
                                                        buf.clear();
                                                    }
                                                }

                                                terminal
                                                    .draw(|frame| tui::draw(frame, &mut app))?;

                                                match rx.try_recv() {
                                                    Ok(resp) => break Some(resp),
                                                    Err(oneshot::error::TryRecvError::Empty) => {
                                                        tokio::time::sleep(Duration::from_millis(
                                                            50,
                                                        ))
                                                        .await;
                                                        continue;
                                                    }
                                                    Err(oneshot::error::TryRecvError::Closed) => {
                                                        break None;
                                                    }
                                                }
                                            };

                                            let _ = stream_handle.await;

                                            {
                                                let mut buf = streaming_buf.lock().unwrap();
                                                if !buf.is_empty() {
                                                    if let Some(last) =
                                                        app.world.text_log.last_mut()
                                                    {
                                                        last.push_str(&buf);
                                                    }
                                                    buf.clear();
                                                }
                                            }

                                            match response {
                                                Some(resp) => {
                                                    if let Some(err) = &resp.error {
                                                        app.world.log(format!(
                                                            "[Ollama error: {}]",
                                                            err
                                                        ));
                                                    } else {
                                                        let parsed =
                                                            parse_npc_stream_response(&resp.text);
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
                                                    app.world.log(
                                                        "[Inference channel closed]".to_string(),
                                                    );
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            *streaming_active.lock().unwrap() = false;
                                            let _ = stream_handle.await;
                                            app.world
                                                .log(format!("[Failed to send request: {}]", e));
                                        }
                                    }
                                } else {
                                    app.world.log("[No inference engine available]".to_string());
                                }
                            } else {
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

/// Shows the current location with description, NPCs, and exits.
fn show_location_arrival(app: &mut App) {
    let loc_name = app.world.current_location().name.clone();
    app.world.log(format!("— {} —", loc_name));

    // Render dynamic description if graph is loaded, else use static description
    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let weather = app.world.weather.clone();
        let npc_names: Vec<&str> = app
            .npcs
            .iter()
            .filter(|n| n.location == app.world.player_location)
            .map(|n| n.name.as_str())
            .collect();
        let desc = render_description(loc_data, tod, &weather, &npc_names);
        app.world.log(desc);
    } else {
        let desc = app.world.current_location().description.clone();
        app.world.log(desc);
    }

    // Show NPCs present
    for npc in &app.npcs {
        if npc.location == app.world.player_location {
            app.world.log(format!("{} is here.", npc.name));
        }
    }

    // Show exits
    let exits = format_exits(app.world.player_location, &app.world.graph);
    app.world.log(exits);
    app.world.log(String::new());
}

/// Shows current location description and exits (for /look or IntentKind::Look).
fn show_location_description(app: &mut App) {
    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let weather = app.world.weather.clone();
        let npc_names: Vec<&str> = app
            .npcs
            .iter()
            .filter(|n| n.location == app.world.player_location)
            .map(|n| n.name.as_str())
            .collect();
        let desc = render_description(loc_data, tod, &weather, &npc_names);
        app.world.log(desc);
    } else {
        let desc = app.world.current_location().description.clone();
        app.world.log(desc);
    }

    let exits = format_exits(app.world.player_location, &app.world.graph);
    app.world.log(exits);
}

/// Handles a movement command: resolve, travel, advance clock, show arrival.
fn handle_movement(app: &mut App, target: &str) {
    let result = movement::resolve_movement(target, &app.world.graph, app.world.player_location);

    match result {
        MovementResult::Arrived {
            destination,
            minutes,
            narration,
            ..
        } => {
            // Show travel narration
            app.world.log(narration);
            app.world.log(String::new());

            // Advance game clock
            app.world.clock.advance(minutes as i64);

            // Update player location
            app.world.player_location = destination;

            // Update legacy locations map with current position
            if let Some(data) = app.world.graph.get(destination) {
                app.world
                    .locations
                    .entry(destination)
                    .or_insert_with(|| parish::world::Location {
                        id: destination,
                        name: data.name.clone(),
                        description: data.description_template.clone(),
                        indoor: data.indoor,
                        public: data.public,
                    });
            }

            // Show new location
            show_location_arrival(app);
        }
        MovementResult::AlreadyHere => {
            app.world.log("You are already here.".to_string());
        }
        MovementResult::NotFound(name) => {
            app.world
                .log(format!("You don't know how to get to \"{}\".", name));

            // Show available exits as a hint
            let exits = format_exits(app.world.player_location, &app.world.graph);
            app.world.log(exits);
        }
    }
}
