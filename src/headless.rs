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
use crate::world::description::{format_exits, render_description};
use crate::world::movement::{self, MovementResult};
use anyhow::Result;
use std::io::{BufRead, Write};
use std::path::Path;
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

    // Initialize app state — load parish data if available
    let mut app = App::new();
    let parish_path = Path::new("data/parish.json");
    if parish_path.exists() {
        match crate::world::WorldState::from_parish_file(parish_path, crate::world::LocationId(1)) {
            Ok(world) => app.world = world,
            Err(e) => eprintln!("Warning: Failed to load parish data: {}", e),
        }
    }
    app.inference_queue = Some(queue);
    app.npcs.push(Npc::new_test_npc());

    // Show initial location
    print_location_arrival(&app);

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

    println!("Safe home to ye. May the road rise to meet you.");
    Ok(())
}

/// Handles a system command in headless mode. Returns true if the game should exit.
/// Atmospheric idle messages shown when no NPC is present for conversation.
const HEADLESS_IDLE_MESSAGES: &[&str] = &[
    "The wind stirs, but nothing else.",
    "Only the sound of a distant crow.",
    "A dog barks somewhere beyond the hill.",
    "The clouds shift. The parish carries on.",
    "Somewhere nearby, a door creaks shut.",
    "A wren hops along the stone wall and vanishes.",
    "The smell of turf smoke drifts from a cottage chimney.",
];

/// Headless idle message counter.
static HEADLESS_IDLE_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Handles a system command in headless mode. Returns true if the game should exit.
fn handle_headless_command(app: &mut App, cmd: Command) -> bool {
    match cmd {
        Command::Quit => {
            app.should_quit = true;
            true
        }
        Command::Pause => {
            app.world.clock.pause();
            println!("The clocks of the parish stand still.");
            false
        }
        Command::Resume => {
            app.world.clock.resume();
            println!("Time stirs again in the parish.");
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
            println!("A few things ye might say:");
            println!("  /quit     - Take your leave");
            println!("  /pause    - Hold time still");
            println!("  /resume   - Let time flow again");
            println!("  /status   - Where am I?");
            println!("  /irish    - Toggle Irish words sidebar (TUI only)");
            println!("  /help     - Show this help");
            println!("  /save     - Save game (not yet arrived)");
            println!("  /fork <n> - Fork save (not yet arrived)");
            println!("  /load <n> - Load save (not yet arrived)");
            false
        }
        Command::ToggleSidebar => {
            println!("The pronunciation sidebar is only available in TUI mode.");
            false
        }
        Command::Save | Command::Fork(_) | Command::Load(_) | Command::Branches | Command::Log => {
            println!("That particular skill hasn't arrived in the parish yet. Patience now.");
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
    // Always parse intent first so Move/Look work even with NPCs present
    let intent = parse_intent(client, text, model).await?;

    match intent.intent {
        crate::input::IntentKind::Move => {
            if let Some(target) = &intent.target {
                handle_headless_movement(app, target);
            } else {
                println!("And where would ye be off to?");
            }
        }
        crate::input::IntentKind::Look => {
            print_location_description(app);
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
                let context = npc::build_tier1_context(&npc, &app.world, text);

                if let Some(queue) = &app.inference_queue {
                    *request_id += 1;

                    let (token_tx, mut token_rx) = mpsc::unbounded_channel::<String>();

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
                                            let new_text =
                                                &accumulated[displayed_len..dialogue_end];
                                            print!("{}", new_text);
                                            std::io::stdout().flush().ok();
                                        }
                                        separator_found = true;
                                        continue;
                                    }

                                    let raw_end =
                                        accumulated.len().saturating_sub(SEPARATOR_HOLDBACK);
                                    let safe_end = floor_char_boundary(&accumulated, raw_end);
                                    if safe_end > displayed_len {
                                        let new_text = &accumulated[displayed_len..safe_end];
                                        print!("{}", new_text);
                                        std::io::stdout().flush().ok();
                                        displayed_len = safe_end;
                                    }
                                }

                                if !separator_found && displayed_len < accumulated.len() {
                                    let remaining = &accumulated[displayed_len..];
                                    print!("{}", remaining);
                                    std::io::stdout().flush().ok();
                                }

                                println!();
                                accumulated
                            });

                            match rx.await {
                                Ok(response) => {
                                    let _streamed = stream_handle.await.unwrap_or_default();

                                    if let Some(err) = &response.error {
                                        println!(
                                            "[The parish storyteller has lost the thread: {}]",
                                            err
                                        );
                                    } else {
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
                                    println!("[The storyteller has wandered off mid-tale.]");
                                }
                            }
                        }
                        Err(e) => {
                            println!();
                            println!("[The storyteller couldn't hear ye: {}]", e);
                        }
                    }
                } else {
                    println!("[No storyteller could be found in the parish today.]");
                }
            } else {
                let idx = HEADLESS_IDLE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                println!(
                    "{}",
                    HEADLESS_IDLE_MESSAGES[idx % HEADLESS_IDLE_MESSAGES.len()]
                );
            }
        }
    }

    println!();
    Ok(())
}

/// Prints the current location with description, NPCs, and exits (headless).
fn print_location_arrival(app: &App) {
    let loc_name = app.world.current_location().name.clone();
    println!("--- {} ---", loc_name);

    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let npc_names: Vec<&str> = app
            .npcs
            .iter()
            .filter(|n| n.location == app.world.player_location)
            .map(|n| n.name.as_str())
            .collect();
        let desc = render_description(loc_data, tod, &app.world.weather, &npc_names);
        println!("{}", desc);
    } else {
        println!("{}", app.world.current_location().description);
    }

    for npc in &app.npcs {
        if npc.location == app.world.player_location {
            println!("{} is here.", npc.name);
        }
    }

    let exits = format_exits(app.world.player_location, &app.world.graph);
    println!("{}", exits);
    println!();
}

/// Prints current location description and exits (headless /look).
fn print_location_description(app: &App) {
    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let npc_names: Vec<&str> = app
            .npcs
            .iter()
            .filter(|n| n.location == app.world.player_location)
            .map(|n| n.name.as_str())
            .collect();
        let desc = render_description(loc_data, tod, &app.world.weather, &npc_names);
        println!("{}", desc);
    } else {
        println!("{}", app.world.current_location().description);
    }

    let exits = format_exits(app.world.player_location, &app.world.graph);
    println!("{}", exits);
}

/// Handles movement in headless mode.
fn handle_headless_movement(app: &mut App, target: &str) {
    let result = movement::resolve_movement(target, &app.world.graph, app.world.player_location);

    match result {
        MovementResult::Arrived {
            destination,
            minutes,
            narration,
            ..
        } => {
            println!("{}", narration);
            println!();

            app.world.clock.advance(minutes as i64);
            app.world.player_location = destination;

            if let Some(data) = app.world.graph.get(destination) {
                app.world
                    .locations
                    .entry(destination)
                    .or_insert_with(|| crate::world::Location {
                        id: destination,
                        name: data.name.clone(),
                        description: data.description_template.clone(),
                        indoor: data.indoor,
                        public: data.public,
                    });
            }

            print_location_arrival(app);
        }
        MovementResult::AlreadyHere => {
            println!("Sure, you're already standing right here.");
        }
        MovementResult::NotFound(name) => {
            println!(
                "You haven't the faintest notion how to reach \"{}\". Try asking about.",
                name
            );
            let exits = format_exits(app.world.player_location, &app.world.graph);
            println!("{}", exits);
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
}
