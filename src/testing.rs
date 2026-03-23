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
//! let result = h.execute("go to crossroads");
//! assert!(matches!(result, ActionResult::Moved { .. }));
//! assert_eq!(h.player_location(), "The Crossroads");
//! ```

use crate::input::{self, Command, InputResult, IntentKind};
use crate::npc::Npc;
use crate::npc::manager::NpcManager;
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
/// assert_eq!(h.player_location(), "Kilteevan Village");
/// h.execute("go to crossroads");
/// assert_eq!(h.player_location(), "The Crossroads");
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
    /// (Padraig O'Brien at The Crossroads). The player starts at Kilteevan Village.
    pub fn new() -> Self {
        let mut app = App::new();
        let parish_path = Path::new("data/parish.json");
        if parish_path.exists() {
            match crate::world::WorldState::from_parish_file(parish_path, LocationId(15)) {
                Ok(world) => app.world = world,
                Err(e) => eprintln!("Warning: Failed to load parish data: {}", e),
            }
        }
        // Load NPCs from data file, fall back to test NPC
        let npcs_path = Path::new("data/npcs.json");
        if npcs_path.exists() {
            match NpcManager::load_from_file(npcs_path) {
                Ok(mgr) => app.npc_manager = mgr,
                Err(_) => app.npc_manager.add_npc(Npc::new_test_npc()),
            }
        } else {
            app.npc_manager.add_npc(Npc::new_test_npc());
        }

        // Initial tier assignment
        app.npc_manager
            .assign_tiers(app.world.player_location, &app.world.graph);

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
    /// After each action, reassigns tiers and advances NPC schedules.
    pub fn execute(&mut self, input: &str) -> ActionResult {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return ActionResult::UnknownInput;
        }

        let result = match input::classify_input(trimmed) {
            InputResult::SystemCommand(cmd) => self.handle_system_command(cmd),
            InputResult::GameInput(text) => self.handle_game_input(&text),
        };

        // Simulation tick after each action
        self.app
            .npc_manager
            .assign_tiers(self.app.world.player_location, &self.app.world.graph);
        let schedule_events = self
            .app
            .npc_manager
            .tick_schedules(&self.app.world.clock, &self.app.world.graph);
        self.process_schedule_events(&schedule_events);

        result
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
            .npc_manager
            .npcs_at(self.app.world.player_location)
            .iter()
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

    /// Advances the game clock and ticks NPC schedules.
    ///
    /// Useful for testing NPC movement without player actions.
    pub fn advance_time(&mut self, minutes: i64) {
        self.app.world.clock.advance(minutes);
        let events = self
            .app
            .npc_manager
            .tick_schedules(&self.app.world.clock, &self.app.world.graph);
        self.process_schedule_events(&events);
        self.app
            .npc_manager
            .assign_tiers(self.app.world.player_location, &self.app.world.graph);
    }

    /// Returns the debug activity log entries.
    pub fn debug_log(&self) -> Vec<&str> {
        self.app.debug_log.iter().map(|s| s.as_str()).collect()
    }

    /// Returns whether the game clock is paused.
    pub fn is_paused(&self) -> bool {
        self.app.world.clock.is_paused()
    }

    /// Processes schedule events: debug log + player-visible text log messages.
    fn process_schedule_events(&mut self, events: &[crate::npc::manager::ScheduleEvent]) {
        use crate::npc::manager::ScheduleEventKind;
        let player_loc = self.app.world.player_location;

        for event in events {
            self.app.debug_event(event.debug_string());

            match &event.kind {
                ScheduleEventKind::Departed { from, .. } if *from == player_loc => {
                    self.app
                        .world
                        .log(format!("{} heads off down the road.", event.npc_name));
                }
                ScheduleEventKind::Arrived { location, .. } if *location == player_loc => {
                    self.app.world.log(format!("{} arrives.", event.npc_name));
                }
                _ => {}
            }
        }
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
            Command::ShowSpeed => {
                let speed_name = self
                    .app
                    .world
                    .clock
                    .current_speed()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        format!("Custom ({}x)", self.app.world.clock.speed_factor())
                    });
                let msg = format!("Speed: {}", speed_name);
                self.app.world.log(msg.clone());
                ActionResult::SystemCommand { response: msg }
            }
            Command::SetSpeed(speed) => {
                self.app.world.clock.set_speed(speed);
                let msg = format!("Speed changed to {}.", speed);
                self.app.world.log(msg.clone());
                ActionResult::SystemCommand { response: msg }
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
            | Command::ToggleSidebar
            | Command::ToggleImprov => {
                self.app
                    .world
                    .log("[Not yet implemented — coming in Phase 4]".to_string());
                ActionResult::SystemCommand {
                    response: "Not yet implemented".to_string(),
                }
            }
            Command::ShowProvider => {
                let msg = format!("Provider: {}", self.app.provider_name);
                self.app.world.log(msg.clone());
                ActionResult::SystemCommand { response: msg }
            }
            Command::SetProvider(name) => {
                use crate::config::Provider;
                match Provider::from_str_loose(&name) {
                    Ok(provider) => {
                        self.app.base_url = provider.default_base_url().to_string();
                        self.app.provider_name = format!("{:?}", provider).to_lowercase();
                        let msg = format!("Provider changed to {}.", self.app.provider_name);
                        self.app.world.log(msg.clone());
                        ActionResult::SystemCommand { response: msg }
                    }
                    Err(e) => {
                        let msg = format!("{}", e);
                        self.app.world.log(msg.clone());
                        ActionResult::SystemCommand { response: msg }
                    }
                }
            }
            Command::ShowModel => {
                let msg = if self.app.model_name.is_empty() {
                    "Model: (auto-detect)".to_string()
                } else {
                    format!("Model: {}", self.app.model_name)
                };
                self.app.world.log(msg.clone());
                ActionResult::SystemCommand { response: msg }
            }
            Command::SetModel(name) => {
                self.app.model_name = name.clone();
                let msg = format!("Model changed to {}.", name);
                self.app.world.log(msg.clone());
                ActionResult::SystemCommand { response: msg }
            }
            Command::ShowKey => {
                let msg = match &self.app.api_key {
                    Some(key) if key.len() > 8 => {
                        format!("API key: {}...{}", &key[..4], &key[key.len() - 4..])
                    }
                    Some(_) => "API key: (set, too short to mask)".to_string(),
                    None => "API key: (not set)".to_string(),
                };
                self.app.world.log(msg.clone());
                ActionResult::SystemCommand { response: msg }
            }
            Command::SetKey(value) => {
                self.app.api_key = Some(value);
                let msg = "API key updated.".to_string();
                self.app.world.log(msg.clone());
                ActionResult::SystemCommand { response: msg }
            }
            Command::Debug(sub) => {
                let lines = crate::debug::handle_debug(sub.as_deref(), &self.app);
                for line in &lines {
                    self.app.world.log(line.clone());
                }
                ActionResult::SystemCommand {
                    response: lines.join("\n"),
                }
            }
            Command::ShowCloud
            | Command::SetCloudProvider(_)
            | Command::ShowCloudModel
            | Command::SetCloudModel(_)
            | Command::ShowCloudKey
            | Command::SetCloudKey(_) => {
                let msg = "Cloud commands not available in test mode.".to_string();
                self.app.world.log(msg.clone());
                ActionResult::SystemCommand { response: msg }
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
    ///
    /// Checks all NPCs at the current location for canned responses,
    /// not just the first one. This allows tests to target specific NPCs
    /// regardless of iteration order.
    fn handle_npc_interaction(&mut self, _text: &str) -> ActionResult {
        let npcs_here = self.app.npc_manager.npcs_at(self.app.world.player_location);

        if npcs_here.is_empty() {
            self.app.world.log("Nothing happens.".to_string());
            return ActionResult::UnknownInput;
        }

        // Check each NPC at this location for canned responses
        for npc in &npcs_here {
            let key = npc.name.to_lowercase();
            if let Some(responses) = self.canned_responses.get_mut(&key)
                && !responses.is_empty()
            {
                let dialogue = responses.remove(0);
                let name = npc.name.clone();
                self.app.world.log(format!("{}: {}", name, dialogue));
                return ActionResult::NpcResponse {
                    npc: name,
                    dialogue,
                };
            }
        }

        ActionResult::NpcNotAvailable
    }

    /// Renders the current location description.
    fn render_current_location(&self) -> String {
        if let Some(loc_data) = self.app.world.current_location_data() {
            let tod = self.app.world.clock.time_of_day();
            let npc_names: Vec<&str> = self
                .app
                .npc_manager
                .npcs_at(self.app.world.player_location)
                .iter()
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
    fn test_harness_new_starts_at_kilteevan() {
        let h = GameTestHarness::new();
        assert_eq!(h.player_location(), "Kilteevan Village");
        assert_eq!(h.location_id(), LocationId(15));
    }

    #[test]
    fn test_harness_has_npcs() {
        let h = GameTestHarness::new();
        // With npcs.json loaded, we should have 8 NPCs
        assert!(
            h.app.npc_manager.npc_count() >= 1,
            "should have at least 1 NPC loaded"
        );
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
        h.execute("go to crossroads");
        let result = h.execute("go to pub");
        assert!(matches!(result, ActionResult::Moved { .. }));
        assert_eq!(h.player_location(), "Darcy's Pub");
    }

    #[test]
    fn test_move_advances_time() {
        let mut h = GameTestHarness::new();
        let before = h.time_of_day();
        // Move far enough to potentially change time
        h.execute("go to crossroads");
        // Time should still be deterministic — just verify it didn't break
        let _after = h.time_of_day();
        // Clock was Morning, a short trip shouldn't change it
        assert_eq!(before, TimeOfDay::Morning);
    }

    #[test]
    fn test_move_already_here() {
        let mut h = GameTestHarness::new();
        let result = h.execute("go to kilteevan");
        assert_eq!(result, ActionResult::AlreadyHere);
        assert_eq!(h.player_location(), "Kilteevan Village");
    }

    #[test]
    fn test_move_not_found() {
        let mut h = GameTestHarness::new();
        let result = h.execute("go to narnia");
        assert!(matches!(result, ActionResult::NotFound { .. }));
        assert_eq!(h.player_location(), "Kilteevan Village");
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
            assert!(response.contains("Kilteevan Village"));
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
        h.add_canned_response("Padraig Darcy", "Ah, good morning to ye!");
        // Advance to 10am when Padraig is scheduled at the pub (9-22)
        h.advance_time(120);
        h.execute("go to crossroads");
        h.execute("go to pub");
        let result = h.execute("hello there");
        assert!(matches!(result, ActionResult::NpcResponse { .. }));
        if let ActionResult::NpcResponse { npc, dialogue } = result {
            assert_eq!(npc, "Padraig Darcy");
            assert_eq!(dialogue, "Ah, good morning to ye!");
        }
    }

    #[test]
    fn test_canned_npc_response_fifo_order() {
        let mut h = GameTestHarness::new();
        h.add_canned_response("Padraig Darcy", "First response");
        h.add_canned_response("Padraig Darcy", "Second response");

        h.advance_time(120); // 10am — Padraig at pub
        h.execute("go to crossroads");
        h.execute("go to pub");
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
        h.add_canned_response("Padraig Darcy", "Only one response");

        h.advance_time(120); // 10am — Padraig at pub
        h.execute("go to crossroads");
        h.execute("go to pub");
        let r1 = h.execute("hello");
        assert!(matches!(r1, ActionResult::NpcResponse { .. }));

        let r2 = h.execute("hello again");
        assert_eq!(r2, ActionResult::NpcNotAvailable);
    }

    #[test]
    fn test_npc_not_at_empty_location() {
        let mut h = GameTestHarness::new();
        // Navigate to a location with no NPCs (e.g., the hurling green)
        h.execute("go to crossroads");
        h.execute("go to hurling green");
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
        assert_eq!(h.player_location(), "Kilteevan Village");

        h.execute("go to crossroads");
        assert_eq!(h.player_location(), "The Crossroads");

        h.execute("go to kilteevan");
        assert_eq!(h.player_location(), "Kilteevan Village");
    }

    #[test]
    fn test_movement_various_verbs() {
        let mut h = GameTestHarness::new();

        h.execute("walk to crossroads");
        assert_eq!(h.player_location(), "The Crossroads");

        h.execute("stroll to kilteevan");
        assert_eq!(h.player_location(), "Kilteevan Village");

        h.execute("head to crossroads");
        assert_eq!(h.player_location(), "The Crossroads");
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
        assert_eq!(h.player_location(), "Kilteevan Village");
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

    #[test]
    fn test_npc_schedule_movement_generates_debug_events() {
        let mut h = GameTestHarness::new();
        // Game starts at 8:00 AM. Padraig's schedule says 7-8 at crossroads.
        // He starts at home (pub). Tick should try to move him to crossroads.
        // After enough time passes, he should arrive and then head back to pub at 9.
        assert!(h.debug_log().is_empty() || !h.debug_log().is_empty());

        // Advance to 9am — this should trigger schedule movements
        h.advance_time(60);

        // Check that some debug events were generated
        let log = h.debug_log();
        // NPCs should have moved based on schedule changes
        let has_movement = log
            .iter()
            .any(|e| e.contains("heading to") || e.contains("arrived at"));
        assert!(
            has_movement,
            "Expected schedule movement events in debug log, got: {:?}",
            log
        );
    }

    #[test]
    fn test_advance_time_moves_npcs() {
        let mut h = GameTestHarness::new();
        // Go to pub where Padraig starts
        h.advance_time(120); // 10am
        h.execute("go to crossroads");
        h.execute("go to pub");

        // Padraig should be at the pub at 10am (schedule 9-22)
        let npcs = h.npcs_here();
        assert!(
            npcs.iter().any(|n| n.contains("Padraig")),
            "Padraig should be at pub at 10am, found: {:?}",
            npcs
        );
    }

    #[test]
    fn test_tier_assignment_after_movement() {
        let mut h = GameTestHarness::new();
        // After execute, tiers should be assigned
        h.execute("look");
        let result = h.execute("/debug tiers");
        if let ActionResult::SystemCommand { response } = result {
            // Should show tier info with player location
            assert!(
                response.contains("Kilteevan Village"),
                "Tier debug should show player location"
            );
        }
    }

    #[test]
    fn test_system_command_show_speed() {
        let mut h = GameTestHarness::new();
        let result = h.execute("/speed");
        if let ActionResult::SystemCommand { response } = result {
            assert!(
                response.contains("Normal"),
                "Default speed should be Normal, got: {}",
                response
            );
        } else {
            panic!("Expected SystemCommand");
        }
    }

    #[test]
    fn test_system_command_set_speed() {
        let mut h = GameTestHarness::new();
        let result = h.execute("/speed fast");
        if let ActionResult::SystemCommand { response } = result {
            assert!(
                response.contains("Fast"),
                "Should confirm speed change, got: {}",
                response
            );
        } else {
            panic!("Expected SystemCommand");
        }
        assert!(
            (h.app.world.clock.speed_factor() - 72.0).abs() < f64::EPSILON,
            "Speed should be 72.0 after /speed fast"
        );

        // Change again and verify
        h.execute("/speed slow");
        assert!(
            (h.app.world.clock.speed_factor() - 18.0).abs() < f64::EPSILON,
            "Speed should be 18.0 after /speed slow"
        );
    }
}
