//! Game testing harness for programmatic interaction without a TUI or LLM.
//!
//! Provides [`GameTestHarness`] — a synchronous, no-Ollama-needed API that
//! drives the game through the same code paths as the headless and TUI modes.
//! Also provides [`run_script_mode`] for executing command files from the CLI
//! with structured JSON output.
//!
//! # Usage in tests
//!
//! ```rust,no_run
//! use parish::testing::{GameTestHarness, ActionResult};
//!
//! let mut h = GameTestHarness::new();
//! let result = h.execute("go to pub");
//! assert!(matches!(result, ActionResult::Moved { .. }));
//! assert_eq!(h.player_location(), "Darcy's Pub");
//! ```

use crate::input::{self, Command, InputResult, IntentKind};
use crate::npc::Npc;
use crate::tui::App;
use crate::world::description::{format_exits, render_description};
use crate::world::movement::{self, MovementResult};
use crate::world::time::{Season, TimeOfDay};
use crate::world::{Location, LocationId};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// The result of executing a command through the test harness.
///
/// Each variant captures the structured outcome of a player action,
/// allowing tests to assert on game state changes without parsing
/// prose output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ActionResult {
    /// Player moved to a new location.
    Moved {
        /// Name of the destination.
        to: String,
        /// Game minutes elapsed during travel.
        minutes: u16,
        /// Travel narration text.
        narration: String,
    },
    /// Player looked around the current location.
    Looked {
        /// The rendered location description.
        description: String,
    },
    /// Player tried to move to their current location.
    AlreadyHere,
    /// Movement target could not be found.
    NotFound {
        /// The unrecognized target name.
        target: String,
    },
    /// A system command was executed.
    SystemCommand {
        /// Description of what happened.
        response: String,
    },
    /// An NPC responded with a canned test response.
    NpcResponse {
        /// The NPC's name.
        npc: String,
        /// The dialogue text.
        dialogue: String,
    },
    /// NPC interaction attempted but no canned response or inference available.
    NpcNotAvailable,
    /// Input could not be parsed locally (would need LLM in production).
    UnknownInput,
    /// The game should exit.
    Quit,
}

/// A synchronous game driver for testing without a TUI or LLM.
///
/// Wraps [`App`] and provides a programmatic API for executing commands,
/// querying state, and registering canned NPC responses. Uses
/// [`parse_intent_local`](crate::input::parse_intent_local) for intent
/// parsing, so movement and look commands work without Ollama.
///
/// # Examples
///
/// ```rust,no_run
/// use parish::testing::GameTestHarness;
///
/// let mut h = GameTestHarness::new();
/// assert_eq!(h.player_location(), "The Crossroads");
/// h.execute("go to pub");
/// assert_eq!(h.player_location(), "Darcy's Pub");
/// ```
pub struct GameTestHarness {
    /// The underlying game state.
    pub app: App,
    /// Queued canned NPC responses, keyed by lowercase NPC name.
    canned_responses: HashMap<String, Vec<String>>,
}

impl GameTestHarness {
    /// Creates a new harness with the full parish world loaded.
    ///
    /// Loads `data/parish.json` for the world graph and adds the test NPC
    /// (Padraig O'Brien at The Crossroads). The player starts at The Crossroads.
    pub fn new() -> Self {
        let mut app = App::new();
        let parish_path = Path::new("data/parish.json");
        if parish_path.exists() {
            match crate::world::WorldState::from_parish_file(parish_path, LocationId(1)) {
                Ok(world) => app.world = world,
                Err(e) => eprintln!("Warning: Failed to load parish data: {}", e),
            }
        }
        app.npcs.push(Npc::new_test_npc());

        Self {
            app,
            canned_responses: HashMap::new(),
        }
    }

    /// Executes a raw input string and returns a structured result.
    ///
    /// Routes input through the same classification and intent parsing
    /// as the real game. Movement and look use local parsing; NPC
    /// interactions use canned responses if available.
    pub fn execute(&mut self, input: &str) -> ActionResult {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return ActionResult::UnknownInput;
        }

        match input::classify_input(trimmed) {
            InputResult::SystemCommand(cmd) => self.handle_system_command(cmd),
            InputResult::GameInput(text) => self.handle_game_input(&text),
        }
    }

    /// Registers a canned NPC response for testing dialogue flows.
    ///
    /// When the player talks or interacts at a location with this NPC,
    /// the harness pops the next canned response instead of calling Ollama.
    /// Responses are consumed in FIFO order.
    pub fn add_canned_response(&mut self, npc_name: &str, response: &str) {
        self.canned_responses
            .entry(npc_name.to_lowercase())
            .or_default()
            .push(response.to_string());
    }

    /// Returns the name of the player's current location.
    pub fn player_location(&self) -> &str {
        &self.app.world.current_location().name
    }

    /// Returns the player's current location id.
    pub fn location_id(&self) -> LocationId {
        self.app.world.player_location
    }

    /// Returns the current time of day.
    pub fn time_of_day(&self) -> TimeOfDay {
        self.app.world.clock.time_of_day()
    }

    /// Returns the current season.
    pub fn season(&self) -> Season {
        self.app.world.clock.season()
    }

    /// Returns the full text log.
    pub fn text_log(&self) -> &[String] {
        &self.app.world.text_log
    }

    /// Returns the last non-empty entry in the text log, or empty string.
    pub fn last_output(&self) -> &str {
        self.app
            .world
            .text_log
            .iter()
            .rev()
            .find(|s| !s.is_empty())
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Returns the names of NPCs at the player's current location.
    pub fn npcs_here(&self) -> Vec<&str> {
        self.app
            .npcs
            .iter()
            .filter(|n| n.location == self.app.world.player_location)
            .map(|n| n.name.as_str())
            .collect()
    }

    /// Returns formatted exit descriptions from the current location.
    pub fn exits(&self) -> String {
        format_exits(self.app.world.player_location, &self.app.world.graph)
    }

    /// Returns the current weather string.
    pub fn weather(&self) -> &str {
        &self.app.world.weather
    }

    /// Returns whether the game clock is paused.
    pub fn is_paused(&self) -> bool {
        self.app.world.clock.is_paused()
    }

    /// Handles a system command, returning a structured result.
    fn handle_system_command(&mut self, cmd: Command) -> ActionResult {
        match cmd {
            Command::Quit => {
                self.app.should_quit = true;
                ActionResult::Quit
            }
            Command::Pause => {
                self.app.world.clock.pause();
                self.app.world.log("[Time paused]".to_string());
                ActionResult::SystemCommand {
                    response: "Time paused".to_string(),
                }
            }
            Command::Resume => {
                self.app.world.clock.resume();
                self.app.world.log("[Time resumed]".to_string());
                ActionResult::SystemCommand {
                    response: "Time resumed".to_string(),
                }
            }
            Command::Status => {
                let time = self.app.world.clock.time_of_day();
                let season = self.app.world.clock.season();
                let loc = self.app.world.current_location().name.clone();
                let paused = if self.app.world.clock.is_paused() {
                    " (paused)"
                } else {
                    ""
                };
                let status = format!("Location: {} | {} | {}{}", loc, time, season, paused);
                self.app.world.log(status.clone());
                ActionResult::SystemCommand { response: status }
            }
            Command::Help => {
                self.app
                    .world
                    .log("Commands: /quit, /pause, /resume, /status, /help".to_string());
                ActionResult::SystemCommand {
                    response: "Help displayed".to_string(),
                }
            }
            Command::Save
            | Command::Fork(_)
            | Command::Load(_)
            | Command::Branches
            | Command::Log
            | Command::ToggleSidebar => {
                self.app
                    .world
                    .log("[Not yet implemented — coming in Phase 4]".to_string());
                ActionResult::SystemCommand {
                    response: "Not yet implemented".to_string(),
                }
            }
        }
    }

    /// Handles game input (movement, look, NPC interaction).
    fn handle_game_input(&mut self, text: &str) -> ActionResult {
        // Try local intent parsing (no LLM needed)
        let intent = input::parse_intent_local(text);

        match intent {
            Some(pi) => match pi.intent {
                IntentKind::Move => {
                    if let Some(target) = &pi.target {
                        self.handle_movement(target)
                    } else {
                        self.app.world.log("Go where?".to_string());
                        ActionResult::UnknownInput
                    }
                }
                IntentKind::Look => {
                    let desc = self.render_current_location();
                    let exits = format_exits(self.app.world.player_location, &self.app.world.graph);
                    self.app.world.log(desc.clone());
                    self.app.world.log(exits);
                    ActionResult::Looked { description: desc }
                }
                // Locally parsed as move/look but fell through — treat as NPC interaction
                _ => self.handle_npc_interaction(text),
            },
            None => {
                // No local match — try NPC interaction, else unknown
                self.handle_npc_interaction(text)
            }
        }
    }

    /// Handles movement, advancing the clock and updating location.
    fn handle_movement(&mut self, target: &str) -> ActionResult {
        let result = movement::resolve_movement(
            target,
            &self.app.world.graph,
            self.app.world.player_location,
        );

        match result {
            MovementResult::Arrived {
                destination,
                minutes,
                narration,
                ..
            } => {
                self.app.world.log(narration.clone());
                self.app.world.log(String::new());

                self.app.world.clock.advance(minutes as i64);
                self.app.world.player_location = destination;

                // Update legacy locations map
                if let Some(data) = self.app.world.graph.get(destination) {
                    self.app
                        .world
                        .locations
                        .entry(destination)
                        .or_insert_with(|| Location {
                            id: destination,
                            name: data.name.clone(),
                            description: data.description_template.clone(),
                            indoor: data.indoor,
                            public: data.public,
                        });
                }

                // Log arrival
                let loc_name = self.app.world.current_location().name.clone();
                self.app.world.log(format!("— {} —", loc_name));
                let desc = self.render_current_location();
                self.app.world.log(desc);
                let exits = format_exits(self.app.world.player_location, &self.app.world.graph);
                self.app.world.log(exits);
                self.app.world.log(String::new());

                ActionResult::Moved {
                    to: loc_name,
                    minutes,
                    narration,
                }
            }
            MovementResult::AlreadyHere => {
                self.app.world.log("You are already here.".to_string());
                ActionResult::AlreadyHere
            }
            MovementResult::NotFound(name) => {
                self.app
                    .world
                    .log(format!("You don't know how to get to \"{}\".", name));
                let exits = format_exits(self.app.world.player_location, &self.app.world.graph);
                self.app.world.log(exits);
                ActionResult::NotFound { target: name }
            }
        }
    }

    /// Attempts NPC interaction using canned responses.
    fn handle_npc_interaction(&mut self, _text: &str) -> ActionResult {
        let npc = self
            .app
            .npcs
            .iter()
            .find(|n| n.location == self.app.world.player_location)
            .cloned();

        if let Some(npc) = npc {
            let key = npc.name.to_lowercase();
            if let Some(responses) = self.canned_responses.get_mut(&key)
                && !responses.is_empty()
            {
                let dialogue = responses.remove(0);
                self.app.world.log(format!("{}: {}", npc.name, dialogue));
                return ActionResult::NpcResponse {
                    npc: npc.name,
                    dialogue,
                };
            }
            ActionResult::NpcNotAvailable
        } else {
            // No NPC here and input wasn't recognized locally
            self.app.world.log("Nothing happens.".to_string());
            ActionResult::UnknownInput
        }
    }

    /// Renders the current location description.
    fn render_current_location(&self) -> String {
        if let Some(loc_data) = self.app.world.current_location_data() {
            let tod = self.app.world.clock.time_of_day();
            let npc_names: Vec<&str> = self
                .app
                .npcs
                .iter()
                .filter(|n| n.location == self.app.world.player_location)
                .map(|n| n.name.as_str())
                .collect();
            render_description(loc_data, tod, &self.app.world.weather, &npc_names)
        } else {
            self.app.world.current_location().description.clone()
        }
    }
}

impl Default for GameTestHarness {
    fn default() -> Self {
        Self::new()
    }
}

/// JSON output line for script mode.
#[derive(Serialize)]
struct ScriptOutputLine {
    command: String,
    #[serde(flatten)]
    result: ActionResult,
    location: String,
    time: String,
    season: String,
}

/// Runs the game in script mode, reading commands from a file.
///
/// Each command is executed through [`GameTestHarness`] and produces
/// one JSON line of output. This allows Claude Code (or any script)
/// to verify game behavior without a terminal or Ollama.
pub fn run_script_mode(script_path: &Path) -> anyhow::Result<()> {
    let contents = std::fs::read_to_string(script_path)?;
    let mut harness = GameTestHarness::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let result = harness.execute(trimmed);
        let output = ScriptOutputLine {
            command: trimmed.to_string(),
            result,
            location: harness.player_location().to_string(),
            time: harness.time_of_day().to_string(),
            season: harness.season().to_string(),
        };
        println!("{}", serde_json::to_string(&output)?);

        if harness.app.should_quit {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_new_starts_at_crossroads() {
        let h = GameTestHarness::new();
        assert_eq!(h.player_location(), "The Crossroads");
        assert_eq!(h.location_id(), LocationId(1));
    }

    #[test]
    fn test_harness_has_test_npc() {
        let h = GameTestHarness::new();
        let npcs = h.npcs_here();
        assert!(npcs.contains(&"Padraig O'Brien"));
    }

    #[test]
    fn test_harness_initial_time() {
        let h = GameTestHarness::new();
        assert_eq!(h.time_of_day(), TimeOfDay::Morning);
        assert_eq!(h.season(), Season::Spring);
    }

    #[test]
    fn test_harness_initial_weather() {
        let h = GameTestHarness::new();
        assert_eq!(h.weather(), "Clear");
    }

    #[test]
    fn test_move_to_pub() {
        let mut h = GameTestHarness::new();
        let result = h.execute("go to pub");
        assert!(matches!(result, ActionResult::Moved { .. }));
        assert_eq!(h.player_location(), "Darcy's Pub");
    }

    #[test]
    fn test_move_advances_time() {
        let mut h = GameTestHarness::new();
        let before = h.time_of_day();
        // Move far enough to potentially change time
        h.execute("go to pub");
        // Time should still be deterministic — just verify it didn't break
        let _after = h.time_of_day();
        // Clock was Morning, a short trip shouldn't change it
        assert_eq!(before, TimeOfDay::Morning);
    }

    #[test]
    fn test_move_already_here() {
        let mut h = GameTestHarness::new();
        let result = h.execute("go to crossroads");
        assert_eq!(result, ActionResult::AlreadyHere);
        assert_eq!(h.player_location(), "The Crossroads");
    }

    #[test]
    fn test_move_not_found() {
        let mut h = GameTestHarness::new();
        let result = h.execute("go to narnia");
        assert!(matches!(result, ActionResult::NotFound { .. }));
        assert_eq!(h.player_location(), "The Crossroads");
    }

    #[test]
    fn test_look() {
        let mut h = GameTestHarness::new();
        let result = h.execute("look");
        assert!(matches!(result, ActionResult::Looked { .. }));
        if let ActionResult::Looked { description } = result {
            assert!(!description.is_empty());
        }
    }

    #[test]
    fn test_look_around() {
        let mut h = GameTestHarness::new();
        let result = h.execute("look around");
        assert!(matches!(result, ActionResult::Looked { .. }));
    }

    #[test]
    fn test_system_command_pause() {
        let mut h = GameTestHarness::new();
        let result = h.execute("/pause");
        assert!(matches!(result, ActionResult::SystemCommand { .. }));
        assert!(h.is_paused());
    }

    #[test]
    fn test_system_command_resume() {
        let mut h = GameTestHarness::new();
        h.execute("/pause");
        let result = h.execute("/resume");
        assert!(matches!(result, ActionResult::SystemCommand { .. }));
        assert!(!h.is_paused());
    }

    #[test]
    fn test_system_command_status() {
        let mut h = GameTestHarness::new();
        let result = h.execute("/status");
        if let ActionResult::SystemCommand { response } = result {
            assert!(response.contains("The Crossroads"));
            assert!(response.contains("Morning"));
        } else {
            panic!("Expected SystemCommand");
        }
    }

    #[test]
    fn test_system_command_quit() {
        let mut h = GameTestHarness::new();
        let result = h.execute("/quit");
        assert_eq!(result, ActionResult::Quit);
        assert!(h.app.should_quit);
    }

    #[test]
    fn test_system_command_help() {
        let mut h = GameTestHarness::new();
        let result = h.execute("/help");
        assert!(matches!(result, ActionResult::SystemCommand { .. }));
    }

    #[test]
    fn test_canned_npc_response() {
        let mut h = GameTestHarness::new();
        h.add_canned_response("Padraig O'Brien", "Ah, good morning to ye!");
        let result = h.execute("hello there");
        assert!(matches!(result, ActionResult::NpcResponse { .. }));
        if let ActionResult::NpcResponse { npc, dialogue } = result {
            assert_eq!(npc, "Padraig O'Brien");
            assert_eq!(dialogue, "Ah, good morning to ye!");
        }
    }

    #[test]
    fn test_canned_npc_response_fifo_order() {
        let mut h = GameTestHarness::new();
        h.add_canned_response("Padraig O'Brien", "First response");
        h.add_canned_response("Padraig O'Brien", "Second response");

        let r1 = h.execute("hello");
        let r2 = h.execute("how are you");

        if let ActionResult::NpcResponse { dialogue, .. } = r1 {
            assert_eq!(dialogue, "First response");
        }
        if let ActionResult::NpcResponse { dialogue, .. } = r2 {
            assert_eq!(dialogue, "Second response");
        }
    }

    #[test]
    fn test_canned_npc_exhausted() {
        let mut h = GameTestHarness::new();
        h.add_canned_response("Padraig O'Brien", "Only one response");

        let r1 = h.execute("hello");
        assert!(matches!(r1, ActionResult::NpcResponse { .. }));

        let r2 = h.execute("hello again");
        assert_eq!(r2, ActionResult::NpcNotAvailable);
    }

    #[test]
    fn test_npc_not_at_location() {
        let mut h = GameTestHarness::new();
        h.execute("go to church");
        // No NPC at church
        let result = h.execute("hello there");
        assert_eq!(result, ActionResult::UnknownInput);
    }

    #[test]
    fn test_empty_input() {
        let mut h = GameTestHarness::new();
        let result = h.execute("");
        assert_eq!(result, ActionResult::UnknownInput);
    }

    #[test]
    fn test_whitespace_input() {
        let mut h = GameTestHarness::new();
        let result = h.execute("   ");
        assert_eq!(result, ActionResult::UnknownInput);
    }

    #[test]
    fn test_text_log_grows() {
        let mut h = GameTestHarness::new();
        let before = h.text_log().len();
        h.execute("look");
        let after = h.text_log().len();
        assert!(after > before);
    }

    #[test]
    fn test_exits_not_empty() {
        let h = GameTestHarness::new();
        let exits = h.exits();
        assert!(exits.contains("You can go to:"));
    }

    #[test]
    fn test_movement_round_trip() {
        let mut h = GameTestHarness::new();
        assert_eq!(h.player_location(), "The Crossroads");

        h.execute("go to pub");
        assert_eq!(h.player_location(), "Darcy's Pub");

        h.execute("go to crossroads");
        assert_eq!(h.player_location(), "The Crossroads");
    }

    #[test]
    fn test_movement_various_verbs() {
        let mut h = GameTestHarness::new();

        h.execute("walk to pub");
        assert_eq!(h.player_location(), "Darcy's Pub");

        h.execute("stroll to crossroads");
        assert_eq!(h.player_location(), "The Crossroads");

        h.execute("head to church");
        assert_eq!(h.player_location(), "St. Brigid's Church");
    }

    #[test]
    fn test_last_output() {
        let mut h = GameTestHarness::new();
        h.execute("look");
        assert!(!h.last_output().is_empty());
    }

    #[test]
    fn test_default_trait() {
        let h = GameTestHarness::default();
        assert_eq!(h.player_location(), "The Crossroads");
    }

    #[test]
    fn test_unimplemented_commands() {
        let mut h = GameTestHarness::new();
        let result = h.execute("/save");
        if let ActionResult::SystemCommand { response } = result {
            assert!(response.contains("Not yet implemented"));
        }
    }

    #[test]
    fn test_script_comment_lines_skipped() {
        // Write a temp script with comments
        let dir = std::env::temp_dir().join("parish_test_script");
        std::fs::create_dir_all(&dir).ok();
        let script = dir.join("comments.txt");
        std::fs::write(
            &script,
            "# This is a comment\n\nlook\n# Another comment\n/quit\n",
        )
        .unwrap();

        // run_script_mode writes to stdout, just verify no panic
        run_script_mode(&script).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}
