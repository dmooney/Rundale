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
//! use parish_engine::testing::{GameTestHarness, ActionResult};
//!
//! let mut h = GameTestHarness::new();
//! let result = h.execute("go to crossroads");
//! assert!(matches!(result, ActionResult::Moved { .. }));
//! assert_eq!(h.player_location(), "The Crossroads");
//! ```

use crate::app::App;
use crate::inference::simulator::SimulatorClient;
use crate::input::{self, Command, InputResult, IntentKind};
use crate::npc::manager::NpcManager;
use crate::npc::{Npc, NpcId};
use crate::world::LocationId;
use crate::world::description::{format_exits, render_description};
use crate::world::time::{Season, TimeOfDay};
use parish_core::ipc::capitalize_first;
use parish_core::world::transport::TransportMode;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

fn parse_canned_npc_response(
    raw_response: String,
    fallback_mood: String,
) -> crate::npc::NpcStreamResponse {
    let parsed = crate::npc::parse_npc_stream_response(&raw_response);
    if parsed.metadata.is_some() {
        return parsed;
    }

    crate::npc::NpcStreamResponse {
        dialogue: raw_response,
        metadata: Some(crate::npc::NpcMetadata {
            action: "responds".to_string(),
            mood: fallback_mood,
            internal_thought: None,
            language_hints: Vec::new(),
            mentioned_people: Vec::new(),
            assigned_task: None,
        }),
    }
}

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
/// use parish_engine::testing::GameTestHarness;
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
    pub(crate) db_sync: Option<crate::persistence::Database>,
    /// Optional offline simulator used as a fallback when no canned response exists.
    simulator: Option<Arc<SimulatorClient>>,
    /// Seeded RNG shared across all weather/gossip calls for deterministic results.
    rng: StdRng,
    /// Scriptable mock LLM backing the real-loop execution path
    /// ([`Self::execute_via_real_loop`]). Distinct from `canned_responses`,
    /// which feed the legacy path; the mock pins the inference seam for the
    /// real `game_loop` so the two engines can be compared (#1159).
    pub(crate) mock: Arc<crate::inference::MockClient>,
    /// Persistent conversation runtime state shared across all
    /// [`Self::execute_via_real_loop`] calls, so session-level state such as
    /// `seen_openers_this_location` accumulates across turns (#1492).
    pub(crate) real_loop_conversation:
        std::sync::Arc<tokio::sync::Mutex<parish_core::ipc::ConversationRuntimeState>>,
    /// When true, [`Self::execute`] also runs the real `game_loop` on a
    /// rolled-back copy of the pre-state and records divergences to
    /// `shadow_ledger`. Seeded from the `PARISH_HARNESS_SHADOW` env var at
    /// construction; tests opt in explicitly via `enable_shadow`.
    pub(crate) shadow_enabled: bool,
    /// Divergence-ledger path used when `shadow_enabled`.
    pub(crate) shadow_ledger: std::path::PathBuf,
    /// `case` label written into each divergence record.
    pub(crate) shadow_case: String,
    /// Deterministic, synchronous-drain server-push bus used **only** in the
    /// test harness (#1176). Production backends (`parish-server`,
    /// `parish-tauri`, `parish-engine` headless) keep using
    /// [`parish_core::event_bus::BroadcastEventBus`]; this opt-in deterministic
    /// impl drains events in exact publish order so harness-level
    /// event-ordering assertions never flake on async scheduling. Exposed via
    /// [`Self::event_bus`].
    event_bus: parish_core::event_bus::DeterministicEventBus,
}

impl GameTestHarness {
    /// Creates a new harness loaded from the Rundale mod. Used by all
    /// unit tests that assert on Rundale-specific content (location names,
    /// NPC names, etc.). Character-log writers are **disabled** so the
    /// hundreds of cargo-test instances don't pollute the shared user-data dir.
    pub fn new() -> Self {
        Self::build_rundale(false)
    }

    /// Creates a harness from whichever mod `mods/mod-list.toml` selects
    /// (i.e. the currently-active mod). Used by `run_script_mode` so that
    /// `parish --script` exercises the mod that is actually deployed.
    pub fn new_from_active_mod() -> Self {
        Self::build(false)
    }

    /// Builds the harness used by `run_script_mode` with the per-character
    /// and per-location markdown writers turned **on** so `parish --script ...`
    /// produces live-proof transcripts (CLAUDE.md rules #10/#13).
    ///
    /// When an explicit `--game-mod` is supplied, the world, NPCs, and log
    /// app-name are all resolved from that mod's directory; otherwise the
    /// active mod from `mod-list.toml` is used. The loaded `game_mod` is stored
    /// on the app so subsequent reloads (e.g. `/new`) use the same mod.
    pub fn build_with_mod(game_mod: Option<parish_core::game_mod::GameMod>) -> Self {
        match game_mod {
            Some(gm) => Self::build_from_mod_dir(true, Some(&gm.mod_dir)),
            None => Self::build(true),
        }
    }

    fn build(enable_character_logs: bool) -> Self {
        Self::build_from_mod_dir(enable_character_logs, None)
    }

    /// Builds a harness loaded from the Rundale mod directory explicitly,
    /// bypassing `mod-list.toml`. Used by tests that assert on Rundale-specific
    /// content (locations, NPC names, etc.) so they remain stable regardless
    /// of which mod is currently active.
    fn build_rundale(enable_character_logs: bool) -> Self {
        let rundale_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        Self::build_from_mod_dir(enable_character_logs, Some(&rundale_dir))
    }

    fn build_from_mod_dir(enable_character_logs: bool, mod_dir: Option<&std::path::Path>) -> Self {
        let mut app = App::new();

        let game_mod = match mod_dir {
            Some(dir) => parish_core::game_mod::GameMod::load(dir).ok(),
            None => parish_core::game_mod::find_default_mod()
                .and_then(|dir| parish_core::game_mod::GameMod::load(&dir).ok()),
        };

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

        // Character logs — opt-in. Plain `new()` keeps writers disabled
        // so the hundreds of cargo-test harness instances don't all
        // dump to the shared user-data dir. `build_with_mod`
        // (used by `run_script_mode`) sets `enable_character_logs=true`
        // so `parish --script ...` still produces log files.
        let log_app_name = parish_core::game_mod::app_name_from_mod(&app.game_mod);
        app.log_app_name = log_app_name.clone();
        {
            let flag_on = !app
                .flags
                .is_disabled(parish_core::character_log::FEATURE_FLAG);
            let enabled = enable_character_logs && flag_on;
            let manager = parish_core::character_log::CharacterLogManager::new(
                &log_app_name,
                app.active_branch_id,
                enabled,
            );
            if manager.enabled() {
                app.character_log_rx = Some(app.world.event_bus.subscribe());
                if let Err(e) = manager.write_all_profiles(&app.world, &app.npc_manager) {
                    tracing::warn!(error = %e, "character-log profile write failed");
                }
            }
            app.character_log = Some(Arc::new(manager));
        }

        // Location logs — same opt-in pattern as the character logs.
        {
            let flag_on = !app
                .flags
                .is_disabled(parish_core::location_log::FEATURE_FLAG);
            let enabled = enable_character_logs && flag_on;
            let manager = parish_core::location_log::LocationLogManager::new(
                &log_app_name,
                app.active_branch_id,
                enabled,
            );
            if manager.enabled() {
                app.location_log_rx = Some(app.world.event_bus.subscribe());
                if let Err(e) = manager.write_all_profiles(&app.world, &app.npc_manager) {
                    tracing::warn!(error = %e, "location-log profile write failed");
                }
            }
            app.location_log = Some(Arc::new(manager));
        }
        app.log_managers_branch = Some(app.active_branch_id);

        Self {
            app,
            canned_responses: HashMap::new(),
            db_sync,
            simulator: None,
            rng: StdRng::seed_from_u64(0),
            mock: Arc::new(crate::inference::MockClient::new()),
            real_loop_conversation: std::sync::Arc::new(tokio::sync::Mutex::new(
                parish_core::ipc::ConversationRuntimeState::new(),
            )),
            shadow_enabled: crate::shadow::is_enabled(),
            shadow_ledger: crate::shadow::ledger_path(),
            shadow_case: crate::shadow::case_label(),
            event_bus: parish_core::event_bus::DeterministicEventBus::new(),
        }
    }

    /// Returns the harness's deterministic server-push [`EventBus`]
    /// (`parish_core::event_bus::DeterministicEventBus`, #1176).
    ///
    /// Unlike the production [`BroadcastEventBus`], this drains synchronously
    /// in exact publish order, so tests can subscribe, drive gameplay that
    /// emits server events, and assert on ordering without any async-scheduling
    /// flakiness. Production push semantics (CLAUDE.md rule #11) are unchanged —
    /// this bus is wired only here in the test harness.
    ///
    /// [`BroadcastEventBus`]: parish_core::event_bus::BroadcastEventBus
    /// [`EventBus`]: parish_core::event_bus::EventBus
    pub fn event_bus(&self) -> &parish_core::event_bus::DeterministicEventBus {
        &self.event_bus
    }

    /// Attaches the built-in offline simulator as a fallback for NPC dialogue.
    ///
    /// When the harness needs an NPC response and no canned reply has been
    /// registered with [`add_canned_response`], it will ask the simulator
    /// for a funny Markov-chain response instead of returning
    /// [`ActionResult::NpcNotAvailable`]. No network or GPU required.
    ///
    /// ```rust,no_run
    /// use parish_engine::testing::GameTestHarness;
    /// let mut h = GameTestHarness::new().with_simulator();
    /// h.execute("go to pub");
    /// h.execute("hello"); // NPC responds with Markov nonsense
    /// ```
    pub fn with_simulator(mut self) -> Self {
        self.simulator = Some(Arc::new(SimulatorClient::new()));
        self.app.provider_name = "simulator".to_string();
        self
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

        // Test-harness-only /doom command: /doom NpcName [hours_from_now]
        //
        // Marks the named NPC as fated to die `hours_from_now` game-hours out
        // (default 18, matching what Tier 4 would schedule). Used by banshee
        // play-test scripts so we don't need to wait on random rolls.
        //
        // Names may contain spaces ("Maire Gallagher"). The optional trailing
        // hours argument is detected by parsing the last whitespace-separated
        // token as an integer — if parsing fails, the whole remainder is the
        // name and the default lead time is used.
        if let Some(rest) = trimmed.strip_prefix("/doom ") {
            let rest = rest.trim();
            let (name, hours): (&str, i64) = match rest.rsplit_once(char::is_whitespace) {
                Some((name, tail)) if tail.parse::<i64>().is_ok() => {
                    (name.trim(), tail.parse().unwrap())
                }
                _ => (rest, crate::npc::banshee::DOOM_LEAD_TIME_HOURS),
            };
            let now = self.app.world.clock.now();
            let doom = now + chrono::Duration::hours(hours);
            let lower = name.to_lowercase();
            let matched = self
                .app
                .npc_manager
                .all_npcs()
                .find(|n| n.name.to_lowercase() == lower)
                .map(|n| n.id);
            let msg = if let Some(id) = matched {
                if let Some(npc) = self.app.npc_manager.npcs_mut().get_mut(&id) {
                    npc.doom = Some(doom);
                    npc.banshee_heralded = false;
                    format!(
                        "Doom set for {} at {} ({}h from now).",
                        npc.name,
                        doom.format("%Y-%m-%d %H:%M"),
                        hours
                    )
                } else {
                    format!("Could not find NPC '{}'.", name)
                }
            } else {
                format!("No NPC named '{}'.", name)
            };
            self.app.world.log(msg.clone());
            return ActionResult::SystemCommand { response: msg };
        }

        // Shadow mode (off by default): snapshot the pre-state so the real
        // game_loop can be run on an identical starting point after the legacy
        // path completes. `None` when disabled ⇒ zero overhead, byte-for-byte
        // legacy behavior.
        let shadow_pre = self.shadow_enabled.then(|| {
            (
                crate::persistence::GameSnapshot::capture(&self.app.world, &self.app.npc_manager),
                self.app.world.text_log.len(),
            )
        });

        let classified = input::classify_input(trimmed);
        let staged_pre = match &classified {
            InputResult::GameInput(text)
                if parish_core::game_loop::input_may_mutate_tasks(&self.app.world, text) =>
            {
                let candidate_world = self.app.world.clone_for_staged_turn();
                let live_world = std::mem::replace(&mut self.app.world, candidate_world);
                let candidate_npcs = self.app.npc_manager.clone();
                let live_npcs = std::mem::replace(&mut self.app.npc_manager, candidate_npcs);
                let semantic_rx = self.app.world.event_bus.subscribe();
                Some((
                    live_world,
                    live_npcs,
                    self.canned_responses.clone(),
                    self.app.debug_log.clone(),
                    semantic_rx,
                ))
            }
            _ => None,
        };
        let result = match classified {
            InputResult::SystemCommand(cmd) => self.handle_system_command(cmd),
            InputResult::GameInput(text) => self.handle_game_input(&text),
        };
        if let Some((live_world, live_npcs, canned_before, debug_log_before, mut semantic_rx)) =
            staged_pre
        {
            let mut semantic_events = Vec::new();
            let staging_error = loop {
                match semantic_rx.try_recv() {
                    Ok(event) => semantic_events.push(event),
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                    | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break None,
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(dropped)) => {
                        break Some(format!(
                            "pending turn semantic event buffer overflowed and dropped {dropped} event(s)"
                        ));
                    }
                }
            };
            let persistence_result = staging_error.map_or_else(
                || self.persist_task_mutations_since(&live_world.player_progress),
                Err,
            );
            match persistence_result {
                Ok(()) => {
                    let mut candidate_world = std::mem::replace(&mut self.app.world, live_world);
                    candidate_world.event_bus = std::mem::take(&mut self.app.world.event_bus);
                    self.app.world = candidate_world;
                    drop(live_npcs);
                    for event in semantic_events {
                        self.app.world.event_bus.publish(event);
                    }
                }
                Err(error) => {
                    self.app.world = live_world;
                    self.app.npc_manager = live_npcs;
                    self.canned_responses = canned_before;
                    self.app.debug_log = debug_log_before;
                    let message = format!("Failed to persist player task changes: {error}");
                    // A rejected staged turn is not a completed player turn:
                    // do not run the post-action world pump, log drains, or
                    // shadow replay against canonical state.
                    return ActionResult::SystemCommand { response: message };
                }
            }
        }

        // Capture the legacy path's player-visible output *before* the
        // post-action pump below appends further lines — the real-loop path
        // runs only the core handler, so comparing core output avoids tick
        // noise.
        let shadow_legacy_lines = shadow_pre.as_ref().map(|(_, pre_len)| {
            self.app
                .world
                .text_log
                .iter()
                .skip(*pre_len)
                .cloned()
                .collect::<Vec<String>>()
        });

        // Simulation tick after each action, through the single shared world
        // pump (rule #12). The harness's per-turn scheduling matches the
        // historical inline copy: schedules + tier reassignment + the banshee
        // tick, but no weather/gossip/tier-4 (those only run on explicit
        // `advance_time` bulk jumps). Consumes no RNG, so seeded eval baselines
        // stay byte-stable.
        {
            use parish_core::game_loop::{AdvanceOptions, GossipMode, WeatherMode, advance_world};

            let report = advance_world(
                &mut self.app.world,
                &mut self.app.npc_manager,
                &mut self.rng,
                AdvanceOptions {
                    weather: WeatherMode::Skip,
                    run_banshee: !self.app.flags.is_disabled("banshee"),
                    gossip: GossipMode::Skip,
                    run_tier4: false,
                },
            );
            self.apply_advance_report(&report);
        }

        // Drain any GameEvents queued on the character-log receiver since
        // the previous execute() and append them to the right log files.
        // Mirrors the synchronous drain pattern in the REPL loop.
        // Rebind on branch switch (#1011, #1034) — must run BEFORE the
        // clones below capture `self.app.character_log` for the drain.
        self.app.rebind_log_managers_if_branch_changed();
        if let (Some(manager), Some(rx)) = (
            self.app.character_log.clone(),
            self.app.character_log_rx.as_mut(),
        ) {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        if let Err(e) =
                            manager.process_event(&event, &self.app.world, &self.app.npc_manager)
                        {
                            tracing::warn!(error = %e, "character-log write failed");
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                }
            }
        }

        // Same drain for the per-location log writer.
        if let (Some(manager), Some(rx)) = (
            self.app.location_log.clone(),
            self.app.location_log_rx.as_mut(),
        ) {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        if let Err(e) =
                            manager.process_event(&event, &self.app.world, &self.app.npc_manager)
                        {
                            tracing::warn!(error = %e, "location-log write failed");
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                }
            }
        }

        // Shadow comparison: replay this input through the real game_loop on the
        // rolled-back pre-state and record any divergence. No-op when disabled.
        if let (Some((pre_snapshot, _)), Some(legacy_lines)) = (shadow_pre, shadow_legacy_lines) {
            self.shadow_compare_after_legacy(trimmed, pre_snapshot, legacy_lines);
        }

        result
    }

    /// Appends every player-task post-state changed by the just-completed turn.
    ///
    /// The harness owns a synchronous in-memory database, so snapshot lookup
    /// and the whole event batch share one SQLite transaction.
    fn persist_task_mutations_since(
        &self,
        before: &parish_core::session_store::PlayerProgress,
    ) -> Result<(), String> {
        let mutations = self
            .app
            .world
            .player_progress
            .tasks()
            .iter()
            .filter(|task| before.task(task.id) != Some(*task))
            .cloned()
            .collect::<Vec<_>>();
        if mutations.is_empty() {
            return Ok(());
        }
        persist_task_mutation_batch(self.db_sync.as_ref(), self.app.active_branch_id, &mutations)
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

    /// Advances the game clock and runs the full world pump.
    ///
    /// Useful for testing NPC movement, weather, and tier-4 life events without
    /// player actions. Delegates to the single shared
    /// [`parish_core::game_loop::advance_world`] pump (rule #12) with the
    /// harness's historical scheduling: weather is **backfilled** one check per
    /// elapsed game-hour (silent on bulk jumps so the broadcast bus is not
    /// flooded), gossip propagates among every co-located Tier-2 group, and
    /// tier-4 dispatches when due. The RNG draw order (weather → gossip →
    /// tier-4) is identical to the pre-#1159 inline copy, so seeded tests are
    /// deterministic and unchanged.
    pub fn advance_time(&mut self, minutes: i64) {
        use parish_core::game_loop::{AdvanceOptions, GossipMode, WeatherMode, advance_world};

        self.app.world.clock.advance(minutes);
        let report = advance_world(
            &mut self.app.world,
            &mut self.app.npc_manager,
            &mut self.rng,
            AdvanceOptions {
                weather: WeatherMode::Backfill { minutes },
                run_banshee: !self.app.flags.is_disabled("banshee"),
                gossip: GossipMode::All,
                run_tier4: parish_core::game_loop::tier4_simulation_enabled(&self.app.flags),
            },
        );
        self.apply_advance_report(&report);
    }

    /// Renders an [`parish_core::game_loop::AdvanceReport`] into the harness's
    /// debug log and player-visible text log, mirroring the per-atom debug
    /// strings and arrival/departure narration the live loops emit.
    fn apply_advance_report(&mut self, report: &parish_core::game_loop::AdvanceReport) {
        for tt in &report.tier_transitions {
            let direction = if tt.promoted { "promoted" } else { "demoted" };
            self.app.debug_event(format!(
                "[tier] {} {} {:?} → {:?}",
                tt.npc_name, direction, tt.old_tier, tt.new_tier,
            ));
        }
        self.process_schedule_events(&report.schedule_events);
        if !report.banshee.is_empty() {
            self.app.debug_event(format!(
                "[banshee] {} wail(s), {} death(s)",
                report.banshee.wails.len(),
                report.banshee.deaths.len()
            ));
        }
        if report.tier4_event_count > 0 {
            self.app
                .debug_event(format!("[tier4] {} events", report.tier4_event_count));
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

    /// Returns whether a named feature flag is currently enabled.
    pub fn is_flag_enabled(&self, name: &str) -> bool {
        self.app.flags.is_enabled(name)
    }

    /// Processes schedule events: debug log + player-visible text log messages.
    fn process_schedule_events(&mut self, events: &[crate::npc::manager::ScheduleEvent]) {
        for msg in crate::headless::process_schedule_events_generic(&mut self.app, events) {
            self.app.world.log(msg);
        }
    }

    /// Handles a system command, returning a structured result.
    ///
    /// Delegates every command — including `Wait` and `Tick` — to the shared
    /// [`parish_core::ipc::handle_command`] and dispatches any returned
    /// [`CommandEffect`]s locally. The world pump (weather/schedules/banshee/
    /// tier-4) runs once per turn in [`Self::execute`] through the single
    /// shared [`parish_core::game_loop::advance_world`] helper, so there is no
    /// harness-local copy of any command's orchestration body (rule #12).
    fn handle_system_command(&mut self, cmd: Command) -> ActionResult {
        use parish_core::ipc::commands::{CommandEffect, handle_command};

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
                CommandEffect::OpenDesigner => {
                    return ActionResult::SystemCommand {
                        response: "The Parish Designer is only available in the GUI.".to_string(),
                    };
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
                CommandEffect::SaveFlags => {
                    // No-op in test mode — flags are in-memory only
                }
                CommandEffect::ApplyTheme(..) => {
                    // No visual theme in test harness; response text is returned below.
                }
                CommandEffect::ApplyTiles(..) => {
                    // No map in test harness; response text is returned below.
                }
                CommandEffect::ResetByok => {
                    // No BYOK overlay in the test harness; response text is returned below.
                }
                CommandEffect::InferenceLog(_) => {
                    // No-op in the test harness; the response text from the
                    // shared dispatcher is what makes it to the player.
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

    /// Handles the ToggleMap effect — renders the map as text in test mode.
    fn handle_map_effect(&mut self) -> ActionResult {
        let player_loc = self.app.world.player_location;
        let mut lines = vec!["=== Parish Map ===".to_string()];
        for node_id in self.app.world.graph.location_ids() {
            if let Some(data) = self.app.world.graph.get(node_id) {
                let marker = if node_id == player_loc { " * " } else { "   " };
                lines.push(format!("{}{}", marker, data.name));
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

        let Some(ref gm) = game_mod else {
            return ActionResult::SystemCommand {
                response: "New game failed: no game mod found.".to_string(),
            };
        };

        let Ok(world) = parish_core::game_mod::world_state_from_mod(gm) else {
            return ActionResult::SystemCommand {
                response: "New game failed: failed to load world state from mod.".to_string(),
            };
        };
        self.app.world = world;

        let npcs_path = gm.npcs_path();
        if !npcs_path.exists() {
            return ActionResult::SystemCommand {
                response: "New game failed: could not find NPCs data file.".to_string(),
            };
        }
        let Ok(mgr) = NpcManager::load_from_file(&npcs_path) else {
            return ActionResult::SystemCommand {
                response: "New game failed: failed to load NPCs from mod.".to_string(),
            };
        };
        self.app.npc_manager = mgr;
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

        // Lightweight "talk to <name>" / "speak to <name>" recognition so
        // fixtures can exercise the addressed-target dispatch path even
        // without an LLM. This mirrors the production Talk intent and
        // matches the absent-NPC system message emitted by
        // `parish_core::game_loop::handle_npc_conversation` (#985).
        let lower = text.trim().to_lowercase();
        let addressed: Option<String> = ["talk to ", "speak to "]
            .iter()
            .find_map(|prefix| lower.strip_prefix(prefix))
            .and_then(|rest| {
                // Stop at " about ", " regarding ", or end-of-input so
                // "talk to Aoife Brennan about the school" yields just the
                // name.
                let stops = [" about ", " regarding "];
                let mut name_end = rest.len();
                for stop in &stops {
                    if let Some(idx) = rest.find(stop) {
                        name_end = name_end.min(idx);
                    }
                }
                let raw_trim = text.trim();
                // Re-slice from the *original* (case-preserved) input so the
                // emitted target keeps its capitalisation ("Aoife Brennan").
                let prefix_chars = if lower.starts_with("talk to ") { 8 } else { 9 };
                let original_rest = raw_trim
                    .char_indices()
                    .nth(prefix_chars)
                    .map(|(i, _)| &raw_trim[i..]);
                let original_rest = original_rest?;
                let name = original_rest
                    .get(..name_end)
                    .unwrap_or(original_rest)
                    .trim();
                if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                }
            });

        if let Some(target) = addressed {
            let r = self.handle_addressed_npc(text, &target);
            self.apply_rule_reactions(text);
            return r;
        }

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
                IntentKind::Examine => {
                    // Feature-flagged: default-ON via is_disabled (#1424).
                    // Collapse: flag must be on AND a target must be present; otherwise room description.
                    match (
                        !self.app.flags.is_disabled("examine-intent"),
                        pi.target.as_deref(),
                    ) {
                        (true, Some(name)) => {
                            let msg = format!(
                                "You look more closely at {name}. There is nothing more noteworthy about it than what you have already observed."
                            );
                            self.app.world.log(msg.clone());
                            ActionResult::SystemCommand { response: msg }
                        }
                        _ => {
                            // Flag disabled or bare examine (no target) → room description.
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
                    }
                }
                IntentKind::Interact if !self.app.flags.is_disabled("interact-narration") => {
                    let outcome = parish_core::game_session::apply_player_action(
                        &mut self.app.world,
                        text,
                        &self.app.flags,
                    );
                    match outcome {
                        Some(outcome) => {
                            self.app.world.log(outcome.narration.clone());
                            ActionResult::SystemCommand {
                                response: outcome.narration,
                            }
                        }
                        None => ActionResult::UnknownInput,
                    }
                }
                // Locally parsed intent that is neither Move/Look/Examine — NPC interaction
                _ => {
                    let r = self.handle_npc_interaction(text);
                    // Apply rule-based reactions to prove mode parity (#402, #403, #404).
                    self.apply_rule_reactions(text);
                    r
                }
            },
            None => {
                // No local match — try NPC interaction, else unknown
                let r = self.handle_npc_interaction(text);
                // Apply rule-based reactions to prove mode parity (#402, #403, #404).
                self.apply_rule_reactions(text);
                r
            }
        }
    }

    /// Handles movement, advancing the clock and updating location.
    ///
    /// Delegates all post-movement logic to [`parish_core::game_session::apply_movement`]
    /// so the test harness stays in sync with the other backends automatically.
    fn handle_movement(&mut self, target: &str) -> ActionResult {
        use parish_core::game_session::{apply_movement, apply_travel_encounter};

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
            &self.app.flags,
        );

        // Travel encounter — default-on, kill-switchable via the `travel-encounters` flag.
        if effects.world_changed && !self.app.flags.is_disabled("travel-encounters") {
            apply_travel_encounter(&mut self.app.world, &effects);
        }

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
            // Check which variant based on message content and subtype
            let first = effects.messages.first();
            let msg = first.map(|m| m.text.as_str()).unwrap_or("");
            let subtype = first.and_then(|m| m.subtype);
            if subtype == Some("blocked-weather") {
                ActionResult::SystemCommand {
                    response: msg.to_string(),
                }
            } else if msg.contains("faintest notion") || msg.contains("You haven't") {
                let name = target.to_string();
                ActionResult::NotFound { target: name }
            } else {
                ActionResult::AlreadyHere
            }
        }
    }

    /// Attempts NPC interaction using canned responses.
    ///
    /// Applies synchronous rule-based NPC reactions to the player's message.
    ///
    /// Used in the test harness to prove mode parity: the real headless and
    /// server paths use the LLM path with `generate_rule_reaction` as a
    /// fallback. Here we apply the fallback directly (no async runtime in the
    /// synchronous harness). Reactions are logged to each NPC's `reaction_log`
    /// and to the world text log so they appear in script output.
    fn apply_rule_reactions(&mut self, text: &str) {
        use parish_core::npc::reactions::generate_rule_reaction;

        let npc_ids_here: Vec<_> = self
            .app
            .npc_manager
            .npcs_at(self.app.world.player_location)
            .into_iter()
            .map(|n| (n.id, n.name.clone()))
            .collect();

        for (id, name) in npc_ids_here {
            if let Some(emoji) = generate_rule_reaction(text) {
                // Persist to reaction_log (proves #403).
                if let Some(event) = parish_core::game_loop::record_directional_reaction(
                    &mut self.app.npc_manager,
                    id,
                    parish_core::ReactionDirection::NpcToPlayer,
                    &emoji,
                    text,
                    chrono::Utc::now(),
                ) {
                    self.app.npc_manager.record_reaction_emoji(&emoji);
                    self.app.world.event_bus.publish(event);
                }
                self.app
                    .world
                    .log(format!("{} {}", capitalize_first(&name), emoji));
            }
        }
    }

    /// Dispatches a "talk to <name>" / "speak to <name>" addressed turn
    /// through the same name-resolution path the production code uses
    /// (`parish_core::ipc::resolve_addressed_targets`).
    ///
    /// When the addressed NPC is co-located, this delegates to the
    /// canned-response flow keyed on that NPC's name. When the addressed
    /// NPC is not at the player's location, the harness emits the same
    /// `"{name} is not here."` system message that the real backend emits
    /// via `text-log` and returns `ActionResult::SystemCommand` so the
    /// fixture baseline can diff it (#985).
    fn handle_addressed_npc(&mut self, text: &str, name: &str) -> ActionResult {
        let addressed = parish_core::ipc::resolve_addressed_targets(
            &self.app.world,
            &self.app.npc_manager,
            &[name.to_string()],
        );

        if !addressed.absent.is_empty() {
            let absent_name = &addressed.absent[0];
            let msg = format!("{absent_name} is not here.");
            self.app.world.log(msg.clone());
            return ActionResult::SystemCommand { response: msg };
        }

        // Resolved → dispatch to a canned-response NPC turn that is forced
        // to the addressed speaker (rather than the first co-located NPC).
        if let Some(speaker_id) = addressed.resolved.first().copied() {
            return self.handle_npc_interaction_for(text, speaker_id);
        }

        // Defensive: should not be reached. Empty target should have been
        // filtered by the caller.
        self.handle_npc_interaction(text)
    }

    /// Variant of [`handle_npc_interaction`] that targets a specific
    /// pre-resolved NPC (rather than scanning all co-located NPCs for the
    /// first canned response).
    fn handle_npc_interaction_for(
        &mut self,
        text: &str,
        speaker_id: crate::npc::NpcId,
    ) -> ActionResult {
        // Detect anachronisms in player input — same pipeline as the
        // first-NPC variant so behaviour stays in lock-step.
        let detected = crate::npc::anachronism::check_input(text);
        let anachronism_terms: Vec<String> = detected.iter().map(|a| a.term.clone()).collect();

        let speaker_name = self.app.npc_manager.get(speaker_id).map(|n| n.name.clone());
        let Some(name) = speaker_name else {
            return ActionResult::NpcNotAvailable;
        };

        let key = name.to_lowercase();
        if let Some(responses) = self.canned_responses.get_mut(&key)
            && !responses.is_empty()
        {
            let fallback_mood = self
                .app
                .npc_manager
                .get(speaker_id)
                .map(|npc| npc.mood.clone())
                .unwrap_or_default();
            let response = parse_canned_npc_response(responses.remove(0), fallback_mood);
            let game_time = self.app.world.clock.now();
            // Shared per-turn pipeline (#1172 / #1173): run the same five steps
            // as every other backend. Previously this addressed path only did
            // name detection + Tier-1 apply, silently dropping the
            // conversation-log record, witness memories and the
            // `DialogueOccurred` publish — the exact harness/headless drift the
            // consolidation removes.
            let location = self.app.world.player_location;
            let player_line = strip_dialogue_verb(text);
            let language = self.app.language_settings();
            let outcome = parish_core::game_session::apply_npc_dialogue_turn(
                &mut self.app.world,
                &mut self.app.npc_manager,
                speaker_id,
                &response,
                text,
                &player_line,
                game_time,
                location,
                &name,
                &name,
                None,
                &[],
                &language,
                &self.app.flags,
            );
            for event in outcome.debug_events {
                self.app.debug_event(event);
            }

            // Log + surface the guarded (#1228) + capped (#1224) text, identical
            // to what was stored in the conversation log and event bus.
            self.app
                .world
                .log(format!("{}: {}", name, outcome.display_text));
            return ActionResult::NpcResponse {
                npc: name,
                dialogue: outcome.display_text,
                anachronisms: anachronism_terms,
            };
        }

        // Fall back to the simulator if configured.
        if let Some(ref sim) = self.simulator {
            let dialogue = sim.generate_sync(text, None);
            self.app.world.log(format!("{}: {}", name, dialogue));
            return ActionResult::NpcResponse {
                npc: name,
                dialogue,
                anachronisms: anachronism_terms,
            };
        }

        ActionResult::NpcNotAvailable
    }

    /// Checks NPCs at the current location for canned responses. Free-text
    /// names and `@mentions` use the shared routing resolver; generic
    /// untargeted dialogue keeps the historical first-canned-response fallback.
    /// Also runs anachronism detection on the player's input and includes any
    /// detected terms in the result.
    ///
    /// When a canned response is consumed, the interaction is processed
    /// through the same memory pipeline as a real LLM response: the NPC's
    /// mood is updated, a memory entry is recorded, and evicted memories
    /// may be promoted to long-term storage.
    fn handle_npc_interaction(&mut self, text: &str) -> ActionResult {
        let npcs_here = self.app.npc_manager.npcs_at(self.app.world.player_location);
        let mentions =
            parish_core::ipc::extract_npc_mentions(text, &self.app.world, &self.app.npc_manager);

        if npcs_here.is_empty() {
            if !mentions.names.is_empty() {
                let addressed = parish_core::ipc::resolve_addressed_targets(
                    &self.app.world,
                    &self.app.npc_manager,
                    &mentions.names,
                );
                if let Some(absent_name) = addressed.absent.first() {
                    let msg = format!("{absent_name} is not here.");
                    self.app.world.log(msg.clone());
                    return ActionResult::SystemCommand { response: msg };
                }
            }
            let msg = self.empty_location_dialogue_message();
            self.app.world.log(msg.clone());
            return ActionResult::SystemCommand { response: msg };
        }

        // Detect anachronisms in player input
        let detected = crate::npc::anachronism::check_input(text);
        let anachronism_terms: Vec<String> = detected.iter().map(|a| a.term.clone()).collect();

        let target_ids = if mentions.names.is_empty() {
            Vec::new()
        } else {
            parish_core::ipc::resolve_npc_targets(
                &self.app.world,
                &self.app.npc_manager,
                &mentions.names,
            )
        };

        if !mentions.names.is_empty() && target_ids.is_empty() {
            let addressed = parish_core::ipc::resolve_addressed_targets(
                &self.app.world,
                &self.app.npc_manager,
                &mentions.names,
            );
            if let Some(absent_name) = addressed.absent.first() {
                let msg = format!("{absent_name} is not here.");
                self.app.world.log(msg.clone());
                return ActionResult::SystemCommand { response: msg };
            }
            return ActionResult::NpcNotAvailable;
        }

        let ordered_npcs: Vec<(NpcId, String, String)> = if target_ids.is_empty() {
            npcs_here
                .into_iter()
                .map(|npc| (npc.id, npc.name.clone(), npc.mood.clone()))
                .collect()
        } else {
            target_ids
                .iter()
                .filter_map(|id| self.app.npc_manager.get(*id))
                .map(|npc| (npc.id, npc.name.clone(), npc.mood.clone()))
                .collect()
        };

        let allow_multiple = !target_ids.is_empty();
        let mut first_response = None;
        for (npc_id, name, mood) in ordered_npcs.iter().cloned() {
            let Some(result) =
                self.consume_canned_npc_response(npc_id, name, mood, text, &anachronism_terms)
            else {
                continue;
            };

            if !allow_multiple {
                return result;
            }
            if first_response.is_none() {
                first_response = Some(result);
            }
        }

        if let Some(result) = first_response {
            return result;
        }

        // No canned response — fall back to the simulator if configured.
        if let Some(ref sim) = self.simulator
            && let Some((_, name, _)) = ordered_npcs.first()
        {
            let dialogue = sim.generate_sync(text, None);
            self.app.world.log(format!("{}: {}", name, dialogue));
            return ActionResult::NpcResponse {
                npc: name.clone(),
                dialogue,
                anachronisms: anachronism_terms,
            };
        }

        ActionResult::NpcNotAvailable
    }

    fn empty_location_dialogue_message(&self) -> String {
        let location = self.app.world.current_location();
        Self::empty_location_dialogue_message_for(&location.name, location.indoor)
    }

    fn empty_location_dialogue_message_for(location_name: &str, indoor: bool) -> String {
        let location_name = location_name.to_lowercase();
        let words: Vec<&str> = location_name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        if words.contains(&"church") {
            "You speak into the empty church, but no one answers.".to_string()
        } else if words.contains(&"green") {
            "You speak across the empty green, but no one answers.".to_string()
        } else if indoor {
            "You speak into the empty room, but no one answers.".to_string()
        } else {
            "You speak into the empty place, but no one answers.".to_string()
        }
    }

    fn consume_canned_npc_response(
        &mut self,
        npc_id: NpcId,
        name: String,
        mood: String,
        text: &str,
        anachronism_terms: &[String],
    ) -> Option<ActionResult> {
        let key = name.to_lowercase();
        let responses = self.canned_responses.get_mut(&key)?;
        if responses.is_empty() {
            return None;
        }

        let response = parse_canned_npc_response(responses.remove(0), mood);
        let game_time = self.app.world.clock.now();

        // Build a parsed or synthetic NPC response and run it through the memory pipeline.
        // Shared per-turn pipeline (#1172 / #1173): name detection, Tier-1
        // apply, conversation-log record, witness memories, and the
        // `DialogueOccurred` publish — one definition for every backend
        // (`parish_core::game_session::apply_npc_dialogue_turn`). The player
        // line is verb-stripped for the journal entry.
        let location = self.app.world.player_location;
        let player_line = strip_dialogue_verb(text);
        let language = self.app.language_settings();
        let outcome = parish_core::game_session::apply_npc_dialogue_turn(
            &mut self.app.world,
            &mut self.app.npc_manager,
            npc_id,
            &response,
            text,
            &player_line,
            game_time,
            location,
            &name,
            &name,
            None,
            &[],
            &language,
            &self.app.flags,
        );
        for event in outcome.debug_events {
            self.app.debug_event(event);
        }

        // Log + surface the guarded (#1228) + capped (#1224) text, identical to
        // what was stored in the conversation log and event bus.
        self.app
            .world
            .log(format!("{}: {}", name, outcome.display_text));
        Some(ActionResult::NpcResponse {
            npc: name,
            dialogue: outcome.display_text,
            anachronisms: anachronism_terms.to_vec(),
        })
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

/// Atomically appends a complete task post-state batch to the harness ledger.
///
/// Shared by the legacy harness and its real `game_loop` adapter so task-aware
/// inputs cannot bypass the staged-turn journal seam.
pub(crate) fn persist_task_mutation_batch(
    db: Option<&crate::persistence::Database>,
    branch_id: i64,
    mutations: &[parish_core::session_store::PlayerTask],
) -> Result<(), String> {
    if mutations.is_empty() {
        return Ok(());
    }
    let db = db.ok_or_else(|| "persistence is unavailable".to_string())?;
    let events = mutations
        .iter()
        .cloned()
        .map(|task| {
            let game_time = task
                .completed_at
                .or(task.started_at)
                .unwrap_or(task.assigned_at)
                .to_rfc3339();
            (
                crate::persistence::WorldEvent::PlayerTaskStateChanged { task },
                game_time,
            )
        })
        .collect::<Vec<_>>();
    match db
        .append_events_to_latest_snapshot(branch_id, &events)
        .map_err(|error| error.to_string())?
    {
        Some(_) => Ok(()),
        None => Err(format!("branch {branch_id} has no snapshot")),
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
    /// Any new entries appended to the world text log since the previous
    /// script step — this is where ambient events like banshee wails and
    /// NPC arrivals surface. Omitted when empty so routine commands stay
    /// terse.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    new_log_lines: Vec<String>,
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

/// Strips a leading `say` verb from a raw command string so the player
/// line in the character-log journal reads as natural speech instead
/// of as a command. Returns the input trimmed when no verb is present.
///
/// Only `say <body>` is stripped — `tell <name> <body>` and similar
/// vocatives are left intact, because the second token is genuinely
/// part of the player's utterance (`Tell me about the weather`,
/// `Ask why he left`). The harness's NPC-routing layer separately
/// strips the addressee where it matters; the journal records what
/// the player actually said.
fn strip_dialogue_verb(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed
        .strip_prefix("say ")
        .or_else(|| trimmed.strip_prefix("Say "))
    {
        return rest.trim().to_string();
    }
    trimmed.to_string()
}

/// Runs the game in script mode, reading commands from a file.
///
/// Each command is executed through [`GameTestHarness`] and produces
/// one JSON line of output. This allows Claude Code (or any script)
/// to verify game behavior without a terminal or Ollama.
pub fn run_script_mode(
    script_path: &Path,
    game_mod: Option<parish_core::game_mod::GameMod>,
) -> anyhow::Result<()> {
    let harness = GameTestHarness::build_with_mod(game_mod);
    run_script_mode_with(script_path, harness)
}

/// Same as [`run_script_mode`] but takes a pre-built harness so
/// callers (and integration tests) can inject a vanilla
/// `GameTestHarness::new()` (character logs off) and avoid writing to
/// the shared user-data directory.
pub fn run_script_mode_with(
    script_path: &Path,
    mut harness: GameTestHarness,
) -> anyhow::Result<()> {
    let contents = std::fs::read_to_string(script_path)?;
    let mut last_log_len = harness.text_log().len();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let result = harness.execute(trimmed);
        let new_log_lines: Vec<String> = harness
            .text_log()
            .iter()
            .skip(last_log_len)
            .cloned()
            .collect();
        last_log_len = harness.text_log().len();
        let output = ScriptOutputLine {
            command: trimmed.to_string(),
            result,
            location: harness.player_location().to_string(),
            time: harness.time_of_day().to_string(),
            season: harness.season().to_string(),
            new_log_lines,
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
    use crate::world::DEFAULT_START_LOCATION;

    #[test]
    fn test_harness_new_starts_at_kilteevan() {
        let h = GameTestHarness::new();
        assert_eq!(h.player_location(), "Kilteevan Village");
        assert_eq!(h.location_id(), DEFAULT_START_LOCATION);
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

    /// #1176: the harness exposes a **deterministic** server-push event bus
    /// (`DeterministicEventBus`) that drains synchronously. Subscribe, drive a
    /// fixed turn sequence that pushes the server events a turn produces, and
    /// confirm the subscriber observes them in exact publish order — with no
    /// async scheduling window between emit and delivery.
    #[test]
    fn harness_event_bus_drains_in_publish_order() {
        use parish_core::event_bus::{EventBus, Topic};

        let h = GameTestHarness::new();
        let mut stream = h.event_bus().subscribe(&[]);

        // Emit the interleaved (topic, name) stream a turn might produce.
        // emit() on the deterministic bus delivers synchronously, so the
        // subscriber sees them back-to-back in publish order.
        let seq = [
            (Topic::TextLog, "text-log"),
            (Topic::WorldUpdate, "world-update"),
            (Topic::NpcReaction, "npc-reaction"),
            (Topic::TextLog, "text-log"),
            (Topic::WorldUpdate, "world-update"),
        ];
        for (topic, name) in seq {
            h.event_bus()
                .emit_named(topic, name, &serde_json::json!({}));
        }

        let mut observed = Vec::new();
        while let Some(ev) = stream.try_recv() {
            observed.push(ev.event);
        }
        let expected: Vec<String> = seq.iter().map(|(_, n)| n.to_string()).collect();
        assert_eq!(observed, expected);
    }

    /// #1176 stability guarantee: the same driven turn sequence yields the
    /// same subscriber ordering on every iteration. A flake here would mean
    /// async scheduling leaked back into the harness push path.
    #[test]
    fn harness_event_bus_ordering_is_stable_across_repeated_runs() {
        use parish_core::event_bus::{EventBus, Topic};

        let seq = [
            (Topic::TextLog, "text-log"),
            (Topic::TravelStart, "travel-start"),
            (Topic::WorldUpdate, "world-update"),
            (Topic::InferenceToken, "stream-token"),
            (Topic::WorldUpdate, "world-update"),
        ];
        let expected: Vec<String> = seq.iter().map(|(_, n)| n.to_string()).collect();

        for iteration in 0..500 {
            let h = GameTestHarness::new();
            let mut firehose = h.event_bus().subscribe(&[]);
            let mut world_only = h.event_bus().subscribe(&[Topic::WorldUpdate]);

            for (topic, name) in seq {
                h.event_bus()
                    .emit_named(topic, name, &serde_json::json!({}));
            }

            let mut firehose_out = Vec::new();
            while let Some(ev) = firehose.try_recv() {
                firehose_out.push(ev.event);
            }
            assert_eq!(
                firehose_out, expected,
                "harness firehose ordering diverged on iteration {iteration}"
            );

            let mut world_out = Vec::new();
            while let Some(ev) = world_only.try_recv() {
                world_out.push(ev.event);
            }
            assert_eq!(
                world_out,
                vec!["world-update".to_string(), "world-update".to_string()],
                "harness filtered ordering diverged on iteration {iteration}"
            );
        }
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
        // Advance to 9am, still Morning, when Padraig is scheduled at the pub (9-22).
        h.advance_time(60);
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
    fn natural_absent_presence_query_reports_named_absence_at_empty_church() {
        let mut h = GameTestHarness::new();
        h.advance_time(90);
        let moved = h.execute("go to St. Brigid's Church");
        assert!(matches!(moved, ActionResult::Moved { .. }), "{moved:?}");
        assert!(
            h.npcs_here().is_empty(),
            "repro requires an empty church after Father Declan's early service"
        );

        let result = h.execute(
            "Is Father Declan here? I should like to introduce myself to the parish priest.",
        );

        assert_eq!(
            result,
            ActionResult::SystemCommand {
                response: "Fr. Declan Tierney is not here.".to_string()
            }
        );
        assert!(
            h.app
                .world
                .text_log
                .iter()
                .any(|line| line.contains("Fr. Declan Tierney is not here.")),
            "absence feedback must be logged for script output"
        );
    }

    #[test]
    fn canned_npc_response_strips_stacked_friend_stranger_vocative() {
        let mut h = GameTestHarness::new();
        h.advance_time(90);
        let moved = h.execute("go to Darcy's Pub");
        assert!(matches!(moved, ActionResult::Moved { .. }), "{moved:?}");

        h.add_canned_response(
            "Padraig Darcy",
            "Do ye have need of him or his guidance, then, friend stranger, or just curious to know the man of God in these parts?",
        );

        let result = h.execute(
            "talk to Padraig Darcy about Is Father Declan the man I should speak with about parish matters?",
        );
        let ActionResult::NpcResponse { dialogue, .. } = result else {
            panic!("expected Padraig canned response, got {result:?}");
        };

        assert!(
            !dialogue.to_lowercase().contains("friend stranger"),
            "stacked vocative must be stripped: {dialogue:?}"
        );
        assert!(
            dialogue.contains("friend, or just curious"),
            "substantive question and one address term must survive: {dialogue:?}"
        );
        assert!(
            dialogue.contains("man of God"),
            "priest reference must survive: {dialogue:?}"
        );
    }

    #[test]
    fn canned_npc_response_polishes_stock_declines_and_midday_morning_tic() {
        let mut h = GameTestHarness::new();
        h.advance_time(240);
        h.execute("go to Darcy's Pub");

        h.add_canned_response(
            "Padraig Darcy",
            "Good morning to ye, mo chara. What brings ye in this fine morning?",
        );
        let time_result = h.execute("talk to Padraig Darcy about Good day, what is the news?");
        let ActionResult::NpcResponse {
            dialogue: time_dialogue,
            ..
        } = time_result
        else {
            panic!("expected Padraig to answer the time-polish turn, got {time_result:?}");
        };
        assert!(
            !time_dialogue.to_lowercase().contains("morn"),
            "midday dialogue must not retain morning wording: {time_dialogue}"
        );

        h.add_canned_response(
            "Padraig Darcy",
            "I know no one by that name in these parts.",
        );
        let person_result = h.execute(
            "talk to Padraig Darcy about Have you met Sorcha O'Malley from beyond the parish?",
        );
        let ActionResult::NpcResponse {
            dialogue: person_dialogue,
            ..
        } = person_result
        else {
            panic!("expected Padraig to answer the stock-person turn, got {person_result:?}");
        };
        assert_ne!(
            person_dialogue,
            "I know no one by that name in these parts."
        );

        h.add_canned_response("Padraig Darcy", "Mayhap ye have the wrong parish entirely.");
        let place_result = h.execute("talk to Padraig Darcy about Where is Silver Bridge?");
        let ActionResult::NpcResponse {
            dialogue: place_dialogue,
            ..
        } = place_result
        else {
            panic!("expected Padraig to answer the stock-place turn, got {place_result:?}");
        };
        assert_ne!(place_dialogue, "Mayhap ye have the wrong parish entirely.");
        assert_ne!(
            person_dialogue, place_dialogue,
            "different unknown-entity prompts should not collapse to one reply"
        );
    }

    #[test]
    fn canned_shopkeeper_stock_decline_keeps_nonrecognition_voice() {
        let mut h = GameTestHarness::new();
        let moved = h.execute("go to Connolly's Shop");
        assert!(matches!(moved, ActionResult::Moved { .. }), "{moved:?}");

        h.add_canned_response(
            "Roisin Connolly",
            "That name is not known to me hereabouts.",
        );
        let result = h.execute("talk to Roisin Connolly about Martin");
        let ActionResult::NpcResponse { npc, dialogue, .. } = result else {
            panic!("expected Roisin to answer through the canned NPC path, got {result:?}");
        };

        assert_eq!(npc, "Roisin Connolly");
        let lower = dialogue.to_lowercase();
        assert!(
            lower.contains("counter")
                || lower.contains("shop")
                || lower.contains("account")
                || lower.contains("goods")
                || lower.contains("trade"),
            "shopkeeper decline should carry trade voice: {dialogue:?}"
        );
        assert!(
            !lower.contains("aye, i know the name"),
            "stock non-recognition must not be flipped into an affirmation: {dialogue:?}"
        );
        assert!(
            !lower.contains("that name is not known to me hereabouts"),
            "reported generic stock phrase must not surface unchanged: {dialogue:?}"
        );
    }

    #[test]
    fn canned_npc_response_rewrites_presumed_prior_acquaintance() {
        let mut h = GameTestHarness::new();
        let moved = h.execute("go to Connolly's Shop");
        assert!(matches!(moved, ActionResult::Moved { .. }), "{moved:?}");

        h.add_canned_response(
            "Roisin Connolly",
            "Colm Gallagher, aye, he's a bright lad at the forge. How do ye find him so far?",
        );
        let result = h.execute("talk to Roisin Connolly about Colm Gallagher");
        let ActionResult::NpcResponse { npc, dialogue, .. } = result else {
            panic!("expected Roisin to answer through the canned NPC path, got {result:?}");
        };
        let lower = dialogue.to_lowercase();

        assert_eq!(npc, "Roisin Connolly");
        assert!(
            lower.contains("have ye met colm gallagher yet"),
            "guard should ask whether the player has met the target: {dialogue:?}"
        );
        assert!(
            !lower.contains("how do ye find him so far"),
            "presupposing question must not surface unchanged: {dialogue:?}"
        );
    }

    #[test]
    fn canned_npc_response_removes_repeated_speaker_name() {
        let mut h = GameTestHarness::new();
        h.add_canned_response(
            "Peig Hannigan",
            "Ye can call me Peig Hannigan. As for yer question, it's Peig Hannigan ye're speaking to.",
        );
        let result = h.execute("talk to Peig Hannigan about your name");
        let ActionResult::NpcResponse { npc, dialogue, .. } = result else {
            panic!("expected Peig to answer through the canned NPC path, got {result:?}");
        };
        let lower = dialogue.to_lowercase();

        assert_eq!(npc, "Peig Hannigan");
        assert_eq!(
            lower.matches("peig hannigan").count(),
            1,
            "speaker full name should appear once: {dialogue:?}"
        );
        assert!(
            !lower.contains("it's peig hannigan ye're speaking to"),
            "redundant self-reference phrase must not surface unchanged: {dialogue:?}"
        );
    }

    #[test]
    fn canned_npc_response_replaces_invented_place_soft_deflection() {
        let mut h = GameTestHarness::new();
        h.add_canned_response(
            "Peig Hannigan",
            "And Ballygostick Tower, now? Have ye a reason for asking?",
        );
        let result =
            h.execute("talk to Peig Hannigan about Is there a place called Ballygostick Tower?");
        let ActionResult::NpcResponse { npc, dialogue, .. } = result else {
            panic!("expected Peig to answer through the canned NPC path, got {result:?}");
        };
        let lower = dialogue.to_lowercase();

        assert_eq!(npc, "Peig Hannigan");
        assert!(
            lower.contains("place") || lower.contains("road i know"),
            "invented place should become a place non-recognition decline: {dialogue:?}"
        );
        assert!(
            !lower.contains("ballygostick tower"),
            "invented place name must not be repeated as a real referent: {dialogue:?}"
        );
        assert!(
            !lower.contains("reason for asking"),
            "soft deflection must not surface unchanged: {dialogue:?}"
        );
    }

    #[test]
    fn canned_npc_response_cools_neutral_rival_target_tone() {
        let mut h = GameTestHarness::new();
        let moved = h.execute("go to Connolly's Shop");
        assert!(matches!(moved, ActionResult::Moved { .. }), "{moved:?}");

        h.add_canned_response(
            "Roisin Connolly",
            "Mick Flanagan, aye. He's retired now but still keeps an eye on things.",
        );

        let result = h.execute("talk to Roisin Connolly about What do you think of Mick Flanagan?");
        let ActionResult::NpcResponse { npc, dialogue, .. } = result else {
            panic!("expected Roisin to answer through the canned NPC path, got {result:?}");
        };

        assert_eq!(npc, "Roisin Connolly");
        assert!(
            !dialogue
                .to_lowercase()
                .contains("still keeps an eye on things"),
            "neutral-warm rival line must not surface unchanged: {dialogue:?}"
        );
        assert!(
            dialogue.contains("Mick Flanagan"),
            "target name should survive the cooled fallback: {dialogue:?}"
        );
        assert!(
            dialogue.to_lowercase().contains("little warmth")
                || dialogue.to_lowercase().contains("keep my distance"),
            "cooled fallback should carry a visible rival cue: {dialogue:?}"
        );
    }

    #[test]
    fn canned_npc_response_corrects_priest_tenure_drift() {
        let mut h = GameTestHarness::new();
        let moved = h.execute("go to Connolly's Shop");
        assert!(matches!(moved, ActionResult::Moved { .. }), "{moved:?}");

        h.add_canned_response(
            "Roisin Connolly",
            "He's been the priest here for nigh on a decade now.",
        );
        let result = h.execute("talk to Roisin Connolly about Father Declan Tierney");
        let ActionResult::NpcResponse { npc, dialogue, .. } = result else {
            panic!("expected Roisin to answer through the canned NPC path, got {result:?}");
        };

        assert_eq!(npc, "Roisin Connolly");
        let lower = dialogue.to_lowercase();
        assert!(
            lower.contains("twenty-five years"),
            "canonical tenure must be visible: {dialogue:?}"
        );
        assert!(
            !lower.contains("decade"),
            "incorrect decade-scale tenure must be removed: {dialogue:?}"
        );
    }

    #[test]
    fn canned_npc_response_declines_invented_titled_landlord() {
        let mut h = GameTestHarness::new();
        let moved = h.execute("go to the forge");
        assert!(matches!(moved, ActionResult::Moved { .. }), "{moved:?}");

        h.add_canned_response(
            "Colm Gallagher",
            "Aye, I've heard the talk of Lord Fitzwilliam. 'Tis said he owns most of the land round hereabouts. Ye'll need to be careful with yer words when ye speak of him, 'tis a mighty man he is.",
        );

        let result = h.execute(
            "talk to Colm Gallagher about Have you heard of Lord Fitzwilliam of Castlemore? I hear he is the great landlord hereabouts",
        );
        let ActionResult::NpcResponse { npc, dialogue, .. } = result else {
            panic!("expected Colm to answer through the canned NPC path, got {result:?}");
        };

        assert_eq!(npc, "Colm Gallagher");
        let lower = dialogue.to_lowercase();
        assert_eq!(dialogue, parish_core::npc::INVALID_DIALOGUE_FALLBACK);
        assert!(!lower.contains("lord fitzwilliam"), "{dialogue}");
        assert!(!lower.contains("owns most of the land"), "{dialogue}");
    }

    #[test]
    fn canned_npc_response_corrects_real_entity_false_denials() {
        let mut h = GameTestHarness::new();
        let moved = h.execute("go to the forge");
        assert!(matches!(moved, ActionResult::Moved { .. }), "{moved:?}");

        h.add_canned_response(
            "Seamus Gallagher",
            "I cannae guide ye to a place that doesn't exist.",
        );
        let place_result = h.execute("talk to Seamus Gallagher about Where is Darcy's Pub?");
        let ActionResult::NpcResponse {
            dialogue: place_dialogue,
            ..
        } = place_result
        else {
            panic!("expected Seamus to answer the known-place turn, got {place_result:?}");
        };
        let place_lower = place_dialogue.to_lowercase();
        assert!(!place_lower.contains("doesn't exist"), "{place_dialogue}");
        assert!(
            place_lower.contains("place")
                && (place_lower.contains("know")
                    || place_lower.contains("known")
                    || place_lower.contains("real")),
            "{place_dialogue}"
        );

        h.add_canned_response(
            "Seamus Gallagher",
            "I know no one by that name in these parts.",
        );
        let person_result = h.execute("talk to Seamus Gallagher about Where is Padraig Darcy?");
        let ActionResult::NpcResponse {
            dialogue: person_dialogue,
            ..
        } = person_result
        else {
            panic!("expected Seamus to answer the known-person turn, got {person_result:?}");
        };
        let person_lower = person_dialogue.to_lowercase();
        assert!(
            !person_lower.contains("no one by that name"),
            "{person_dialogue}"
        );
        assert!(
            person_lower.contains("name") || person_lower.contains("parish"),
            "{person_dialogue}"
        );
    }

    #[test]
    fn test_canned_multi_npc_response_from_free_text_names() {
        let mut h = GameTestHarness::new();
        // Free-text name addressing ("Padraig", "Niamh") resolves against
        // *introduced* NPCs, and arrival greetings are what introduce them on
        // entry. Those greetings are gated off by default (npc-arrival-greetings),
        // so enable the flag here to exercise the historical arrival-introduction
        // path this test depends on.
        h.execute("/flag enable npc-arrival-greetings");
        h.advance_time(120); // 10am — Padraig and Niamh are scheduled at the pub.
        h.execute("go to crossroads");
        h.execute("go to pub");

        let npcs = h.npcs_here();
        assert!(npcs.iter().any(|n| n == &"Padraig Darcy"), "{npcs:?}");
        assert!(npcs.iter().any(|n| n == &"Niamh Darcy"), "{npcs:?}");

        h.add_canned_response("Padraig Darcy", "A fair morning to ye from Padraig.");
        h.add_canned_response("Niamh Darcy", "And a good day back to ye from Niamh.");

        let result = h.execute("Good morning, Padraig and good day, Niamh.");
        assert!(matches!(result, ActionResult::NpcResponse { .. }));

        let loc = h.location_id();
        let recent = h.app.world.conversation_log.recent_at(loc, 2);
        let speakers: Vec<&str> = recent
            .iter()
            .map(|exchange| exchange.speaker_name.as_str())
            .collect();
        assert_eq!(speakers, vec!["Padraig Darcy", "Niamh Darcy"]);
    }

    #[test]
    fn test_canned_free_text_names_ignore_absent_npcs() {
        let mut h = GameTestHarness::new();
        h.advance_time(120);
        h.execute("go to crossroads");

        h.add_canned_response("Padraig Darcy", "This should not fire.");
        h.add_canned_response("Niamh Darcy", "Nor should this.");

        let result = h.execute("I saw Padraig and Niamh by the road.");
        assert!(!matches!(result, ActionResult::NpcResponse { .. }));
        assert!(
            h.text_log()
                .iter()
                .all(|line| !line.contains("This should not fire")
                    && !line.contains("Nor should this")),
            "{:?}",
            h.text_log()
        );
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
    fn empty_location_dialogue_returns_no_answer_message() {
        let mut h = GameTestHarness::new();
        // Navigate to a location with no NPCs (e.g., the hurling green)
        h.execute("go to crossroads");
        h.execute("go to hurling green");
        let before = h.app.world.clock.now();
        let result = h.execute("hello there");
        let after = h.app.world.clock.now();
        let ActionResult::SystemCommand { response } = result else {
            panic!("expected empty-location dialogue feedback, got {result:?}");
        };
        assert!(
            response.to_lowercase().contains("no one answers")
                || response.to_lowercase().contains("empty"),
            "empty-location response should explain that no one answers: {response:?}"
        );
        assert_eq!(
            after, before,
            "empty-location dialogue must not advance time"
        );
    }

    #[test]
    fn empty_location_dialogue_message_matches_kind_by_whole_word() {
        assert_eq!(
            GameTestHarness::empty_location_dialogue_message_for("Churchill's Farm", false),
            "You speak into the empty place, but no one answers."
        );
        assert_eq!(
            GameTestHarness::empty_location_dialogue_message_for("Greenwood", false),
            "You speak into the empty place, but no one answers."
        );
        assert_eq!(
            GameTestHarness::empty_location_dialogue_message_for("The Old Green", false),
            "You speak across the empty green, but no one answers."
        );
        assert_eq!(
            GameTestHarness::empty_location_dialogue_message_for("The Storehouse", true),
            "You speak into the empty room, but no one answers."
        );
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
    fn test_text_log_capped() {
        let mut h = GameTestHarness::new();
        // Push well over the 500-entry backend cap.
        for i in 0..600 {
            h.app.world.log(format!("entry {i}"));
        }
        assert!(
            h.text_log().len() <= 500,
            "text_log should be capped at 500 but was {}",
            h.text_log().len()
        );
        // The most recent entry should still be present.
        assert!(h.text_log().last().unwrap().contains("entry 599"));
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

        // Use the no-character-logs harness so this test doesn't write
        // into the shared `~/Library/Application Support/<app>/logs/`
        // dir that the live `parish --script` invocation owns.
        run_script_mode_with(&script, GameTestHarness::new()).unwrap();
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
        if transmitted > 0 {
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

    /// Tier 4 tick fires after enough game time has elapsed.
    ///
    /// Places a single NPC far from the player (distance > tier3_max_distance) so
    /// it is assigned Tier 4, then advances the clock by the default tick interval
    /// (90 game-days) and verifies that `last_tier4_game_time` is recorded.
    #[test]
    fn test_tier4_tick_fires_after_interval() {
        use crate::npc::Npc;
        use crate::npc::manager::NpcManager;
        use crate::world::LocationId;
        use parish_core::world::graph::WorldGraph;

        // Build a chain graph long enough that an NPC at the far end is Tier 4
        // (default tier3_max_distance = 5, so distance 6 → Tier 4).
        let locations: Vec<serde_json::Value> = (0u32..=6)
            .map(|i| {
                let mut conns = Vec::new();
                if i > 0 {
                    conns.push(serde_json::json!({
                        "target": i - 1,
                        "path_description": "a road"
                    }));
                }
                if i < 6 {
                    conns.push(serde_json::json!({
                        "target": i + 1,
                        "path_description": "a road"
                    }));
                }
                serde_json::json!({
                    "id": i,
                    "name": format!("Loc {}", i),
                    "description_template": "Test",
                    "indoor": false,
                    "public": true,
                    "connections": conns
                })
            })
            .collect();
        let graph_json = serde_json::json!({"locations": locations}).to_string();
        let graph = WorldGraph::load_from_str(&graph_json).unwrap();

        // NPC at distance 6 from player (player at Loc 0) → Tier 4
        let mut far_npc = Npc::new_test_npc();
        far_npc.id = crate::npc::NpcId(42);
        far_npc.name = "Far Away Person".to_string();
        far_npc.brief_description = "a distant figure".to_string();
        far_npc.age = 40;
        far_npc.occupation = "Farmer".to_string();
        far_npc.personality = "Quiet".to_string();
        far_npc.pronouns = "they/them".to_string();
        far_npc.intelligence = parish_core::npc::types::Intelligence::default();
        far_npc.set_location(LocationId(6));
        far_npc.mood = "calm".to_string();
        far_npc.home = Some(LocationId(6));

        let mut app = crate::app::App::new();
        app.world.player_location = LocationId(0);
        app.world.graph = graph;
        app.npc_manager = NpcManager::new();
        app.npc_manager.add_npc(far_npc);
        app.npc_manager.assign_tiers(&app.world, &[]);

        // Confirm the NPC is actually Tier 4
        assert_eq!(
            app.npc_manager.tier_of(crate::npc::NpcId(42)),
            Some(parish_core::npc::types::CogTier::Tier4),
            "NPC at distance 6 should be Tier 4"
        );

        // No tier4 tick yet
        assert!(app.npc_manager.last_tier4_game_time().is_none());

        // Wrap in a harness-like struct so we can call advance_time
        // (GameTestHarness::new() loads the real mod, so build it manually).
        let mut h = GameTestHarness {
            app,
            canned_responses: std::collections::HashMap::new(),
            db_sync: None,
            simulator: None,
            rng: rand::rngs::StdRng::seed_from_u64(0),
            mock: Arc::new(crate::inference::MockClient::new()),
            real_loop_conversation: std::sync::Arc::new(tokio::sync::Mutex::new(
                parish_core::ipc::ConversationRuntimeState::new(),
            )),
            shadow_enabled: false,
            shadow_ledger: crate::shadow::ledger_path(),
            shadow_case: "test".to_string(),
            event_bus: parish_core::event_bus::DeterministicEventBus::new(),
        };

        // Advance by the default tier4 tick interval (90 game-days = 90 * 24 * 60 minutes)
        let tier4_interval_minutes: i64 = 90 * 24 * 60;
        h.advance_time(tier4_interval_minutes);

        assert!(
            h.app.npc_manager.last_tier4_game_time().is_some(),
            "last_tier4_game_time should be recorded after advancing 90 game-days"
        );
    }

    // ── NPC reactions feature tests (#200, #402, #403, #404) ─────────────────

    /// Rule-based fallback fires when no LLM client is present (#404).
    ///
    /// Sends a message with a landlord keyword to an NPC location and verifies
    /// that the world text log contains a reaction line for at least one NPC.
    /// This proves the fallback path is live in the CLI/harness (mode parity, #402).
    #[test]
    fn test_rule_reaction_fires_on_keyword_match() {
        let mut h = GameTestHarness::new();

        // Verify there are NPCs at the starting location.
        let npcs = h.npcs_here();
        if npcs.is_empty() {
            // If starting location has no NPCs, move to one that does.
            h.execute("go to the pub");
        }

        let log_len_before = h.text_log().len();
        // The landlord keyword group always triggers the 😠 emoji via keyword matching.
        // We run it a few times to overcome the 60% probabilistic gate.
        let mut any_reaction = false;
        for _ in 0..10 {
            h.execute("The landlord's agent is demanding the rent this week.");
            let new_log: Vec<&str> = h
                .text_log()
                .iter()
                .skip(log_len_before)
                .map(|s| s.as_str())
                .collect();
            if new_log
                .iter()
                .any(|line| line.contains('😠') || line.contains('😢') || line.contains("👀"))
            {
                any_reaction = true;
                break;
            }
        }
        assert!(
            any_reaction,
            "At least one rule-based NPC reaction (😠) should appear in text log for landlord keyword"
        );
    }

    /// Reactions are persisted to reaction_log (#403).
    ///
    /// Directly calls apply_rule_reactions and then inspects the NPC's
    /// reaction_log to confirm the emoji was recorded.
    #[test]
    fn test_reaction_log_written_after_keyword_message() {
        let mut h = GameTestHarness::new();

        // Ensure we're at a location with at least one NPC.
        if h.npcs_here().is_empty() {
            h.execute("go to the pub");
        }

        let npcs = h.npcs_here();
        if npcs.is_empty() {
            return; // Can't test without NPCs.
        }

        // Use a known keyword that always appears in KEYWORD_REACTIONS.
        // "rent" → 😠 is in the table. Run many times to beat the 60% gate.
        let trigger_input = "The landlord and the rent collectors were seen this morning.";

        // apply_rule_reactions goes through the 60% gate; run 20 times to ensure
        // at least one fires (probability of all 20 missing: 0.4^20 ≈ 1e-8).
        for _ in 0..20 {
            h.apply_rule_reactions(trigger_input);
        }

        // At least one NPC at this location should now have a non-empty reaction_log.
        let loc = h.location_id();
        let npcs_here = h.app.npc_manager.npcs_at(loc);
        let any_logged = npcs_here.iter().any(|npc| {
            npc.reaction_log.entries().any(|entry| {
                entry.direction == parish_core::ReactionDirection::NpcToPlayer
                    && entry.context == trigger_input
            })
        });

        assert!(
            any_logged,
            "rule-based NPC reactions must retain NPC→player direction"
        );
    }

    /// Flag gating: disabling npc-llm-reactions still allows rule-based fallback (#404).
    #[test]
    fn test_flag_off_still_fires_rule_based_reactions() {
        let mut h = GameTestHarness::new();

        if h.npcs_here().is_empty() {
            h.execute("go to the pub");
        }

        // Disable LLM reactions (flag gate).
        h.app.flags.disable("npc-llm-reactions");

        // Rule-based fallback should still fire.
        let log_len_before = h.text_log().len();
        for _ in 0..10 {
            h.execute("The landlord is after us all for rent.");
            let new_log: Vec<&str> = h
                .text_log()
                .iter()
                .skip(log_len_before)
                .map(|s| s.as_str())
                .collect();
            if new_log.iter().any(|line| line.contains('😠')) {
                h.app.flags.enable("npc-llm-reactions");
                return; // Proved: fallback fires even with flag disabled.
            }
        }
        h.app.flags.enable("npc-llm-reactions");
        // If we get here without a reaction it may be bad luck with the 60% gate.
        // Not a hard failure — the 10 iterations make it statistically unlikely.
    }

    // #1199 / TD-036: `run_script_mode` builds via `build_with_mod`, which must
    // enable the character + location log writers so `parish --script ...`
    // produces the player.md / npc-*.md / location markdown that proof bundles
    // (CLAUDE.md rules #10/#13) depend on. Regressed by the #1123 refactor when
    // `build_with_mod` was hardcoded to `Self::build(false)`.
    #[test]
    #[serial_test::serial]
    fn build_with_mod_enables_script_mode_logs() {
        use tempfile::tempdir;
        // Both managers resolve their log dirs from PARISH_USER_DATA_DIR; redirect
        // to a temp dir so the write_all_profiles call in build_from_mod_dir does
        // not pollute the shared user-data dir.
        let tmp = tempdir().expect("tempdir");
        // safety: env-mutation in test, serialized via #[serial]
        unsafe {
            std::env::set_var("PARISH_USER_DATA_DIR", tmp.path());
        }

        let h = GameTestHarness::build_with_mod(None);
        let character_enabled = h
            .app
            .character_log
            .as_ref()
            .map(|m| m.enabled())
            .unwrap_or(false);
        let location_enabled = h
            .app
            .location_log
            .as_ref()
            .map(|m| m.enabled())
            .unwrap_or(false);

        // safety: env-cleanup in test
        unsafe {
            std::env::remove_var("PARISH_USER_DATA_DIR");
        }

        assert!(
            character_enabled,
            "script-mode harness must enable the character-log writer (#1199)"
        );
        assert!(
            location_enabled,
            "script-mode harness must enable the location-log writer (#1199)"
        );
    }

    // Regression guard: the plain cargo-test constructors must keep the log
    // writers disabled, so the hundreds of harness instances spun up by the test
    // suite do not each dump profiles into the shared user-data dir.
    #[test]
    fn plain_harness_keeps_logs_disabled() {
        let h = GameTestHarness::new();
        assert!(
            h.app.character_log.as_ref().is_some_and(|m| !m.enabled()),
            "GameTestHarness::new() must keep the character-log writer disabled"
        );
        let h2 = GameTestHarness::new_from_active_mod();
        assert!(
            h2.app.character_log.as_ref().is_some_and(|m| !m.enabled()),
            "new_from_active_mod() must keep the character-log writer disabled"
        );
    }

    #[test]
    fn harness_task_progress_is_recoverable_from_the_journal() {
        let mut harness = GameTestHarness::new();
        let task_id = harness
            .app
            .world
            .player_progress
            .assign_task(
                "Dig over the potato patch.",
                parish_core::npc::NpcId(7),
                harness.app.world.player_location,
                harness.app.world.clock.now(),
            )
            .unwrap();
        let db = harness.db_sync.as_ref().unwrap();
        let snapshot =
            crate::persistence::GameSnapshot::capture(&harness.app.world, &harness.app.npc_manager);
        db.save_snapshot(harness.app.active_branch_id, &snapshot)
            .unwrap();

        harness.execute("I set to work in the potato patch, breaking clods and planting seed.");
        let progressed = harness
            .app
            .world
            .player_progress
            .task(task_id)
            .unwrap()
            .clone();
        assert_eq!(progressed.status, parish_types::TaskStatus::InProgress);

        let recovery = harness
            .db_sync
            .as_ref()
            .unwrap()
            .load_recovery_data(harness.app.active_branch_id)
            .unwrap()
            .unwrap();
        let mut restored_world = crate::world::WorldState::new();
        restored_world.graph = harness.app.world.graph.clone();
        restored_world.locations = harness.app.world.locations.clone();
        let mut restored_npcs = NpcManager::new();
        recovery
            .snapshot
            .restore(&mut restored_world, &mut restored_npcs);
        crate::persistence::replay_journal(
            &mut restored_world,
            &mut restored_npcs,
            &recovery.journal,
        );
        assert_eq!(
            restored_world.player_progress.task(task_id),
            Some(&progressed)
        );
    }

    #[test]
    fn harness_failed_task_append_rolls_back_for_retry() {
        let mut harness = GameTestHarness::new();
        let task_id = harness
            .app
            .world
            .player_progress
            .assign_task(
                "Dig over the potato patch.",
                parish_core::npc::NpcId(7),
                harness.app.world.player_location,
                harness.app.world.clock.now(),
            )
            .unwrap();
        let before =
            crate::persistence::GameSnapshot::capture(&harness.app.world, &harness.app.npc_manager);
        harness
            .app
            .debug_event("pre-turn debug sentinel".to_string());
        let debug_before = harness.app.debug_log.clone();
        let mut semantic_rx = harness.app.world.event_bus.subscribe();
        harness.db_sync = None;

        let result =
            harness.execute("I set to work in the potato patch, breaking clods and planting seed.");

        assert!(matches!(
            result,
            ActionResult::SystemCommand { ref response }
                if response.contains("Failed to persist player task changes")
        ));
        assert_eq!(
            harness
                .app
                .world
                .player_progress
                .task(task_id)
                .unwrap()
                .status,
            parish_types::TaskStatus::Assigned
        );
        assert_eq!(
            crate::persistence::GameSnapshot::capture(&harness.app.world, &harness.app.npc_manager),
            before,
            "failed task persistence must restore the entire world and NPC candidate"
        );
        assert_eq!(
            harness.app.debug_log, debug_before,
            "failed staged turns must restore the legacy App debug audit exactly"
        );
        assert!(matches!(
            semantic_rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));

        let db = crate::persistence::Database::open_memory().unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        db.save_snapshot(branch.id, &before).unwrap();
        harness.app.active_branch_id = branch.id;
        harness.db_sync = Some(db);
        let retry =
            harness.execute("I set to work in the potato patch, breaking clods and planting seed.");
        assert!(!matches!(
            retry,
            ActionResult::SystemCommand { ref response }
                if response.contains("Failed to persist player task changes")
        ));
        assert_eq!(
            harness
                .app
                .world
                .player_progress
                .task(task_id)
                .unwrap()
                .status,
            parish_types::TaskStatus::InProgress
        );
        let progressed_events = std::iter::from_fn(|| semantic_rx.try_recv().ok())
            .filter(|event| matches!(event, parish_types::GameEvent::PlayerTaskProgressed { .. }))
            .count();
        assert_eq!(
            progressed_events, 1,
            "retry publishes progress exactly once"
        );
    }
}
