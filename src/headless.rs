//! Headless CLI mode for testing without the TUI.
//!
//! Provides a simple stdin/stdout REPL that reuses the same game logic
//! (NPC inference, intent parsing, system commands) as the TUI mode.
//! Activated with `--headless` on the command line.

use crate::config::Provider;
use crate::inference::openai_client::OpenAiClient;
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
        match crate::world::WorldState::from_parish_file(parish_path, crate::world::LocationId(15))
        {
            Ok(world) => app.world = world,
            Err(e) => eprintln!("Warning: Failed to load parish data: {}", e),
        }
    }
    app.inference_queue = Some(queue);
    app.client = Some(client.clone());
    app.model_name = model_name.clone();
    app.base_url = client.base_url().to_string();
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
                let (quit, rebuild) = handle_headless_command(&mut app, cmd);
                if rebuild && let Some(new_client) = app.client.clone() {
                    let (tx, rx) = mpsc::channel(32);
                    let _new_worker = inference::spawn_inference_worker(new_client, rx);
                    app.inference_queue = Some(InferenceQueue::new(tx));
                }
                if quit {
                    break;
                }
            }
            InputResult::GameInput(text) => {
                let c = app.client.clone().unwrap();
                let m = app.model_name.clone();
                handle_headless_game_input(&mut app, &c, &m, &text, &mut request_id).await?;
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

/// Handles a system command in headless mode.
///
/// Returns `(should_quit, rebuild_inference)`.
fn handle_headless_command(app: &mut App, cmd: Command) -> (bool, bool) {
    let mut rebuild = false;
    match cmd {
        Command::Quit => {
            app.should_quit = true;
            return (true, false);
        }
        Command::Pause => {
            app.world.clock.pause();
            println!("The clocks of the parish stand still.");
        }
        Command::Resume => {
            app.world.clock.resume();
            println!("Time stirs again in the parish.");
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
        }
        Command::Help => {
            println!("A few things ye might say:");
            println!("  /quit     - Take your leave");
            println!("  /pause    - Hold time still");
            println!("  /resume   - Let time flow again");
            println!("  /status   - Where am I?");
            println!("  /irish    - Toggle Irish words sidebar (TUI only)");
            println!("  /improv   - Toggle improv craft for NPC dialogue");
            println!("  /provider - Show or change LLM provider");
            println!("  /model    - Show or change model name");
            println!("  /key      - Show or change API key");
            println!("  /help     - Show this help");
            println!("  /save     - Save game (not yet arrived)");
            println!("  /fork <n> - Fork save (not yet arrived)");
            println!("  /load <n> - Load save (not yet arrived)");
        }
        Command::ToggleSidebar => {
            println!("The pronunciation sidebar is only available in TUI mode.");
        }
        Command::ToggleImprov => {
            app.improv_enabled = !app.improv_enabled;
            if app.improv_enabled {
                println!("The characters loosen up — improv craft engaged.");
            } else {
                println!("The characters settle back to their usual selves.");
            }
        }
        Command::ShowProvider => {
            println!("Provider: {}", app.provider_name);
        }
        Command::SetProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                app.base_url = provider.default_base_url().to_string();
                app.provider_name = format!("{:?}", provider).to_lowercase();
                app.client = Some(OpenAiClient::new(&app.base_url, app.api_key.as_deref()));
                rebuild = true;
                println!("Provider changed to {}.", app.provider_name);
            }
            Err(e) => {
                println!("{}", e);
            }
        },
        Command::ShowModel => {
            if app.model_name.is_empty() {
                println!("Model: (auto-detect)");
            } else {
                println!("Model: {}", app.model_name);
            }
        }
        Command::SetModel(name) => {
            app.model_name = name.clone();
            println!("Model changed to {}.", name);
        }
        Command::ShowKey => match &app.api_key {
            Some(key) if key.len() > 8 => {
                let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                println!("API key: {}", masked);
            }
            Some(_) => {
                println!("API key: (set, too short to mask)");
            }
            None => {
                println!("API key: (not set)");
            }
        },
        Command::SetKey(value) => {
            app.api_key = Some(value);
            app.client = Some(OpenAiClient::new(&app.base_url, app.api_key.as_deref()));
            rebuild = true;
            println!("API key updated.");
        }
        Command::Save | Command::Fork(_) | Command::Load(_) | Command::Branches | Command::Log => {
            println!("That particular skill hasn't arrived in the parish yet. Patience now.");
        }
    }
    (false, rebuild)
}

/// Handles game input (NPC interaction or intent parsing) in headless mode.
async fn handle_headless_game_input(
    app: &mut App,
    client: &OpenAiClient,
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
                let system_prompt = npc::build_tier1_system_prompt(&npc, app.improv_enabled);
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
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Quit);
        assert!(quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_headless_command_pause() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Pause);
        assert!(!quit);
        assert!(app.world.clock.is_paused());
    }

    #[test]
    fn test_handle_headless_command_resume() {
        let mut app = App::new();
        app.world.clock.pause();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Resume);
        assert!(!quit);
        assert!(!app.world.clock.is_paused());
    }

    #[test]
    fn test_handle_headless_command_help() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Help);
        assert!(!quit);
    }

    #[test]
    fn test_handle_headless_command_status() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Status);
        assert!(!quit);
    }

    #[test]
    fn test_handle_headless_command_unimplemented() {
        let mut app = App::new();
        assert_eq!(
            handle_headless_command(&mut app, Command::Save),
            (false, false)
        );
        assert_eq!(
            handle_headless_command(&mut app, Command::Fork("test".to_string())),
            (false, false)
        );
        assert_eq!(
            handle_headless_command(&mut app, Command::Load("test".to_string())),
            (false, false)
        );
        assert_eq!(
            handle_headless_command(&mut app, Command::Branches),
            (false, false)
        );
        assert_eq!(
            handle_headless_command(&mut app, Command::Log),
            (false, false)
        );
    }

    #[test]
    fn test_handle_headless_command_show_provider() {
        let mut app = App::new();
        app.provider_name = "openrouter".to_string();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowProvider);
        assert!(!quit);
        assert!(!rebuild);
    }

    #[test]
    fn test_handle_headless_command_set_provider() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetProvider("openrouter".to_string()));
        assert!(!quit);
        assert!(rebuild);
        assert_eq!(app.provider_name, "openrouter");
        assert!(app.client.is_some());
    }

    #[test]
    fn test_handle_headless_command_set_provider_invalid() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetProvider("bogus".to_string()));
        assert!(!quit);
        assert!(!rebuild);
    }

    #[test]
    fn test_handle_headless_command_show_model() {
        let mut app = App::new();
        app.model_name = "test-model".to_string();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowModel);
        assert!(!quit);
        assert!(!rebuild);
    }

    #[test]
    fn test_handle_headless_command_set_model() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetModel("new-model".to_string()));
        assert!(!quit);
        assert!(!rebuild);
        assert_eq!(app.model_name, "new-model");
    }

    #[test]
    fn test_handle_headless_command_show_key_none() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowKey);
        assert!(!quit);
        assert!(!rebuild);
    }

    #[test]
    fn test_handle_headless_command_show_key_masked() {
        let mut app = App::new();
        app.api_key = Some("sk-or-v1-abcdef1234".to_string());
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowKey);
        assert!(!quit);
        assert!(!rebuild);
    }

    #[test]
    fn test_handle_headless_command_set_key() {
        let mut app = App::new();
        app.base_url = "https://openrouter.ai/api".to_string();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetKey("sk-new-key-12345678".to_string()));
        assert!(!quit);
        assert!(rebuild);
        assert_eq!(app.api_key, Some("sk-new-key-12345678".to_string()));
        assert!(app.client.is_some());
    }
}
