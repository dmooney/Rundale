//! World state and location graph for the Parish game engine.

pub mod description;
pub mod encounter;
pub mod geo;
pub mod graph;
pub mod movement;
pub mod session;
pub mod transport;
pub mod wayfarers;
pub mod weather;
pub mod weather_travel;

/// Re-export time types from parish-types for cross-crate convenience.
pub mod time {
    pub use parish_types::time::*;
}

/// Re-export event types from parish-types for cross-crate convenience.
pub mod events {
    pub use parish_types::events::*;
}

pub use parish_types::{DEFAULT_START_LOCATION, Location, LocationId, Weather};

use std::collections::{HashMap, HashSet};
use std::path::Path;

use parish_types::{
    ConversationLog, EventBus, GameClock, GossipNetwork, ParishError, PlayerProgress,
};

use graph::{LocationData, WorldGraph};
use weather::WeatherEngine;

/// Maximum number of entries kept in the backend text log, matching the
/// frontend cap (`MAX_TEXT_LOG_SIZE` in `parish/apps/ui/src/stores/game.ts`).
const MAX_TEXT_LOG: usize = 500;

/// Central game state container.
///
/// Holds the game clock, player position, the world graph, weather,
/// and the scrollback text log displayed in the UI.
#[derive(Clone)]
pub struct WorldState {
    /// The game clock mapping real time to game time.
    pub clock: GameClock,
    /// The player's current location.
    pub player_location: LocationId,
    /// All locations in the world, keyed by id (legacy, used by NPC context).
    pub locations: HashMap<LocationId, Location>,
    /// The world graph with full location data and connections.
    pub graph: WorldGraph,
    /// Current weather conditions affecting palette and descriptions.
    pub weather: Weather,
    /// Dynamic weather state machine that transitions over time.
    pub weather_engine: WeatherEngine,
    /// Scrollback text log displayed in the main text panel.
    pub text_log: Vec<String>,
    /// Cross-tier event bus for publishing and subscribing to game events.
    pub event_bus: EventBus,
    /// Set of location IDs the player has visited (for fog-of-war map).
    pub visited_locations: HashSet<LocationId>,
    /// First-visit order, parallel to `visited_locations`. Each id appears
    /// at most once, in the order [`mark_visited`] first inserted it. Used
    /// by `character_log`'s player-profile renderer to list visited
    /// locations in playthrough order rather than by numeric id (#1130).
    pub visited_order: Vec<LocationId>,
    /// Edge traversal counts for "worn path" footprints on the map.
    ///
    /// Keys are canonically ordered `(min_id, max_id)` pairs. The count
    /// increments each time the player walks along that edge.
    pub edge_traversals: HashMap<(LocationId, LocationId), u32>,
    /// Gossip propagation network tracking information spread among NPCs.
    pub gossip_network: GossipNetwork,
    /// Recent conversation exchanges for scene awareness and NPC memory.
    pub conversation_log: ConversationLog,
    /// The player's name, learned from dialogue (e.g. "My name is Ciaran").
    /// `None` until the player introduces themselves.
    pub player_name: Option<String>,
    /// Durable task assignments and progression for the player.
    pub player_progress: PlayerProgress,
    /// Monotonically increasing counter incremented once per background tick.
    ///
    /// Used by `handle_game_input` to detect TOCTOU races: the generation is
    /// captured before the world lock is released for LLM inference, then
    /// compared after the lock is re-acquired.  A mismatch means the world
    /// changed (NPCs moved, clock advanced, weather shifted) while the
    /// intent was being parsed.  See issue #283.
    pub tick_generation: u64,
    /// Mod-authored terms that must never survive canonical dialogue validation.
    /// Loaded once with the world so every runtime and staged clone uses the
    /// same setting-specific safety contract.
    pub dialogue_anachronisms: Vec<parish_types::AnachronismEntry>,
    /// Mod-authored prompt framing for the same canonical term set.
    pub dialogue_anachronism_alert_prefix: String,
    pub dialogue_anachronism_alert_suffix: String,
    /// Last canonical `/session` beat, date/location scoped when consumed.
    pub active_session: Option<session::ActiveSessionFact>,
}

impl WorldState {
    /// Clones canonical state for a pending player turn while isolating all
    /// semantic events until the turn's durable commit succeeds.
    pub fn clone_for_staged_turn(&self) -> Self {
        let mut staged = self.clone();
        staged.event_bus = EventBus::new();
        staged
    }

    /// Creates a new world state with a single test location ("The Crossroads").
    ///
    /// The game clock starts at 8:00 AM on March 20, 1820 (spring morning).
    pub fn new() -> Self {
        use chrono::{TimeZone, Utc};

        let crossroads_id = LocationId(1);
        let crossroads = Location {
            id: crossroads_id,
            name: "The Crossroads".to_string(),
            description: "A quiet crossroads where four narrow roads meet. \
                A weathered stone wall lines the eastern side, half-hidden \
                by brambles. To the north, smoke rises from a cluster of \
                cottages. The air smells of turf and wet grass."
                .to_string(),
            indoor: false,
            public: true,
            lat: 53.618,
            lon: -8.095,
        };

        let mut locations = HashMap::new();
        locations.insert(crossroads_id, crossroads);

        let clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap());

        Self::init(clock, crossroads_id, locations, WorldGraph::new())
    }

    fn init(
        clock: GameClock,
        player_location: LocationId,
        locations: HashMap<LocationId, Location>,
        graph: WorldGraph,
    ) -> Self {
        let weather_engine = WeatherEngine::new(Weather::Clear, clock.now());
        Self {
            clock,
            player_location,
            locations,
            graph,
            weather: Weather::Clear,
            weather_engine,
            text_log: Vec::new(),
            event_bus: EventBus::new(),
            visited_locations: HashSet::from([player_location]),
            visited_order: vec![player_location],
            edge_traversals: HashMap::new(),
            gossip_network: GossipNetwork::new(),
            conversation_log: ConversationLog::new(),
            player_name: None,
            player_progress: PlayerProgress::default(),
            tick_generation: 0,
            dialogue_anachronisms: Vec::new(),
            dialogue_anachronism_alert_prefix: String::new(),
            dialogue_anachronism_alert_suffix: String::new(),
            active_session: None,
        }
    }

    /// Creates a world state from a parish data file.
    ///
    /// Loads the world graph from JSON and sets the player at the
    /// specified starting location. Also populates the legacy `locations`
    /// map for backward compatibility with NPC context building.
    pub fn from_parish_file(path: &Path, start_location: LocationId) -> Result<Self, ParishError> {
        use chrono::{TimeZone, Utc};

        let graph = WorldGraph::load_from_file(path)?;
        let locations = graph_to_legacy_locations(&graph);
        let clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap());

        Ok(Self::init(clock, start_location, locations, graph))
    }

    /// Creates a world state from mod parameters.
    ///
    /// Equivalent to `from_parish_file` but also sets the start date from an
    /// RFC 3339 string. Used by `parish-core`'s mod loader so that `parish-world`
    /// does not need to depend on `GameMod` directly.
    pub fn from_mod_params(
        world_path: &Path,
        start_location: LocationId,
        start_date_rfc3339: &str,
    ) -> Result<Self, ParishError> {
        let graph = WorldGraph::load_from_file(world_path)?;
        let locations = graph_to_legacy_locations(&graph);

        let start_dt = chrono::DateTime::parse_from_rfc3339(start_date_rfc3339)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|e| {
                tracing::warn!(
                    start_date = start_date_rfc3339,
                    error = %e,
                    "Failed to parse mod start_date, falling back to current time"
                );
                chrono::Utc::now()
            });

        let clock = GameClock::new(start_dt);

        Ok(Self::init(clock, start_location, locations, graph))
    }

    /// Marks a location as visited for the fog-of-war map.
    ///
    /// Idempotent: a second call with the same id is a no-op for both
    /// the set and the first-visit-order vector (#1130).
    pub fn mark_visited(&mut self, id: LocationId) {
        if self.visited_locations.insert(id) {
            self.visited_order.push(id);
        }
    }

    /// Records a traversal along a path of locations, incrementing edge counts.
    ///
    /// Edges are stored in canonical order (smaller ID first) so that
    /// A→B and B→A are the same edge.
    pub fn record_path_traversal(&mut self, path: &[LocationId]) {
        for window in path.windows(2) {
            let (a, b) = if window[0] < window[1] {
                (window[0], window[1])
            } else {
                (window[1], window[0])
            };
            *self.edge_traversals.entry((a, b)).or_insert(0) += 1;
        }
    }

    /// Returns a reference to the player's current location.
    ///
    /// # Panics
    ///
    /// Panics if the player's location id is not in the locations map.
    pub fn current_location(&self) -> &Location {
        self.locations
            .get(&self.player_location)
            .expect("player location must exist in world")
    }

    /// Returns the current location's data from the world graph, if loaded.
    pub fn current_location_data(&self) -> Option<&LocationData> {
        self.graph.get(self.player_location)
    }

    /// Appends a line to the text log, evicting the oldest entries when the
    /// log exceeds [`MAX_TEXT_LOG`].
    pub fn log(&mut self, text: String) {
        self.text_log.push(text);
        if self.text_log.len() > MAX_TEXT_LOG {
            let excess = self.text_log.len() - MAX_TEXT_LOG;
            self.text_log.drain(..excess);
        }
    }

    /// Increments the tick generation counter.
    ///
    /// Called once per background tick cycle.  Wraps on overflow (a game
    /// session is not expected to run for 2^64 ticks).
    pub fn increment_tick_generation(&mut self) {
        self.tick_generation = self.tick_generation.wrapping_add(1);
    }

    /// Advances the weather engine for the given check time, and — on a
    /// transition — updates [`Self::weather`] and publishes a
    /// [`GameEvent::WeatherChanged`] on the world event bus. Returns the new
    /// weather if it changed, else `None`.
    ///
    /// This is the single source of truth for "tick the weather and announce
    /// it" that every runtime loop (server, Tauri, headless, and the script
    /// harness) shares. Inlining the `weather_engine.tick` + publish pair at
    /// each call site is how the harness silently drifted into ticking weather
    /// without emitting the event (#1156 follow-up; tracked in #1159).
    pub fn tick_weather_at(
        &mut self,
        check_time: chrono::DateTime<chrono::Utc>,
        rng: &mut impl rand::Rng,
    ) -> Option<Weather> {
        let season = self.clock.season();
        let new_weather = self.weather_engine.tick(check_time, season, rng)?;
        self.weather = new_weather;
        self.event_bus
            .publish(parish_types::events::GameEvent::WeatherChanged {
                new_weather: new_weather.to_string(),
                timestamp: self.clock.now(),
            });
        Some(new_weather)
    }

    /// Convenience wrapper over [`Self::tick_weather_at`] that checks at the
    /// current clock time. Used by the real-time loops, which tick once per
    /// timer interval.
    pub fn tick_weather(&mut self, rng: &mut impl rand::Rng) -> Option<Weather> {
        let now = self.clock.now();
        self.tick_weather_at(now, rng)
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the legacy `locations` map from a `WorldGraph` for backward
/// compatibility with NPC context building and UI snapshots.
fn graph_to_legacy_locations(graph: &WorldGraph) -> HashMap<LocationId, Location> {
    let mut locations = HashMap::new();
    for loc_id in graph.location_ids() {
        if let Some(data) = graph.get(loc_id) {
            locations.insert(
                loc_id,
                Location {
                    id: loc_id,
                    name: data.name.clone(),
                    description: data.description_template.clone(),
                    indoor: data.indoor,
                    public: data.public,
                    lat: data.lat,
                    lon: data.lon,
                },
            );
        }
    }
    locations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_crossroads() {
        let world = WorldState::new();
        assert_eq!(world.player_location, LocationId(1));
        assert_eq!(world.current_location().name, "The Crossroads");
    }

    #[test]
    fn tick_weather_publishes_on_transition() {
        use parish_types::events::GameEvent;
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        // The shared `tick_weather` helper must do both halves of the job every
        // runtime used to inline: update `self.weather` AND publish a
        // `WeatherChanged` event. (#1156 follow-up — the script harness used to
        // tick weather without emitting; #1159.)
        let mut transitioned = false;
        for seed in 0..500u64 {
            let mut world = WorldState::new();
            // Arm the engine on Overcast (which has transitions in both
            // directions) and step the clock past the min-duration gate.
            world
                .weather_engine
                .force(Weather::Overcast, world.clock.now());
            world.weather = Weather::Overcast;
            world.clock.advance(3 * 60);

            let mut rx = world.event_bus.subscribe();
            let mut rng = StdRng::seed_from_u64(seed);

            if let Some(new_weather) = world.tick_weather(&mut rng) {
                // State updated to the returned weather.
                assert_eq!(world.weather, new_weather);
                // …and exactly that change was announced on the bus.
                let evt = rx.try_recv().expect("WeatherChanged should be published");
                match evt {
                    GameEvent::WeatherChanged { new_weather: w, .. } => {
                        assert_eq!(w, new_weather.to_string());
                    }
                    other => panic!("expected WeatherChanged, got {other:?}"),
                }
                transitioned = true;
                break;
            }
        }
        assert!(
            transitioned,
            "no seed in 0..500 produced a weather transition to exercise tick_weather"
        );
    }

    #[test]
    fn tick_weather_silent_when_no_transition() {
        // No transition (engine just armed, clock not advanced) ⇒ no event,
        // weather unchanged, returns None.
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let mut world = WorldState::new();
        let before = world.weather;
        let mut rx = world.event_bus.subscribe();
        let mut rng = StdRng::seed_from_u64(0);

        assert_eq!(world.tick_weather(&mut rng), None);
        assert_eq!(world.weather, before);
        assert!(rx.try_recv().is_err(), "no event should be published");
    }

    #[test]
    fn new_initial_collections_are_fresh() {
        let world = WorldState::new();
        assert!(world.text_log.is_empty());
        assert!(world.edge_traversals.is_empty());
        // Starting location is pre-marked as visited.
        assert!(world.visited_locations.contains(&LocationId(1)));
        assert_eq!(world.visited_locations.len(), 1);
        assert!(world.player_name.is_none());
        assert!(world.player_progress.is_empty());
    }

    #[test]
    fn new_default_weather_is_clear() {
        let world = WorldState::new();
        assert_eq!(world.weather, Weather::Clear);
    }

    #[test]
    fn default_matches_new() {
        let a = WorldState::default();
        let b = WorldState::new();
        assert_eq!(a.player_location, b.player_location);
        assert_eq!(a.weather, b.weather);
        assert_eq!(a.text_log.len(), b.text_log.len());
    }

    #[test]
    fn log_appends_to_text_log() {
        let mut world = WorldState::new();
        world.log("hello".to_string());
        world.log("world".to_string());
        assert_eq!(world.text_log, vec!["hello", "world"]);
    }

    #[test]
    fn log_caps_text_log_and_evicts_oldest_entries() {
        let mut world = WorldState::new();

        for i in 0..(MAX_TEXT_LOG + 3) {
            world.log(format!("entry {i}"));
        }

        assert_eq!(world.text_log.len(), MAX_TEXT_LOG);
        assert_eq!(world.text_log.first().map(String::as_str), Some("entry 3"));
        assert_eq!(world.text_log.last().map(String::as_str), Some("entry 502"));
    }

    #[test]
    fn mark_visited_adds_location() {
        let mut world = WorldState::new();
        world.mark_visited(LocationId(42));
        assert!(world.visited_locations.contains(&LocationId(42)));
    }

    #[test]
    fn mark_visited_is_idempotent() {
        let mut world = WorldState::new();
        world.mark_visited(LocationId(5));
        world.mark_visited(LocationId(5));
        assert_eq!(
            world
                .visited_locations
                .iter()
                .filter(|&&id| id == LocationId(5))
                .count(),
            1
        );
    }

    #[test]
    fn record_path_traversal_canonicalises_edge_order() {
        let mut world = WorldState::new();
        // Walk 2 → 1 then 1 → 2 — both should land on the same canonical edge.
        world.record_path_traversal(&[LocationId(2), LocationId(1)]);
        world.record_path_traversal(&[LocationId(1), LocationId(2)]);
        assert_eq!(world.edge_traversals.len(), 1);
        assert_eq!(
            world.edge_traversals.get(&(LocationId(1), LocationId(2))),
            Some(&2)
        );
        // The reversed key should never appear.
        assert!(
            !world
                .edge_traversals
                .contains_key(&(LocationId(2), LocationId(1)))
        );
    }

    #[test]
    fn record_path_traversal_handles_multi_hop_paths() {
        let mut world = WorldState::new();
        // Path A→B→C should register two edges.
        world.record_path_traversal(&[LocationId(1), LocationId(2), LocationId(3)]);
        assert_eq!(
            world.edge_traversals.get(&(LocationId(1), LocationId(2))),
            Some(&1)
        );
        assert_eq!(
            world.edge_traversals.get(&(LocationId(2), LocationId(3))),
            Some(&1)
        );
    }

    #[test]
    fn record_path_traversal_ignores_empty_and_single() {
        let mut world = WorldState::new();
        world.record_path_traversal(&[]);
        world.record_path_traversal(&[LocationId(1)]);
        assert!(world.edge_traversals.is_empty());
    }

    #[test]
    fn current_location_data_none_for_empty_graph() {
        // new() sets up a legacy `locations` map but an empty `graph`.
        let world = WorldState::new();
        assert!(world.current_location_data().is_none());
    }

    #[test]
    #[should_panic(expected = "player location must exist")]
    fn current_location_panics_when_player_location_missing() {
        let mut world = WorldState::new();
        world.player_location = LocationId(999);
        let _ = world.current_location();
    }

    #[test]
    fn increment_tick_generation_increments() {
        let mut world = WorldState::new();
        assert_eq!(world.tick_generation, 0);
        world.increment_tick_generation();
        assert_eq!(world.tick_generation, 1);
    }

    #[test]
    fn increment_tick_generation_wraps_on_overflow() {
        let mut world = WorldState::new();
        world.tick_generation = u64::MAX;
        world.increment_tick_generation();
        assert_eq!(world.tick_generation, 0);
    }

    #[test]
    fn from_parish_file_loads_graph_and_sets_location() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 2, "path_description": "path"}]
                },
                {
                    "id": 2,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "path"}]
                }
            ]
        }"#;
        let path = std::env::temp_dir().join("parish_world_test_from_parish_file.json");
        std::fs::write(&path, json).unwrap();
        let world = WorldState::from_parish_file(&path, LocationId(1)).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(world.player_location, LocationId(1));
        assert!(world.graph.get(LocationId(1)).is_some());
        assert!(world.graph.get(LocationId(2)).is_some());
    }

    #[test]
    fn from_mod_params_parses_start_date() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 2, "path_description": "path"}]
                },
                {
                    "id": 2,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "path"}]
                }
            ]
        }"#;
        let path = std::env::temp_dir().join("parish_world_test_from_mod_params.json");
        std::fs::write(&path, json).unwrap();
        let world =
            WorldState::from_mod_params(&path, LocationId(2), "1820-03-20T08:00:00Z").unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(world.player_location, LocationId(2));
        let now = world.clock.now();
        let expected = chrono::DateTime::parse_from_rfc3339("1820-03-20T08:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let diff = (now - expected).num_seconds().abs();
        assert!(diff < 5, "Clock start date off by {}s", diff);
    }

    #[test]
    fn from_mod_params_fallback_on_bad_date() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 2, "path_description": "path"}]
                },
                {
                    "id": 2,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "path"}]
                }
            ]
        }"#;
        let path = std::env::temp_dir().join("parish_world_test_from_mod_params_bad.json");
        std::fs::write(&path, json).unwrap();
        let world = WorldState::from_mod_params(&path, LocationId(1), "not-a-date").unwrap();
        std::fs::remove_file(&path).unwrap();
        let now = world.clock.now();
        let utc_now = chrono::Utc::now();
        let diff = (now - utc_now).num_seconds().abs();
        assert!(diff < 5, "Fallback date off by {}s", diff);
    }
}
