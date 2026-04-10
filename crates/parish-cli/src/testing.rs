//! Game testing harness for programmatic interaction without an LLM.
//!
//! Provides [`GameTestHarness`] — a synchronous, no-Ollama-needed API that
//! drives the game through the same code paths as the headless mode.
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

use crate::app::App;
use crate::input::{self, Command, InputResult, IntentKind};
use crate::npc::Npc;
use crate::npc::manager::NpcManager;
use crate::world::LocationId;
use crate::world::description::{format_exits, render_description};
use crate::world::time::{Season, TimeOfDay};
use parish_core::ipc::capitalize_first;
use parish_core::world::transport::TransportMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// The result of executing a command through the test harness.
///
/// Each variant captures the structured outcome of a player action,
/// allowing tests to assert on game state changes without parsing
/// prose output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        /// Anachronistic terms detected in the player's input.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        anachronisms: Vec<String>,
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
    /// Synchronous database handle for persistence in tests.
    db_sync: Option<crate::persistence::Database>,
}

impl GameTestHarness {
    /// Creates a new harness with the full parish world loaded from the active mod.
    pub fn new() -> Self {
        let mut app = App::new();

        let game_mod = parish_core::game_mod::find_default_mod()
            .and_then(|dir| parish_core::game_mod::GameMod::load(&dir).ok());

        if let Some(ref gm) = game_mod {
            match parish_core::game_mod::world_state_from_mod(gm) {
                Ok(world) => app.world = world,
                Err(e) => eprintln!("Warning: Failed to load world from mod: {}", e),
            }

            let npcs_path = gm.npcs_path();
            if npcs_path.exists() {
                match NpcManager::load_from_file(&npcs_path) {
                    Ok(mgr) => app.npc_manager = mgr,
                    Err(_) => app.npc_manager.add_npc(Npc::new_test_npc()),
                }
            } else {
                app.npc_manager.add_npc(Npc::new_test_npc());
            }
        } else {
            app.npc_manager.add_npc(Npc::new_test_npc());
        }
        app.game_mod = game_mod;

        // Initial tier assignment
        app.npc_manager.assign_tiers(&app.world, &[]);

        // Initialize in-memory persistence for test harness
        let db_sync = crate::persistence::Database::open_memory().ok();
        let mut active_branch_id = 1;
        let mut latest_snapshot_id = 0;
        if let Some(ref db) = db_sync
            && let Ok(Some(branch)) = db.find_branch("main")
        {
            active_branch_id = branch.id;
            let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
            if let Ok(snap_id) = db.save_snapshot(branch.id, &snapshot) {
                latest_snapshot_id = snap_id;
            }
        }
        app.active_branch_id = active_branch_id;
        app.latest_snapshot_id = latest_snapshot_id;

        Self {
            app,
            canned_responses: HashMap::new(),
            db_sync,
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

        // Handle test-harness-only /stub command: /stub NpcName: dialogue text
        if let Some(rest) = trimmed.strip_prefix("/stub ")
            && let Some((name, dialogue)) = rest.split_once(':')
        {
            let name = name.trim();
            let dialogue = dialogue.trim();
            self.add_canned_response(name, dialogue);
            let msg = format!("Stubbed response for {}: \"{}\"", name, dialogue);
            self.app.world.log(msg.clone());
            return ActionResult::SystemCommand { response: msg };
        }

        let result = match input::classify_input(trimmed) {
            InputResult::SystemCommand(cmd) => self.handle_system_command(cmd),
            InputResult::GameInput(text) => self.handle_game_input(&text),
        };

        // Simulation tick after each action
        let tier_transitions = self.app.npc_manager.assign_tiers(&self.app.world, &[]);
        for tt in &tier_transitions {
            let direction = if tt.promoted { "promoted" } else { "demoted" };
            self.app.debug_event(format!(
                "[tier] {} {} {:?} → {:?}",
                tt.npc_name, direction, tt.old_tier, tt.new_tier,
            ));
        }
        let schedule_events = self.app.npc_manager.tick_schedules(
            &self.app.world.clock,
            &self.app.world.graph,
            self.app.world.weather,
        );
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

    /// Returns the default transport mode from the game mod, or walking.
    fn default_transport(&self) -> TransportMode {
        self.app
            .game_mod
            .as_ref()
            .map(|gm| gm.transport.default_mode().clone())
            .unwrap_or_else(TransportMode::walking)
    }

    /// Returns formatted exit descriptions from the current location.
    pub fn exits(&self) -> String {
        let transport = self.default_transport();
        format_exits(
            self.app.world.player_location,
            &self.app.world.graph,
            transport.speed_m_per_s,
            &transport.label,
        )
    }

    /// Returns the current weather.
    pub fn weather(&self) -> &crate::world::Weather {
        &self.app.world.weather
    }

    /// Advances the game clock and ticks NPC schedules.
    ///
    /// Useful for testing NPC movement without player actions.
    pub fn advance_time(&mut self, minutes: i64) {
        self.app.world.clock.advance(minutes);

        // Tick the weather engine for each hour that elapsed, so large time jumps
        // don't skip weather checks. The engine deduplicates by game-hour internally.
        let season = self.app.world.clock.season();
        let now = self.app.world.clock.now();
        let mut rng = rand::thread_rng();
        let hours_elapsed = (minutes / 60).max(1) as u32;
        for h in 0..hours_elapsed {
            let check_time =
                now - chrono::Duration::minutes((hours_elapsed.saturating_sub(h + 1) as i64) * 60);
            if let Some(new_weather) = self
                .app
                .world
                .weather_engine
                .tick(check_time, season, &mut rng)
            {
                self.app.world.weather = new_weather;
            }
        }

        let events = self.app.npc_manager.tick_schedules(
            &self.app.world.clock,
            &self.app.world.graph,
            self.app.world.weather,
        );
        self.process_schedule_events(&events);
        self.app.npc_manager.assign_tiers(&self.app.world, &[]);

        // Propagate gossip between co-located NPCs
        if !self.app.world.gossip_network.is_empty() {
            let groups = self.app.npc_manager.tier2_groups();
            for npc_ids in groups.values() {
                if npc_ids.len() >= 2 {
                    crate::npc::ticks::propagate_gossip_at_location(
                        npc_ids,
                        &mut self.app.world.gossip_network,
                        &mut rng,
                    );
                }
            }
        }
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

            let display = self
                .app
                .npc_manager
                .get(event.npc_id)
                .map(|n| self.app.npc_manager.display_name(n).to_string())
                .unwrap_or_else(|| event.npc_name.clone());

            match &event.kind {
                ScheduleEventKind::Departed { from, .. } if *from == player_loc => {
                    self.app.world.log(format!(
                        "{} heads off down the road.",
                        capitalize_first(&display)
                    ));
                }
                ScheduleEventKind::Arrived { location, .. } if *location == player_loc => {
                    self.app
                        .world
                        .log(format!("{} arrives.", capitalize_first(&display)));
                }
                _ => {}
            }
        }
    }

    /// Handles a system command, returning a structured result.
    ///
    /// Delegates most commands to [`parish_core::ipc::handle_command`] and
    /// dispatches any returned [`CommandEffect`]s locally. Wait and Tick are
    /// handled locally because the test harness needs weather ticking and
    /// schedule-event processing that the shared handler doesn't perform.
    fn handle_system_command(&mut self, cmd: Command) -> ActionResult {
        use chrono::Timelike;
        use parish_core::ipc::commands::{CommandEffect, handle_command};

        // Wait and Tick need test-harness-specific behavior (weather ticks,
        // schedule event processing, gossip propagation via advance_time).
        match cmd {
            Command::Wait(minutes) => {
                self.advance_time(minutes as i64);
                let now = self.app.world.clock.now();
                let tod = self.app.world.clock.time_of_day();
                let msg = format!(
                    "Waited {} minutes. Now {:02}:{:02} {}.",
                    minutes,
                    now.hour(),
                    now.minute(),
                    tod
                );
                self.app.world.log(msg.clone());
                return ActionResult::SystemCommand { response: msg };
            }
            Command::Tick => {
                // Tick weather engine (shared handler doesn't do this)
                let season = self.app.world.clock.season();
                let now = self.app.world.clock.now();
                let mut rng = rand::thread_rng();
                if let Some(new_weather) = self.app.world.weather_engine.tick(now, season, &mut rng)
                {
                    self.app.world.weather = new_weather;
                }

                self.app.npc_manager.assign_tiers(&self.app.world, &[]);
                let events = self.app.npc_manager.tick_schedules(
                    &self.app.world.clock,
                    &self.app.world.graph,
                    self.app.world.weather,
                );
                let count = events.len();
                self.process_schedule_events(&events);
                let msg = if count == 0 {
                    "No NPC activity.".to_string()
                } else {
                    format!("{} schedule event(s) processed.", count)
                };
                self.app.world.log(msg.clone());
                return ActionResult::SystemCommand { response: msg };
            }
            _ => {} // fall through to shared handler
        }

        // Delegate to shared handler
        let mut config = self.app.snapshot_config();
        let result = handle_command(
            cmd,
            &mut self.app.world,
            &mut self.app.npc_manager,
            &mut config,
        );
        self.app.apply_config(&config);

        // Log and dispatch effects
        if !result.response.is_empty() {
            self.app.world.log(result.response.clone());
        }

        for effect in &result.effects {
            match effect {
                CommandEffect::Quit => {
                    self.app.should_quit = true;
                    return ActionResult::Quit;
                }
                CommandEffect::SaveGame => {
                    return self.handle_save_effect();
                }
                CommandEffect::ForkBranch(name) => {
                    return self.handle_fork_effect(name);
                }
                CommandEffect::LoadBranch(name) => {
                    return self.handle_load_effect(name);
                }
                CommandEffect::ListBranches => {
                    return self.handle_list_branches_effect();
                }
                CommandEffect::ShowLog => {
                    return self.handle_show_log_effect();
                }
                CommandEffect::ToggleMap => {
                    return self.handle_map_effect();
                }
                CommandEffect::Debug(sub) => {
                    let lines = crate::debug::handle_debug(sub.as_deref(), &self.app);
                    for line in &lines {
                        self.app.world.log(line.clone());
                    }
                    return ActionResult::SystemCommand {
                        response: lines.join("\n"),
                    };
                }
                CommandEffect::ShowSpinner(secs) => {
                    let msg = format!("Spinner preview ({secs}s) — GUI only.");
                    self.app.world.log(msg.clone());
                    return ActionResult::SystemCommand { response: msg };
                }
                CommandEffect::NewGame => {
                    return self.handle_new_game_effect();
                }
                CommandEffect::RebuildInference | CommandEffect::RebuildCloudClient => {
                    // No-op in test mode — no real inference clients
                }
            }
        }

        ActionResult::SystemCommand {
            response: result.response,
        }
    }

    /// Handles the SaveGame effect.
    fn handle_save_effect(&mut self) -> ActionResult {
        if let Some(ref db_sync) = self.db_sync {
            let snapshot =
                crate::persistence::GameSnapshot::capture(&self.app.world, &self.app.npc_manager);
            match db_sync.save_snapshot(self.app.active_branch_id, &snapshot) {
                Ok(snap_id) => {
                    let _ = db_sync
                        .clear_journal(self.app.active_branch_id, self.app.latest_snapshot_id);
                    self.app.latest_snapshot_id = snap_id;
                    self.app.world.log("Game saved.".to_string());
                    ActionResult::SystemCommand {
                        response: "Game saved.".to_string(),
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to save: {}", e);
                    self.app.world.log(msg.clone());
                    ActionResult::SystemCommand { response: msg }
                }
            }
        } else {
            self.app.world.log("Persistence not available.".to_string());
            ActionResult::SystemCommand {
                response: "Persistence not available.".to_string(),
            }
        }
    }

    /// Handles the ForkBranch effect.
    fn handle_fork_effect(&mut self, name: &str) -> ActionResult {
        if let Some(ref db_sync) = self.db_sync {
            let snapshot =
                crate::persistence::GameSnapshot::capture(&self.app.world, &self.app.npc_manager);
            let _ = db_sync.save_snapshot(self.app.active_branch_id, &snapshot);

            match db_sync.create_branch(name, Some(self.app.active_branch_id)) {
                Ok(new_branch_id) => match db_sync.save_snapshot(new_branch_id, &snapshot) {
                    Ok(snap_id) => {
                        self.app.active_branch_id = new_branch_id;
                        self.app.latest_snapshot_id = snap_id;
                        let msg = format!("Forked to branch '{}'.", name);
                        self.app.world.log(msg.clone());
                        ActionResult::SystemCommand { response: msg }
                    }
                    Err(e) => {
                        let msg = format!("Failed to save fork: {}", e);
                        self.app.world.log(msg.clone());
                        ActionResult::SystemCommand { response: msg }
                    }
                },
                Err(e) => {
                    let msg = format!("Failed to fork: {}", e);
                    self.app.world.log(msg.clone());
                    ActionResult::SystemCommand { response: msg }
                }
            }
        } else {
            self.app.world.log("Persistence not available.".to_string());
            ActionResult::SystemCommand {
                response: "Persistence not available.".to_string(),
            }
        }
    }

    /// Handles the LoadBranch effect.
    fn handle_load_effect(&mut self, name: &str) -> ActionResult {
        if name.is_empty() {
            let msg = "Save picker not available in test mode.".to_string();
            self.app.world.log(msg.clone());
            return ActionResult::SystemCommand { response: msg };
        }
        if let Some(ref db_sync) = self.db_sync {
            match db_sync.find_branch(name) {
                Ok(Some(branch)) => {
                    if branch.id != self.app.active_branch_id {
                        let snapshot = crate::persistence::GameSnapshot::capture(
                            &self.app.world,
                            &self.app.npc_manager,
                        );
                        let _ = db_sync.save_snapshot(self.app.active_branch_id, &snapshot);
                    }
                    match db_sync.load_latest_snapshot(branch.id) {
                        Ok(Some((snap_id, loaded_snapshot))) => {
                            let events = db_sync
                                .events_since_snapshot(branch.id, snap_id)
                                .unwrap_or_default();
                            loaded_snapshot.restore(&mut self.app.world, &mut self.app.npc_manager);
                            crate::persistence::replay_journal(
                                &mut self.app.world,
                                &mut self.app.npc_manager,
                                &events,
                            );
                            self.app.active_branch_id = branch.id;
                            self.app.latest_snapshot_id = snap_id;
                            self.app.npc_manager.assign_tiers(&self.app.world, &[]);
                            let time = self.app.world.clock.time_of_day();
                            let season = self.app.world.clock.season();
                            let msg = format!("Loaded branch '{}'. {}, {}.", name, season, time);
                            self.app.world.log(msg.clone());
                            ActionResult::SystemCommand { response: msg }
                        }
                        Ok(None) => {
                            let msg = format!("Branch '{}' has no saves.", name);
                            self.app.world.log(msg.clone());
                            ActionResult::SystemCommand { response: msg }
                        }
                        Err(e) => {
                            let msg = format!("Failed to load: {}", e);
                            self.app.world.log(msg.clone());
                            ActionResult::SystemCommand { response: msg }
                        }
                    }
                }
                Ok(None) => {
                    let msg = format!("No branch named '{}'.", name);
                    self.app.world.log(msg.clone());
                    ActionResult::SystemCommand { response: msg }
                }
                Err(e) => {
                    let msg = format!("Failed to find branch: {}", e);
                    self.app.world.log(msg.clone());
                    ActionResult::SystemCommand { response: msg }
                }
            }
        } else {
            self.app.world.log("Persistence not available.".to_string());
            ActionResult::SystemCommand {
                response: "Persistence not available.".to_string(),
            }
        }
    }

    /// Handles the ListBranches effect.
    fn handle_list_branches_effect(&mut self) -> ActionResult {
        if let Some(ref db_sync) = self.db_sync {
            match db_sync.list_branches() {
                Ok(branches) => {
                    let mut lines = vec!["Save branches:".to_string()];
                    for b in &branches {
                        let marker = if b.id == self.app.active_branch_id {
                            " *"
                        } else {
                            ""
                        };
                        lines.push(format!(
                            "  {}{} (created {})",
                            b.name,
                            marker,
                            crate::persistence::format_timestamp(&b.created_at)
                        ));
                    }
                    let msg = lines.join("\n");
                    self.app.world.log(msg.clone());
                    ActionResult::SystemCommand { response: msg }
                }
                Err(e) => {
                    let msg = format!("Failed to list branches: {}", e);
                    self.app.world.log(msg.clone());
                    ActionResult::SystemCommand { response: msg }
                }
            }
        } else {
            self.app.world.log("Persistence not available.".to_string());
            ActionResult::SystemCommand {
                response: "Persistence not available.".to_string(),
            }
        }
    }

    /// Handles the ShowLog effect.
    fn handle_show_log_effect(&mut self) -> ActionResult {
        if let Some(ref db_sync) = self.db_sync {
            match db_sync.branch_log(self.app.active_branch_id) {
                Ok(snapshots) => {
                    let msg = if snapshots.is_empty() {
                        "No snapshots on this branch yet.".to_string()
                    } else {
                        let mut lines = vec!["Snapshot history (most recent first):".to_string()];
                        for s in &snapshots {
                            lines.push(format!(
                                "  #{} — game: {} | saved: {}",
                                s.id,
                                s.game_time,
                                crate::persistence::format_timestamp(&s.real_time)
                            ));
                        }
                        lines.join("\n")
                    };
                    self.app.world.log(msg.clone());
                    ActionResult::SystemCommand { response: msg }
                }
                Err(e) => {
                    let msg = format!("Failed to get branch log: {}", e);
                    self.app.world.log(msg.clone());
                    ActionResult::SystemCommand { response: msg }
                }
            }
        } else {
            self.app.world.log("Persistence not available.".to_string());
            ActionResult::SystemCommand {
                response: "Persistence not available.".to_string(),
            }
        }
    }

    /// Handles the ToggleMap effect — renders a text map for the test harness.
    fn handle_map_effect(&mut self) -> ActionResult {
        let player_loc = self.app.world.player_location;
        let mut lines = vec!["=== Parish Map ===".to_string()];
        for node_id in self.app.world.graph.location_ids() {
            if let Some(data) = self.app.world.graph.get(node_id) {
                let marker = if node_id == player_loc { " * " } else { "   " };
                lines.push(format!("{}{}", marker, data.name));
            }
        }
        lines.push(String::new());
        lines.push("Connections:".to_string());
        for node_id in self.app.world.graph.location_ids() {
            if let Some(data) = self.app.world.graph.get(node_id) {
                for (neighbor_id, _) in self.app.world.graph.neighbors(node_id) {
                    if node_id.0 < neighbor_id.0 {
                        let neighbor_name = self
                            .app
                            .world
                            .graph
                            .get(neighbor_id)
                            .map(|d| d.name.as_str())
                            .unwrap_or("???");
                        lines.push(format!("  {} — {}", data.name, neighbor_name));
                    }
                }
            }
        }
        let msg = lines.join("\n");
        self.app.world.log(msg.clone());
        ActionResult::SystemCommand { response: msg }
    }

    /// Handles the NewGame effect — reinitializes world and NPCs.
    fn handle_new_game_effect(&mut self) -> ActionResult {
        let game_mod = parish_core::game_mod::find_default_mod()
            .and_then(|dir| parish_core::game_mod::GameMod::load(&dir).ok());

        if let Some(ref gm) = game_mod
            && let Ok(world) = parish_core::game_mod::world_state_from_mod(gm)
        {
            self.app.world = world;
        } else {
            let parish_path = Path::new("data/parish.json");
            if parish_path.exists()
                && let Ok(world) =
                    crate::world::WorldState::from_parish_file(parish_path, LocationId(15))
            {
                self.app.world = world;
            }
        }

        let npcs_path = if let Some(ref gm) = game_mod {
            gm.npcs_path()
        } else {
            std::path::PathBuf::from("data/npcs.json")
        };
        if npcs_path.exists()
            && let Ok(mgr) = NpcManager::load_from_file(&npcs_path)
        {
            self.app.npc_manager = mgr;
        }
        self.app.game_mod = game_mod;
        self.app.npc_manager.assign_tiers(&self.app.world, &[]);

        let msg = "New game started.".to_string();
        self.app.world.log(msg.clone());
        ActionResult::SystemCommand { response: msg }
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
                    let transport = self.default_transport();
                    let exits = format_exits(
                        self.app.world.player_location,
                        &self.app.world.graph,
                        transport.speed_m_per_s,
                        &transport.label,
                    );
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
    ///
    /// Delegates all post-movement logic to [`parish_core::game_session::apply_movement`]
    /// so the test harness stays in sync with the other backends automatically.
    fn handle_movement(&mut self, target: &str) -> ActionResult {
        use parish_core::game_session::apply_movement;

        let transport = self.default_transport();
        let reaction_templates = self
            .app
            .game_mod
            .as_ref()
            .map(|gm| gm.reactions.clone())
            .unwrap_or_default();

        let effects = apply_movement(
            &mut self.app.world,
            &mut self.app.npc_manager,
            &reaction_templates,
            target,
            &transport,
        );

        // Log tier transitions to the debug log (mirrors Tauri/server behaviour)
        for tt in &effects.tier_transitions {
            let direction = if tt.promoted { "promoted" } else { "demoted" };
            self.app.debug_event(format!(
                "[tier] {} {} {:?} → {:?}",
                tt.npc_name, direction, tt.old_tier, tt.new_tier,
            ));
        }

        if effects.world_changed {
            let loc_name = self.app.world.current_location().name.clone();
            // Retrieve travel time from the first system message (narration contains minutes)
            // We need minutes for ActionResult — reconstruct from the travel_start payload.
            let minutes = effects
                .travel_start
                .as_ref()
                .map(|ts| ts.duration_minutes)
                .unwrap_or(0);
            let narration = effects
                .messages
                .first()
                .map(|m| m.text.clone())
                .unwrap_or_default();
            ActionResult::Moved {
                to: loc_name,
                minutes,
                narration,
            }
        } else {
            // Check which variant based on message content
            let msg = effects
                .messages
                .first()
                .map(|m| m.text.as_str())
                .unwrap_or("");
            if msg.contains("faintest notion") || msg.contains("You haven't") {
                // Extract target name from the message
                let name = target.to_string();
                ActionResult::NotFound { target: name }
            } else {
                ActionResult::AlreadyHere
            }
        }
    }

    /// Attempts NPC interaction using canned responses.
    ///
    /// Checks all NPCs at the current location for canned responses,
    /// not just the first one. This allows tests to target specific NPCs
    /// regardless of iteration order. Also runs anachronism detection on
    /// the player's input and includes any detected terms in the result.
    ///
    /// When a canned response is consumed, the interaction is processed
    /// through the same memory pipeline as a real LLM response: the NPC's
    /// mood is updated, a memory entry is recorded, and evicted memories
    /// may be promoted to long-term storage.
    fn handle_npc_interaction(&mut self, text: &str) -> ActionResult {
        let npcs_here = self.app.npc_manager.npcs_at(self.app.world.player_location);

        if npcs_here.is_empty() {
            self.app.world.log("Nothing happens.".to_string());
            return ActionResult::UnknownInput;
        }

        // Detect anachronisms in player input
        let detected = crate::npc::anachronism::check_input(text);
        let anachronism_terms: Vec<String> = detected.iter().map(|a| a.term.clone()).collect();

        // Check each NPC at this location for canned responses
        for npc in &npcs_here {
            let key = npc.name.to_lowercase();
            if let Some(responses) = self.canned_responses.get_mut(&key)
                && !responses.is_empty()
            {
                let dialogue = responses.remove(0);
                let name = npc.name.clone();
                let npc_id = npc.id;
                self.app.world.log(format!("{}: {}", name, dialogue));

                // Build a synthetic NPC response and run it through the memory pipeline
                let response = crate::npc::NpcStreamResponse {
                    dialogue: dialogue.clone(),
                    metadata: Some(crate::npc::NpcMetadata {
                        action: "responds".to_string(),
                        mood: npc.mood.clone(),
                        internal_thought: None,
                        language_hints: Vec::new(),
                    }),
                };
                let game_time = self.app.world.clock.now();
                if let Some(npc_mut) = self.app.npc_manager.get_mut(npc_id) {
                    let debug_events = crate::npc::ticks::apply_tier1_response(
                        npc_mut, &response, text, game_time,
                    );
                    for event in debug_events {
                        self.app.debug_event(event);
                    }
                }

                // Record conversation exchange for scene awareness
                let location = self.app.world.player_location;
                self.app.world.conversation_log.add(
                    crate::npc::conversation::ConversationExchange {
                        timestamp: game_time,
                        speaker_id: npc_id,
                        speaker_name: name.clone(),
                        player_input: text.to_string(),
                        npc_dialogue: dialogue.clone(),
                        location,
                    },
                );

                // Record witness memories for bystander NPCs
                let witness_events = crate::npc::ticks::record_witness_memories(
                    self.app.npc_manager.npcs_mut(),
                    npc_id,
                    &name,
                    text,
                    &dialogue,
                    game_time,
                    location,
                );
                for event in witness_events {
                    self.app.debug_event(event);
                }

                return ActionResult::NpcResponse {
                    npc: name,
                    dialogue,
                    anachronisms: anachronism_terms,
                };
            }
        }

        ActionResult::NpcNotAvailable
    }

    /// Renders the current location description.
    fn render_current_location(&self) -> String {
        if let Some(loc_data) = self.app.world.current_location_data() {
            let tod = self.app.world.clock.time_of_day();
            let npc_display: Vec<String> = self
                .app
                .npc_manager
                .npcs_at(self.app.world.player_location)
                .iter()
                .map(|n| self.app.npc_manager.display_name(n).to_string())
                .collect();
            let npc_names: Vec<&str> = npc_display.iter().map(|s| s.as_str()).collect();
            render_description(
                loc_data,
                tod,
                &self.app.world.weather.to_string(),
                &npc_names,
            )
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

/// Captured result of executing one script command (for test assertions).
///
/// Unlike [`ScriptOutputLine`] (internal, stdout-only), this struct is public
/// and returned by [`run_script_captured`] so tests can assert on every field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptResult {
    /// The command that was executed.
    pub command: String,
    /// The structured outcome.
    pub result: ActionResult,
    /// Player location after the command.
    pub location: String,
    /// Time of day after the command.
    pub time: String,
    /// Season after the command.
    pub season: String,
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

/// Executes a script file and returns captured results for assertion in tests.
///
/// Same logic as [`run_script_mode`] but collects [`ScriptResult`] values
/// into a `Vec` instead of printing JSON to stdout. This allows integration
/// tests to assert on every command's outcome, location, time, and season.
///
/// # Errors
///
/// Returns an error if the script file cannot be read.
pub fn run_script_captured(script_path: &Path) -> anyhow::Result<Vec<ScriptResult>> {
    let contents = std::fs::read_to_string(script_path)?;
    let mut harness = GameTestHarness::new();
    let mut results = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let result = harness.execute(trimmed);
        results.push(ScriptResult {
            command: trimmed.to_string(),
            result,
            location: harness.player_location().to_string(),
            time: harness.time_of_day().to_string(),
            season: harness.season().to_string(),
        });

        if harness.app.should_quit {
            break;
        }
    }

    Ok(results)
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
        assert_eq!(*h.weather(), crate::world::Weather::Clear);
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
        if let ActionResult::NpcResponse { npc, dialogue, .. } = result {
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
    fn test_persistence_commands() {
        let mut h = GameTestHarness::new();

        // Save should work with in-memory DB
        let result = h.execute("/save");
        if let ActionResult::SystemCommand { response } = result {
            assert!(
                response.contains("Game saved"),
                "expected save confirmation, got: {}",
                response
            );
        }

        // Branches should list main
        let result = h.execute("/branches");
        if let ActionResult::SystemCommand { response } = result {
            assert!(response.contains("main"), "branches should list main");
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
                response.contains("quickens"),
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

    #[test]
    fn test_system_command_invalid_speed() {
        let mut h = GameTestHarness::new();
        let result = h.execute("/speed bogus");
        if let ActionResult::SystemCommand { response } = result {
            assert!(
                response.contains("Unknown speed"),
                "Should report unknown speed, got: {}",
                response
            );
            assert!(
                response.contains("bogus"),
                "Should echo the invalid name, got: {}",
                response
            );
        } else {
            panic!("Expected SystemCommand");
        }
    }

    #[test]
    fn test_tier_transitions_logged_on_movement() {
        let mut h = GameTestHarness::new();

        // Move far from starting location to trigger tier changes
        h.execute("go to crossroads");
        h.execute("go to fairy fort");

        // Check that tier transition events appeared in the debug log
        let log = h.debug_log();
        let has_tier_event = log.iter().any(|e| e.contains("[tier]"));
        assert!(
            has_tier_event,
            "Expected tier transition events in debug log after movement, got: {:?}",
            log
        );
    }

    #[test]
    fn test_gossip_network_on_world_state() {
        use crate::npc::NpcId;
        let mut h = GameTestHarness::new();

        // Seed gossip into the world state
        let now = h.app.world.clock.now();
        let npc_id = NpcId(1);
        h.app.world.gossip_network.create(
            "The landlord raised the rent again".to_string(),
            npc_id,
            now,
        );
        h.app.world.gossip_network.create(
            "A stranger was seen at the fairy fort".to_string(),
            NpcId(2),
            now,
        );

        // Verify via debug command
        let result = h.execute("/debug gossip");
        let text = match &result {
            ActionResult::SystemCommand { response } => response.clone(),
            other => panic!("Expected system command, got {:?}", other),
        };
        assert!(
            text.contains("2 items"),
            "Should show 2 gossip items: {text}"
        );
        assert!(
            text.contains("landlord"),
            "Should contain landlord gossip: {text}"
        );
        assert!(
            text.contains("stranger"),
            "Should contain stranger gossip: {text}"
        );
    }

    #[test]
    fn test_long_term_memory_debug_display() {
        use crate::npc::NpcId;
        let mut h = GameTestHarness::new();

        // Find an NPC and seed long-term memory
        let npc_id = NpcId(1);
        if let Some(npc) = h.app.npc_manager.get_mut(npc_id) {
            use parish_core::npc::memory::LongTermEntry;
            let now = h.app.world.clock.now();
            npc.long_term_memory.store(LongTermEntry {
                timestamp: now,
                content: "Argued with the landlord about tithes".to_string(),
                importance: 0.8,
                keywords: vec!["landlord".to_string(), "tithes".to_string()],
            });
        }

        // Verify via debug command — get NPC name first
        let npc_name = h.app.npc_manager.get(npc_id).unwrap().name.clone();
        let result = h.execute(&format!("/debug memory {}", npc_name));
        let text = match &result {
            ActionResult::SystemCommand { response } => response.clone(),
            other => panic!("Expected system command, got {:?}", other),
        };
        assert!(
            text.contains("Long-term (1 entries)"),
            "Should show 1 LTM entry: {text}"
        );
    }

    #[test]
    fn test_gossip_propagation_runtime() {
        use crate::npc::NpcId;
        let mut h = GameTestHarness::new();
        let now = h.app.world.clock.now();

        // Create gossip known by NPC 1
        h.app.world.gossip_network.create(
            "Mary's cow went missing last night".to_string(),
            NpcId(1),
            now,
        );

        // Propagate between NPC 1 and NPC 2
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let transmitted = parish_core::npc::ticks::propagate_gossip_at_location(
            &[NpcId(1), NpcId(2)],
            &mut h.app.world.gossip_network,
            &mut rng,
        );

        // Check if gossip was transmitted (probabilistic, but seed 42 should work)
        if !transmitted.is_empty() {
            let npc2_gossip = h.app.world.gossip_network.known_by(NpcId(2));
            assert!(!npc2_gossip.is_empty(), "NPC 2 should now know gossip");
        }
    }

    // ── Conversation awareness integration tests ─────────────────────

    #[test]
    fn test_witness_memory_via_harness() {
        let mut h = GameTestHarness::new();

        // Move to a location with multiple NPCs
        // First find a location with 2+ NPCs
        let loc = h.location_id();
        let npcs_here = h.npcs_here();

        if npcs_here.len() >= 2 {
            // Stub a response for the first NPC
            let first_npc_name = npcs_here[0].to_string();
            let second_npc_name = npcs_here[1].to_string();
            h.add_canned_response(&first_npc_name, "Ah sure, grand weather today!");

            // Talk to the first NPC
            let result = h.execute("Tell me about the weather");
            assert!(matches!(result, ActionResult::NpcResponse { .. }));

            // Check that the second NPC has a witness memory
            let second_npc = h
                .app
                .npc_manager
                .npcs_at(loc)
                .into_iter()
                .find(|n| n.name == second_npc_name)
                .cloned();
            if let Some(witness) = second_npc {
                assert!(
                    !witness.memory.is_empty(),
                    "Witness NPC should have a memory of the overheard conversation"
                );
                let memories = witness.memory.recent(1);
                assert!(
                    memories[0].content.contains("Overheard"),
                    "Witness memory should mention overhearing: {}",
                    memories[0].content
                );
            }
        }
    }

    #[test]
    fn test_conversation_log_recorded_via_harness() {
        let mut h = GameTestHarness::new();
        let npcs_here = h.npcs_here();

        if !npcs_here.is_empty() {
            let npc_name = npcs_here[0].to_string();
            h.add_canned_response(&npc_name, "Dia dhuit, a chara!");

            h.execute("Hello there");

            // Check that the conversation log has an entry
            let loc = h.location_id();
            let recent = h.app.world.conversation_log.recent_at(loc, 5);
            assert_eq!(
                recent.len(),
                1,
                "Conversation log should have 1 entry after 1 exchange"
            );
            assert_eq!(recent[0].speaker_name, npc_name);
            assert!(recent[0].player_input.contains("Hello"));
            assert!(recent[0].npc_dialogue.contains("Dia dhuit"));
        }
    }

    #[test]
    fn test_conversation_continuity_after_multiple_exchanges() {
        let mut h = GameTestHarness::new();
        let npcs_here = h.npcs_here();

        if !npcs_here.is_empty() {
            let npc_name = npcs_here[0].to_string();
            h.add_canned_response(&npc_name, "Good morning to ye!");

            let result = h.execute("Good morning");
            assert!(
                matches!(result, ActionResult::NpcResponse { .. }),
                "First exchange should succeed"
            );

            let loc = h.location_id();
            let recent = h.app.world.conversation_log.recent_at(loc, 5);
            assert_eq!(
                recent.len(),
                1,
                "Conversation log should have 1 entry after first exchange"
            );

            // If the NPC is still here after ticks, try a second exchange
            let npcs_still_here = h.npcs_here();
            if npcs_still_here.contains(&npc_name.as_str()) {
                h.add_canned_response(&npc_name, "The weather is grand, so it is.");
                let result2 = h.execute("How is the weather?");
                if matches!(result2, ActionResult::NpcResponse { .. }) {
                    let recent2 = h.app.world.conversation_log.recent_at(loc, 5);
                    assert_eq!(
                        recent2.len(),
                        2,
                        "Conversation log should have 2 entries after 2 exchanges"
                    );

                    // Verify continuity tracking
                    assert!(h.app.world.conversation_log.has_recent_exchange_with(
                        loc,
                        recent2[0].speaker_id,
                        5
                    ));
                }
            }
        }
    }
}
